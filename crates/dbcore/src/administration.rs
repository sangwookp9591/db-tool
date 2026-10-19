//! Bounded native administrative payloads. No credentials are included in these contracts.
use crate::*;

// shortcut: protocol 1 carries one bounded payload; use chunked transfer before supporting larger backups.
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
