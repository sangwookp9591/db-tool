use crate::{agent_bridge, storage, DbError, Host};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize)]
pub struct Preferences {
    auto_check: bool,
    skipped_version: Option<String>,
}

fn preferences(settings: &serde_json::Value) -> Preferences {
    Preferences {
        auto_check: settings["updater"]["auto_check"].as_bool().unwrap_or(true),
        skipped_version: settings["updater"]["skipped_version"]
            .as_str()
            .map(str::to_owned),
    }
}

fn automatic_allowed(debug: bool, bench: bool, bridge: bool) -> bool {
    !debug && !bench && !bridge
}

#[tauri::command]
pub async fn updater_settings(host: tauri::State<'_, Host>) -> Result<serde_json::Value, DbError> {
    let _guard = host.storage.lock().await;
    let settings = agent_bridge::read_settings(&host.paths.config)?;
    Ok(json!({
        "preferences": preferences(&settings),
        "automatic": automatic_allowed(cfg!(debug_assertions), std::env::var_os("AXDB_BENCH").is_some(), std::env::var("AXDB_AGENT_BRIDGE").as_deref() == Ok("1")),
        "current_version": env!("CARGO_PKG_VERSION")
    }))
}

fn save(config: &std::path::Path, preferences: Preferences) -> Result<(), DbError> {
    let mut settings = agent_bridge::read_settings(config)?;
    settings["updater"] = json!(preferences);
    storage::write_private(
        &config.join("settings.json"),
        &serde_json::to_vec(&settings)
            .map_err(|_| DbError::internal("Cannot encode update settings"))?,
    )
}

#[tauri::command]
pub async fn updater_configure(
    host: tauri::State<'_, Host>,
    preferences: Preferences,
) -> Result<(), DbError> {
    let _guard = host.storage.lock().await;
    save(&host.paths.config, preferences)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automation_is_never_automatic() {
        assert!(automatic_allowed(false, false, false));
        for (debug, bench, bridge) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert!(!automatic_allowed(debug, bench, bridge));
        }
    }
    #[test]
    fn preferences_preserve_other_settings_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        assert!(preferences(&json!({})).auto_check);
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"agent_bridge":true,"onboarding_version":1}"#).unwrap();
        save(
            dir.path(),
            Preferences {
                auto_check: false,
                skipped_version: Some("0.3.0".into()),
            },
        )
        .unwrap();
        let settings = agent_bridge::read_settings(dir.path()).unwrap();
        assert_eq!(settings["agent_bridge"], true);
        assert_eq!(settings["onboarding_version"], 1);
        assert!(!preferences(&settings).auto_check);
        assert_eq!(
            preferences(&settings).skipped_version.as_deref(),
            Some("0.3.0")
        );
        std::fs::write(&path, b"broken").unwrap();
        assert!(save(
            dir.path(),
            Preferences {
                auto_check: true,
                skipped_version: None
            }
        )
        .is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"broken");
    }
}
