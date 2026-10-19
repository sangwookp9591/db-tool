use crate::{agent_bridge, storage, DbError, Host};
use serde_json::{json, Value};

const VERSION: u64 = 1;

#[tauri::command]
pub async fn onboarding_status(host: tauri::State<'_, Host>) -> Result<Value, DbError> {
    let _guard = host.storage.lock().await;
    let settings = agent_bridge::read_settings(&host.paths.config)?;
    // Isolated automation must reach the workspace without dismissing a first-run modal.
    let automation = std::env::var_os("AXDB_CONFIG_DIR").is_some()
        || std::env::var("AXDB_BENCH").as_deref() == Ok("1")
        || std::env::var("AXDB_AGENT_BRIDGE").as_deref() == Ok("1");
    Ok(
        json!({"version": settings["onboarding_version"].as_u64().unwrap_or(0), "automatic": !automation}),
    )
}

fn complete(config: &std::path::Path) -> Result<(), DbError> {
    let mut settings = agent_bridge::read_settings(config)?;
    let version = settings["onboarding_version"]
        .as_u64()
        .unwrap_or(0)
        .max(VERSION);
    settings["onboarding_version"] = json!(version);
    storage::write_private(
        &config.join("settings.json"),
        &serde_json::to_vec(&settings).map_err(|_| DbError::internal("Cannot encode settings"))?,
    )
}

#[tauri::command]
pub async fn onboarding_complete(host: tauri::State<'_, Host>) -> Result<(), DbError> {
    let _guard = host.storage.lock().await;
    complete(&host.paths.config)
}

#[tauri::command]
pub async fn onboarding_mcp_config(
    bridge: tauri::State<'_, agent_bridge::Bridge>,
) -> Result<Value, DbError> {
    bridge.client_config().await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_preserves_settings_and_rejects_corruption() {
        let path = std::env::temp_dir().join(format!("axdb-onboarding-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        complete(&path).unwrap();
        assert_eq!(
            agent_bridge::read_settings(&path).unwrap()["onboarding_version"],
            1
        );
        std::fs::write(
            path.join("settings.json"),
            br#"{"agent_bridge":true,"other":{"keep":42},"onboarding_version":3}"#,
        )
        .unwrap();
        complete(&path).unwrap();
        assert_eq!(
            agent_bridge::read_settings(&path).unwrap(),
            json!({"agent_bridge":true,"other":{"keep":42},"onboarding_version":3})
        );
        std::fs::write(path.join("settings.json"), b"broken").unwrap();
        assert!(complete(&path).is_err());
        assert_eq!(
            std::fs::read(path.join("settings.json")).unwrap(),
            b"broken"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn stopped_bridge_never_returns_config() {
        assert!(agent_bridge::Bridge::default()
            .client_config()
            .await
            .is_err());
    }
}
