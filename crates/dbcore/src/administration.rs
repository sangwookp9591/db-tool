//! Native administrative contracts. Streamed backups have bounded chunks, not bounded archives.
use crate::*;
use std::io::Read;

// Legacy single-frame APIs remain bounded for protocol-1 compatibility.
pub const BACKUP_MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupData {
    pub format: String,
    #[serde(with = "backup_bytes")]
    pub data: Vec<u8>,
    pub counts: Vec<(String, u64)>,
}
impl BackupData {
    pub fn validate(&self, format: &str) -> Result<(), DbError> {
        if self.format != format || self.data.len() > BACKUP_MAX_BYTES || self.counts.len() > 50000
        {
            return Err(DbError::query(
                "Unsupported backup format or size limit exceeded",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleInfo {
    pub name: String,
    pub login: bool,
    pub superuser: bool,
    pub member_of: Vec<String>,
    pub summary: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectPrivilege {
    pub role: String,
    pub namespace: String,
    pub object: String,
    pub privilege: String,
    pub allowed: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RolesSnapshot {
    pub supported: bool,
    pub reason: Option<String>,
    pub roles: Vec<RoleInfo>,
    pub matrix: Vec<ObjectPrivilege>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GrantChange {
    pub role: String,
    pub namespace: String,
    pub object: String,
    pub privilege: String,
    pub revoke: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrantPlan {
    pub statements: Vec<String>,
    pub warnings: Vec<String>,
    pub apply_supported: bool,
}
pub fn validate_grants(changes: &[GrantChange]) -> Result<(), DbError> {
    if changes.is_empty()
        || changes.len() > 100
        || changes.iter().any(|c| {
            [&c.role, &c.namespace, &c.object, &c.privilege]
                .iter()
                .any(|s| s.len() > 255 || s.contains('\0'))
                || c.role.is_empty()
        })
    {
        return Err(DbError::query("Grant plan requires 1..100 valid changes"));
    }
    Ok(())
}
#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn progress(&self, completed: u64, total: u64) -> Result<(), DbError>;
}

mod backup_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() > super::BACKUP_MAX_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("Backup exceeds payload limit"));
        }
        STANDARD
            .decode(value)
            .map_err(|_| serde::de::Error::custom("Invalid backup base64"))
    }
}

/// Byte chunks use the existing protocol-1 chunk envelope; archive size is not a frame size.
pub const BACKUP_CHUNK_BYTES: usize = 64 * 1024;
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BackupSummary {
    pub format: String,
    pub counts: Vec<(String, u64)>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
#[async_trait]
pub trait BackupSink: Send + Sync {
    async fn write(&self, bytes: &[u8]) -> Result<(), DbError>;
}
pub async fn backup_record(sink: &dyn BackupSink, tag: u8, bytes: &[u8]) -> Result<(), DbError> {
    if bytes.len() > 128 * 1024 * 1024 { return Err(DbError::query("Backup record exceeds 128 MiB")); }
    let mut header = [0u8; 5]; header[0] = tag;
    header[1..].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
    sink.write(&header).await?;
    for chunk in bytes.chunks(BACKUP_CHUNK_BYTES) { sink.write(chunk).await?; }
    Ok(())
}
pub fn read_backup_record(reader: &mut (dyn std::io::Read + Send)) -> Result<Option<(u8, Vec<u8>)>, DbError> {
    let mut tag = [0u8; 1];
    if reader.read(&mut tag).map_err(|_|DbError::query("Cannot read backup"))? == 0 { return Ok(None); }
    let mut len = [0u8; 4];
    reader.read_exact(&mut len).map_err(|_|DbError::query("Truncated backup record"))?;
    let len = u32::from_be_bytes(len) as usize;
    if len > 128 * 1024 * 1024 { return Err(DbError::query("Backup record exceeds 128 MiB")); }
    let mut bytes = vec![0;len];
    reader.read_exact(&mut bytes).map_err(|_|DbError::query("Truncated backup record"))?;
    Ok(Some((tag[0], bytes)))
}

#[derive(Debug,Clone,Default,Serialize,Deserialize,PartialEq)]
pub struct RestoreOptions {pub overwrite:bool,pub new_database:bool,pub guidance:String}
pub fn validate_restore_database(name:&str)->Result<(),DbError>{
    if name.is_empty()||name.len()>63||!name.as_bytes()[0].is_ascii_alphabetic()||!name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'||b==b'-'){return Err(DbError::query("Database name requires 1..63 ASCII letters, digits, underscores or hyphens, starting with a letter"));}Ok(())
}

pub fn legacy_backup(summary:&BackupSummary,source:&mut (dyn std::io::Read+Send))->Result<BackupData,DbError>{
    let mut data=vec![];source.take(BACKUP_MAX_BYTES as u64+1).read_to_end(&mut data).map_err(|_|DbError::query("Cannot read legacy backup"))?;
    let backup=BackupData{format:summary.format.clone(),data,counts:summary.counts.clone()};backup.validate(&summary.format)?;Ok(backup)
}
