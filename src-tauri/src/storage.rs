use dbcore::{DbError, DriverManifest};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub driver_id: String,
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub save_secret: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistorySource {
    #[default]
    Ui,
    Agent,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(default)]
    pub source: HistorySource,
    pub timestamp: String,
    pub profile_name: String,
    pub text: String,
    pub elapsed_ms: u64,
    pub row_count: u64,
    pub error: Option<String>,
}
#[derive(Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub log: PathBuf,
}
impl Paths {
    pub fn create(config: PathBuf, data: PathBuf, log: PathBuf) -> Result<Self, DbError> {
        let paths = if let Some(dir) = std::env::var_os("DBTOOL_CONFIG_DIR") {
            let dir = PathBuf::from(dir);
            Self {
                config: dir.clone(),
                data: dir.clone(),
                log: dir,
            }
        } else {
            Self { config, data, log }
        };
        for dir in [&paths.config, &paths.data, &paths.log] {
            std::fs::create_dir_all(dir)
                .map_err(|_| DbError::internal("Cannot create app directory"))?;
        }
        Ok(paths)
    }
}
pub fn strip_secrets(profile: &mut Profile, manifest: &DriverManifest) -> Option<String> {
    let mut secret = None;
    for field in manifest.connection_fields.iter().filter(|f| f.secret) {
        if let Some(value) = profile.fields.remove(&field.key) {
            if !value.is_empty() {
                secret = Some(value);
            }
        }
    }
    secret
}
pub fn redact(text: &str, secret: Option<&str>) -> String {
    match secret.filter(|s| !s.is_empty()) {
        Some(s) => text.replace(s, "***"),
        None => text.into(),
    }
}
pub fn read_profiles(path: &Path) -> Result<Vec<Profile>, DbError> {
    match std::fs::read(path) {
        Ok(data) => {
            serde_json::from_slice(&data).map_err(|_| DbError::internal("Invalid profiles file"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err(DbError::internal("Cannot read profiles file")),
    }
}
pub fn read_history(path: &Path) -> Result<Vec<HistoryEntry>, DbError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(DbError::internal("Cannot read history file")),
    };
    text.lines()
        .map(|line| {
            serde_json::from_str(line).map_err(|_| DbError::internal("Invalid history file"))
        })
        .collect()
}
pub fn open_private_log(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

pub fn write_private(path: &Path, data: &[u8]) -> Result<(), DbError> {
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|_| DbError::internal("Cannot create app file"))?;
        file.write_all(data)
            .and_then(|_| file.sync_all())
            .map_err(|_| DbError::internal("Cannot write app file"))?;
        std::fs::rename(&temp, path).map_err(|_| DbError::internal("Cannot replace app file"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
pub fn keychain_get(id: &str) -> Result<Option<String>, DbError> {
    let entry = keyring::Entry::new("db-tool", &format!("profile:{id}"))
        .map_err(|_| DbError::internal("Keychain unavailable"))?;
    match entry.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(DbError::internal("Cannot read keychain credential")),
    }
}
pub fn keychain_set(id: &str, value: Option<&str>) -> Result<(), DbError> {
    let entry = keyring::Entry::new("db-tool", &format!("profile:{id}"))
        .map_err(|_| DbError::internal("Keychain unavailable"))?;
    match value {
        Some(value) => entry
            .set_password(value)
            .map_err(|_| DbError::internal("Cannot store keychain credential")),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(DbError::internal("Cannot remove keychain credential")),
        },
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn log_permissions_are_private_for_new_and_existing_files() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("dbtool-log-mode-{}", uuid::Uuid::new_v4()));
        drop(super::open_private_log(&path).unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        drop(super::open_private_log(&path).unwrap());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_file(path).unwrap();
    }

    use super::*;
    #[test]
    fn profile_secrets_and_atomic_files() {
        let driver = crate::drivers::registry().manifests().remove(0);
        let mut profile = Profile {
            id: "p".into(),
            name: "local".into(),
            driver_id: driver.id.clone(),
            fields: BTreeMap::from([
                ("password".into(), "private-value".into()),
                ("host".into(), "127.0.0.1".into()),
            ]),
            save_secret: false,
        };
        assert_eq!(
            strip_secrets(&mut profile, &driver),
            Some("private-value".into())
        );
        let data = serde_json::to_vec(&profile).unwrap();
        assert!(!String::from_utf8_lossy(&data).contains("private-value"));
        let dir = std::env::temp_dir().join(format!("dbtool-storage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profiles.json");
        write_private(&path, &serde_json::to_vec(&vec![profile]).unwrap()).unwrap();
        assert_eq!(read_profiles(&path).unwrap().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            redact("SELECT 'private-value'", Some("private-value")),
            "SELECT '***'"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
