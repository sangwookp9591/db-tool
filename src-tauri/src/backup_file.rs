//! File-backed backup archives: zstd bytes followed by a checked manifest trailer.
use dbcore::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Mutex,
};
pub const MAGIC: &[u8] = b"AXBACKUP\x02";
const MANIFEST_LIMIT: u64 = 512 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub format: String,
    pub sha256: String,
    pub bytes: u64,
    pub counts: Vec<(String, u64)>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
impl Manifest {
    pub fn summary(&self) -> BackupSummary {
        BackupSummary {
            format: self.format.clone(),
            counts: self.counts.clone(),
            warnings: self.warnings.clone(),
        }
    }
}
struct State {
    writer: zstd::stream::write::Encoder<'static, BufWriter<File>>,
    hash: Sha256,
    bytes: u64,
}
pub struct ArchiveWriter {
    state: Mutex<State>,
}
fn io_error(_: std::io::Error) -> DbError {
    DbError::query("Backup file I/O failed; check free disk space and permissions")
}
impl ArchiveWriter {
    pub fn new(mut file: File) -> Result<Self, DbError> {
        file.write_all(MAGIC).map_err(io_error)?;
        let writer =
            zstd::stream::write::Encoder::new(BufWriter::new(file), 3).map_err(io_error)?;
        Ok(Self {
            state: Mutex::new(State {
                writer,
                hash: Sha256::new(),
                bytes: 0,
            }),
        })
    }
    pub fn finish(self, summary: BackupSummary) -> Result<Manifest, DbError> {
        if summary.format.is_empty() || summary.counts.len() > 50000 {
            return Err(DbError::query("Invalid backup summary"));
        }
        let state = self
            .state
            .into_inner()
            .map_err(|_| DbError::internal("Backup writer lock unavailable"))?;
        let manifest = Manifest {
            version: 2,
            format: summary.format,
            sha256: format!("{:x}", state.hash.finalize()),
            bytes: state.bytes,
            counts: summary.counts,
            warnings: summary.warnings,
        };
        let json =
            serde_json::to_vec(&manifest).map_err(|_| DbError::query("Invalid backup manifest"))?;
        if json.len() as u64 > MANIFEST_LIMIT {
            return Err(DbError::query("Backup manifest exceeds 512 KiB"));
        }
        let mut writer = state.writer.finish().map_err(io_error)?;
        writer.write_all(&json).map_err(io_error)?;
        writer
            .write_all(&(json.len() as u64).to_be_bytes())
            .map_err(io_error)?;
        writer.write_all(MAGIC).map_err(io_error)?;
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)?;
        Ok(manifest)
    }
}
#[async_trait::async_trait]
impl BackupSink for ArchiveWriter {
    async fn write(&self, bytes: &[u8]) -> Result<(), DbError> {
        if bytes.len() > BACKUP_CHUNK_BYTES {
            return Err(DbError::query("Backup chunk exceeds 64 KiB"));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| DbError::internal("Backup writer lock unavailable"))?;
        state.writer.write_all(bytes).map_err(io_error)?;
        state.hash.update(bytes);
        state.bytes = state
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| DbError::query("Backup size overflow"))?;
        Ok(())
    }
}
/// Failure or cancellation drops the private temporary file and leaves any previous destination intact.
pub async fn create(
    session: &dyn Session,
    path: &Path,
    cancel: CancelToken,
    progress: &dyn ProgressSink,
) -> Result<Manifest, DbError> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
    {
        return Err(DbError::query("Backup target must be a regular file"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| DbError::query("Backup destination has no parent"))?;
    let file = tempfile::Builder::new()
        .prefix(".axdb-backup-")
        .tempfile_in(parent)
        .map_err(io_error)?;
    let writer = ArchiveWriter::new(file.reopen().map_err(io_error)?)?;
    struct Output<'a> {
        writer: &'a ArchiveWriter,
        progress: &'a dyn ProgressSink,
        cancel: CancelToken,
        bytes: std::sync::atomic::AtomicU64,
    }
    #[async_trait::async_trait]
    impl BackupSink for Output<'_> {
        async fn write(&self, bytes: &[u8]) -> Result<(), DbError> {
            if self.cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "Backup cancelled"));
            }
            self.writer.write(bytes).await?;
            let total = self
                .bytes
                .fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed)
                + bytes.len() as u64;
            self.progress.progress(total, 0).await
        }
    }
    let output = Output {
        writer: &writer,
        progress,
        cancel: cancel.clone(),
        bytes: std::sync::atomic::AtomicU64::new(0),
    };
    let summary = session.backup_stream(cancel.clone(), &output).await?;
    if cancel.is_cancelled() {
        return Err(DbError::new(ErrorKind::Cancelled, "Backup cancelled"));
    }
    let manifest = writer.finish(summary)?;
    if cancel.is_cancelled() {
        return Err(DbError::new(ErrorKind::Cancelled, "Backup cancelled"));
    }
    if path
        .symlink_metadata()
        .is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
    {
        return Err(DbError::query("Backup destination changed"));
    }
    file.persist(path).map_err(|e| io_error(e.error))?;
    Ok(manifest)
}
pub fn open_selected(path: &Path) -> Result<File, DbError> {
    let before = path.symlink_metadata().map_err(io_error)?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(DbError::query("Backup input must be a regular file"));
    }
    let file = File::open(path).map_err(io_error)?;
    let after = path.symlink_metadata().map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata().map_err(io_error)?;
        if after.dev() != opened.dev() || after.ino() != opened.ino() {
            return Err(DbError::query("Backup input changed while opening"));
        }
    }
    if !after.is_file() || after.file_type().is_symlink() {
        return Err(DbError::query("Backup input changed while opening"));
    }
    Ok(file)
}
pub struct Staged {
    pub manifest: Manifest,
    pub payload: File,
}
/// Freeze and verify the entire payload before granting a preview receipt or sending any DB writes.
pub fn stage(file: &mut File, cancel: &CancelToken) -> Result<Staged, DbError> {
    let length = file.metadata().map_err(io_error)?.len();
    let footer = 8 + MAGIC.len() as u64;
    if length < (MAGIC.len() * 2) as u64 + 8 {
        return Err(DbError::query("Invalid .axbackup file"));
    }
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut magic = vec![0; MAGIC.len()];
    file.read_exact(&mut magic).map_err(io_error)?;
    let (manifest, payload_start, payload_end) = if magic == b"AXBACKUP\x01" {
        let mut len = [0; 4];
        file.read_exact(&mut len).map_err(io_error)?;
        let len = u32::from_be_bytes(len) as u64;
        if len > MANIFEST_LIMIT || MAGIC.len() as u64 + 4 + len >= length {
            return Err(DbError::query("Invalid legacy backup manifest"));
        }
        let mut json = vec![0; len as usize];
        file.read_exact(&mut json).map_err(io_error)?;
        let manifest: Manifest = serde_json::from_slice(&json)
            .map_err(|_| DbError::query("Invalid legacy backup manifest"))?;
        if manifest.version != 1 || manifest.bytes > BACKUP_MAX_BYTES as u64 {
            return Err(DbError::query("Invalid legacy backup version or size"));
        }
        (manifest, MAGIC.len() as u64 + 4 + len, length)
    } else if magic == MAGIC {
        file.seek(SeekFrom::End(-(footer as i64)))
            .map_err(io_error)?;
        let mut len = [0; 8];
        file.read_exact(&mut len).map_err(io_error)?;
        file.read_exact(&mut magic).map_err(io_error)?;
        let len = u64::from_be_bytes(len);
        if magic != MAGIC || len > MANIFEST_LIMIT || length <= MAGIC.len() as u64 + footer + len {
            return Err(DbError::query("Invalid backup trailer"));
        }
        let payload_end = length - footer - len;
        file.seek(SeekFrom::Start(payload_end)).map_err(io_error)?;
        let mut json = vec![0; len as usize];
        file.read_exact(&mut json).map_err(io_error)?;
        let manifest: Manifest =
            serde_json::from_slice(&json).map_err(|_| DbError::query("Invalid backup manifest"))?;
        if manifest.version != 2 {
            return Err(DbError::query("Unsupported backup version"));
        }
        (manifest, MAGIC.len() as u64, payload_end)
    } else {
        return Err(DbError::query("Unsupported .axbackup file"));
    };
    if manifest.format.is_empty() || manifest.counts.len() > 50000 {
        return Err(DbError::query("Unsupported backup manifest"));
    }
    file.seek(SeekFrom::Start(payload_start))
        .map_err(io_error)?;
    let mut decoder =
        zstd::stream::read::Decoder::new(BufReader::new(file.take(payload_end - payload_start)))
            .map_err(io_error)?;
    let mut payload = tempfile::tempfile().map_err(io_error)?;
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    let mut chunk = vec![0; BACKUP_CHUNK_BYTES];
    loop {
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "Backup verification cancelled",
            ));
        }
        let len = decoder.read(&mut chunk).map_err(io_error)?;
        if len == 0 {
            break;
        }
        bytes = bytes
            .checked_add(len as u64)
            .ok_or_else(|| DbError::query("Backup length overflow"))?;
        if bytes > manifest.bytes {
            return Err(DbError::query(
                "Backup decompressed length exceeds manifest",
            ));
        }
        hash.update(&chunk[..len]);
        payload.write_all(&chunk[..len]).map_err(io_error)?;
    }
    if bytes != manifest.bytes || format!("{:x}", hash.finalize()) != manifest.sha256 {
        return Err(DbError::query("Backup checksum or length mismatch"));
    }
    payload.seek(SeekFrom::Start(0)).map_err(io_error)?;
    Ok(Staged { manifest, payload })
}
