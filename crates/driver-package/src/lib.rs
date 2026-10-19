//! Signed .axdriver archives and the signed, single-request driver catalog.
use anyhow::{ensure, Context, Result};
pub use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

pub const REPOSITORY: &str = "sangwookp9591/db-tool";
pub const INDEX_URL: &str =
    "https://github.com/sangwookp9591/db-tool/releases/download/drivers-index/index.json";
pub const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
pub const TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
];

// Official AxDB driver signing key (2026-10-19; private seed lives only in the release Mac's Keychain).
// AXDB_OFFICIAL_DRIVER_PUBLIC_KEY overrides it for tests and key rotation.
pub const OFFICIAL_PUBLIC_KEY: Option<&str> = match option_env!("AXDB_OFFICIAL_DRIVER_PUBLIC_KEY") {
    Some(key) => Some(key),
    None => Some("dcdd779ce6af24db29663761120f24ec902cfbe440c939abc5a23bd0901ee3ad"),
};
pub const OFFICIAL_PREVIOUS_PUBLIC_KEY: Option<&str> =
    option_env!("AXDB_PREVIOUS_DRIVER_PUBLIC_KEY");
pub fn official_keys() -> Vec<TrustedKey> {
    [OFFICIAL_PUBLIC_KEY, OFFICIAL_PREVIOUS_PUBLIC_KEY]
        .into_iter()
        .flatten()
        .filter_map(|key| TrustedKey::from_hex(key).ok())
        .collect()
}

#[derive(Debug, Clone)]
pub struct TrustedKey {
    pub public_key: [u8; 32],
}
impl TrustedKey {
    pub fn from_hex(value: &str) -> Result<Self> {
        let public_key = hex::decode(value)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Public key must be 32 bytes"))?;
        VerifyingKey::from_bytes(&public_key)?;
        Ok(Self { public_key })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetachedSignature {
    pub public_key: String,
    pub signature: String,
}
pub fn sign(data: &[u8], key: &SigningKey) -> DetachedSignature {
    DetachedSignature {
        public_key: hex::encode(key.verifying_key().to_bytes()),
        signature: hex::encode(key.sign(data).to_bytes()),
    }
}
pub fn verify(data: &[u8], signature: &DetachedSignature, trusted: &[TrustedKey]) -> Result<bool> {
    let key = TrustedKey::from_hex(&signature.public_key)?;
    let bytes = hex::decode(&signature.signature)?;
    let signature = Signature::from_slice(&bytes)?;
    VerifyingKey::from_bytes(&key.public_key)?
        .verify_strict(data, &signature)
        .context("Invalid signature")?;
    Ok(trusted
        .iter()
        .any(|known| known.public_key == key.public_key))
}
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SHA-256"
    );
    ensure!(
        sha256(bytes).eq_ignore_ascii_case(expected),
        "SHA-256 mismatch"
    );
    Ok(())
}
pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'),
        "Invalid driver id"
    );
    Ok(())
}
pub fn safe_relative(path: &str) -> Result<&Path> {
    ensure!(
        !path.is_empty() && !path.contains(['\\', ':', '\0']) && !path.starts_with('/'),
        "Unsafe archive path"
    );
    ensure!(
        path.split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".."),
        "Unsafe archive path"
    );
    let value = Path::new(path);
    ensure!(
        value
            .components()
            .all(|p| matches!(p, Component::Normal(_))),
        "Unsafe archive path"
    );
    Ok(value)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub description: String,
    pub paradigm: String,
    pub homepage: String,
    pub license: String,
    pub platforms: BTreeMap<String, Platform>,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        semver::Version::parse(&self.version)?;
        ensure!(
            self.protocol == 1,
            "드라이버 업데이트 필요: unsupported protocol"
        );
        ensure!(
            !self.name.trim().is_empty() && !self.platforms.is_empty(),
            "Incomplete manifest"
        );
        ensure!(
            ["Relational", "Graph", "Document", "KeyValue"].contains(&self.paradigm.as_str()),
            "Invalid paradigm"
        );
        for (target, platform) in &self.platforms {
            ensure!(TARGETS.contains(&target.as_str()), "Unsupported target");
            let path = safe_relative(&platform.path)?;
            ensure!(
                path.parent() == Some(Path::new("bin").join(target).as_path()),
                "Executable must be inside bin/<target>/"
            );
            ensure!(
                platform.sha256.len() == 64
                    && platform.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid executable SHA-256"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub paradigm: String,
    pub homepage: String,
    pub license: String,
    pub platforms: Vec<String>,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub drivers: Vec<CatalogEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCatalog {
    pub catalog: Catalog,
    pub signature: DetachedSignature,
}
impl SignedCatalog {
    pub fn new(catalog: Catalog, key: &SigningKey) -> Result<Self> {
        let signature = sign(&serde_json::to_vec(&catalog)?, key);
        Ok(Self { catalog, signature })
    }
    pub fn verified(self, keys: &[TrustedKey]) -> Result<Catalog> {
        ensure!(
            verify(&serde_json::to_vec(&self.catalog)?, &self.signature, keys)?,
            "Unknown catalog signing key"
        );
        let mut ids = BTreeSet::new();
        for driver in &self.catalog.drivers {
            validate_id(&driver.id)?;
            ensure!(ids.insert(&driver.id), "Duplicate catalog driver");
            semver::Version::parse(&driver.version)?;
            ensure!(
                driver.size > 0 && driver.size <= MAX_PACKAGE_BYTES,
                "Invalid package size"
            );
            ensure!(
                driver.sha256.len() == 64 && driver.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid package SHA-256"
            );
        }
        Ok(self.catalog)
    }
}
pub fn package_url(id: &str, version: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/drivers-{id}-v{version}/{id}-{version}.axdriver")
}

pub fn write_package(
    manifest: &Manifest,
    binaries: &BTreeMap<String, Vec<u8>>,
    key: Option<&SigningKey>,
) -> Result<Vec<u8>> {
    manifest.validate()?;
    ensure!(
        binaries.len() == manifest.platforms.len(),
        "Binary count mismatch"
    );
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let data = serde_json::to_vec(manifest)?;
    zip.start_file("manifest.json", options)?;
    zip.write_all(&data)?;
    if let Some(key) = key {
        zip.start_file("manifest.sig", options)?;
        zip.write_all(&serde_json::to_vec(&sign(&data, key))?)?;
    }
    for (target, platform) in &manifest.platforms {
        let data = binaries.get(target).context("Missing target binary")?;
        verify_sha256(data, &platform.sha256)?;
        zip.start_file(&platform.path, options.unix_permissions(0o755))?;
        zip.write_all(data)?;
    }
    let bytes = zip.finish()?.into_inner();
    ensure!(bytes.len() as u64 <= MAX_PACKAGE_BYTES, "Package too large");
    Ok(bytes)
}

pub struct VerifiedPackage {
    pub manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
    pub trusted: bool,
}
pub fn read_package(
    bytes: &[u8],
    keys: &[TrustedKey],
    allow_unverified: bool,
) -> Result<VerifiedPackage> {
    read_package_cancellable(bytes, keys, allow_unverified, || false)
}
pub fn read_package_cancellable(
    bytes: &[u8],
    keys: &[TrustedKey],
    allow_unverified: bool,
    cancelled: impl Fn() -> bool,
) -> Result<VerifiedPackage> {
    ensure!(!cancelled(), "Driver installation cancelled");
    ensure!(bytes.len() as u64 <= MAX_PACKAGE_BYTES, "Package too large");
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    ensure!(zip.len() <= TARGETS.len() + 2, "Too many archive entries");
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name()?.into_owned();
        safe_relative(&name)?;
        ensure!(
            !file.is_dir()
                && file
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 == 0 || mode & 0o170000 == 0o100000),
            "Archive links and special files are forbidden"
        );
        total = total
            .checked_add(file.size())
            .context("Archive size overflow")?;
        ensure!(total <= MAX_UNPACKED_BYTES, "Unpacked package too large");
        if name == "manifest.json" || name == "manifest.sig" {
            ensure!(file.size() <= 64 * 1024, "Manifest too large");
        }
        ensure!(!files.contains_key(&name), "Duplicate archive path");
        let mut data = Vec::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            ensure!(!cancelled(), "Driver installation cancelled");
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            ensure!(
                (data.len() + count) as u64 <= file.size(),
                "Invalid archive size"
            );
            data.extend_from_slice(&buffer[..count]);
        }
        ensure!(data.len() as u64 == file.size(), "Invalid archive size");
        files.insert(name, data);
    }
    let manifest_bytes = files
        .get("manifest.json")
        .context("Missing manifest.json")?;
    let manifest: Manifest = serde_json::from_slice(manifest_bytes)?;
    manifest.validate()?;
    let trusted = match files.get("manifest.sig") {
        Some(data) => verify(manifest_bytes, &serde_json::from_slice(data)?, keys)?,
        None => false,
    };
    ensure!(
        trusted || allow_unverified,
        "확인되지 않은 드라이버: explicit consent required"
    );
    let allowed: BTreeSet<&str> = manifest
        .platforms
        .values()
        .map(|p| p.path.as_str())
        .chain(["manifest.json", "manifest.sig"])
        .collect();
    ensure!(
        files.keys().all(|path| allowed.contains(path.as_str())),
        "Unexpected archive entry"
    );
    for platform in manifest.platforms.values() {
        ensure!(!cancelled(), "Driver installation cancelled");
        verify_sha256(
            files.get(&platform.path).context("Missing executable")?,
            &platform.sha256,
        )?;
    }
    Ok(VerifiedPackage {
        manifest,
        files,
        trusted,
    })
}
impl VerifiedPackage {
    /// The caller handshakes the renamed candidate, then commits its active marker.
    pub fn stage(&self, drivers: &Path, target: &str) -> Result<StagedInstall> {
        self.stage_cancellable(drivers, target, || false)
    }
    pub fn stage_cancellable(
        &self,
        drivers: &Path,
        target: &str,
        cancelled: impl Fn() -> bool,
    ) -> Result<StagedInstall> {
        ensure!(!cancelled(), "Driver installation cancelled");
        let platform = self
            .manifest
            .platforms
            .get(target)
            .context("No binary for this platform")?;
        std::fs::create_dir_all(drivers)?;
        let parent = drivers.join(&self.manifest.id);
        std::fs::create_dir_all(&parent)?;
        ensure!(
            !std::fs::symlink_metadata(&parent)?.file_type().is_symlink(),
            "Driver directory is a symlink"
        );
        let destination = parent.join(&self.manifest.version);
        ensure!(!destination.exists(), "Driver version already installed");
        let temp = tempfile::Builder::new()
            .prefix(".install-")
            .tempdir_in(&parent)?;
        for name in ["manifest.json", "manifest.sig", platform.path.as_str()] {
            if let Some(data) = self.files.get(name) {
                let path = temp.path().join(safe_relative(name)?);
                std::fs::create_dir_all(path.parent().context("Missing parent")?)?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                for chunk in data.chunks(64 * 1024) {
                    ensure!(!cancelled(), "Driver installation cancelled");
                    file.write_all(chunk)?;
                }
                file.sync_all()?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    file.set_permissions(std::fs::Permissions::from_mode(
                        if name == platform.path { 0o755 } else { 0o644 },
                    ))?;
                }
            }
        }
        ensure!(!cancelled(), "Driver installation cancelled");
        std::fs::rename(temp.path(), &destination)?;
        Ok(StagedInstall {
            executable: destination.join(&platform.path),
            destination,
            committed: false,
        })
    }
}
pub struct StagedInstall {
    pub executable: PathBuf,
    pub destination: PathBuf,
    committed: bool,
}
impl StagedInstall {
    pub fn commit(mut self) {
        self.committed = true;
    }
}
impl Drop for StagedInstall {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.destination);
        }
    }
}

#[cfg(test)]
mod tests;
