use crate::{agent_bridge, storage, DbError, Host};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
const DAY: u64 = 86_400;
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct Star {
    first_seen: u64,
    successful_queries: u64,
    shown_at: Option<u64>,
    dismissed: bool,
    snooze_until: Option<u64>,
    shown_count: u8,
    starred_clicked: bool,
}
fn now() -> u64 { time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64 }
fn load(settings: &Value, timestamp: u64) -> Result<Star, DbError> {
    let mut star: Star = serde_json::from_value(settings.get("star").cloned().unwrap_or(json!({})))
        .map_err(|_| DbError::internal("Invalid star preferences"))?;
    if star.first_seen == 0 { star.first_seen = timestamp; }
    Ok(star)
}
fn eligible(star: &Star, timestamp: u64, automatic: bool) -> bool {
    automatic && !star.dismissed && star.successful_queries >= 20
        && timestamp.saturating_sub(star.first_seen) >= 3 * DAY && star.shown_count < 2
        && if star.shown_count == 0 { true } else { star.snooze_until.is_some_and(|until| timestamp >= until) }
}
fn save(config: &std::path::Path, mut settings: Value, star: Star) -> Result<(), DbError> {
    settings["star"] = json!(star);
    storage::write_private(&config.join("settings.json"), &serde_json::to_vec(&settings).map_err(|_| DbError::internal("Cannot encode star preferences"))?)
}
pub(crate) fn successful_query(config: &std::path::Path) -> Result<(), DbError> {
    let settings = agent_bridge::read_settings(config)?;
    let mut star = load(&settings, now())?;
    star.successful_queries = star.successful_queries.saturating_add(1);
    save(config, settings, star)
}
#[tauri::command]
pub async fn star_prompt(host: tauri::State<'_, Host>, action: String) -> Result<Value, DbError> {
    let _guard = host.storage.lock().await;
    let settings = agent_bridge::read_settings(&host.paths.config)?;
    let timestamp = now();
    let mut star = load(&settings, timestamp)?;
    let automatic = !cfg!(debug_assertions) && std::env::var_os("AXDB_CONFIG_DIR").is_none()
        && std::env::var_os("AXDB_BENCH").is_none() && std::env::var_os("AXDB_AGENT_BRIDGE").is_none();
    let allowed = eligible(&star, timestamp, automatic);
    match action.as_str() {
        "status" => (),
        "shown" if allowed => { star.shown_at = Some(timestamp); star.shown_count += 1; star.snooze_until = None; },
        "later" => star.snooze_until = Some(timestamp + 14 * DAY),
        "dismiss" => star.dismissed = true,
        "star" => {star.dismissed = true;star.starred_clicked=true;},
        "shown" => return Ok(json!({"eligible":false})),
        _ => return Err(DbError::query("Unknown star action")),
    }
    save(&host.paths.config, settings, star)?;
    Ok(json!({"eligible": allowed}))
}
#[tauri::command]
pub async fn star_count(host: tauri::State<'_, Host>) -> Result<Option<u64>, DbError> {
    let _guard=host.storage.lock().await;
    let mut settings=agent_bridge::read_settings(&host.paths.config)?;
    let timestamp=now();
    if timestamp.saturating_sub(settings["star_count"]["fetched_at"].as_u64().unwrap_or(0))<DAY {
        return Ok(settings["star_count"]["count"].as_u64());
    }
    let fetched=async {
        let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).build().ok()?;
        let mut response=client.get("https://api.github.com/repos/sangwookp9591/db-tool").header("user-agent","AxDB").header("accept","application/vnd.github+json").send().await.ok()?;
        if !response.status().is_success(){return None;}
        let mut bytes=Vec::new();
        while let Some(chunk)=response.chunk().await.ok()? {if bytes.len()+chunk.len()>64_000{return None;}bytes.extend_from_slice(&chunk);}
        let value:Value=serde_json::from_slice(&bytes).ok()?;
        value["stargazers_count"].as_u64().filter(|count|*count<=1_000_000_000)
    }.await;
    settings["star_count"]=json!({"fetched_at":timestamp,"count":fetched});
    storage::write_private(&host.paths.config.join("settings.json"),&serde_json::to_vec(&settings).map_err(|_|DbError::internal("Cannot encode star count cache"))?)?;
    Ok(fetched)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn click_snooze_and_cached_count_preserve_other_settings_without_network() {
        use tauri::Manager;
        let dir=tempfile::tempdir().unwrap();
        let host=Host::new(storage::Paths{config:dir.path().into(),data:dir.path().into(),log:dir.path().into()});
        let app=tauri::test::mock_builder().manage(host).build(tauri::test::mock_context(tauri::test::noop_assets())).unwrap();
        storage::write_private(&dir.path().join("settings.json"),br#"{"other":42}"#).unwrap();
        star_prompt(app.state(),"later".into()).await.unwrap();
        let settings=agent_bridge::read_settings(dir.path()).unwrap();
        assert!(settings["star"]["snooze_until"].as_u64().unwrap()>=now()+14*DAY-1);
        star_prompt(app.state(),"star".into()).await.unwrap();
        let mut settings=agent_bridge::read_settings(dir.path()).unwrap();
        assert_eq!(settings["star"]["starred_clicked"],true);assert_eq!(settings["star"]["dismissed"],true);assert_eq!(settings["other"],42);
        settings["star_count"]=json!({"fetched_at":now(),"count":123});
        storage::write_private(&dir.path().join("settings.json"),&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(star_count(app.state()).await.unwrap(),Some(123));
        assert!(!star_prompt(app.state(),"status".into()).await.unwrap()["eligible"].as_bool().unwrap());
    }
    #[test]
    fn usage_age_snooze_and_two_prompt_limit() {
        let mut s = Star {first_seen:1, successful_queries:19, ..Default::default()};
        let t = 1 + 3 * DAY;
        assert!(!eligible(&s,t,true)); s.successful_queries=20;
        assert!(!eligible(&s,t-1,true)); assert!(eligible(&s,t,true)); assert!(!eligible(&s,t,false));
        s.shown_count=1; s.shown_at=Some(t); assert!(!eligible(&s,t+99*DAY,true));
        s.snooze_until=Some(t+14*DAY); assert!(!eligible(&s,t+14*DAY-1,true)); assert!(eligible(&s,t+14*DAY,true));
        s.shown_count=2; assert!(!eligible(&s,t+30*DAY,true)); s.shown_count=0;s.dismissed=true; assert!(!eligible(&s,t,true));
    }
    #[test]
    fn usage_preserves_settings_and_rejects_corruption() {
        let dir=tempfile::tempdir().unwrap();
        storage::write_private(&dir.path().join("settings.json"),br#"{"other":42}"#).unwrap();
        successful_query(dir.path()).unwrap();
        let settings=agent_bridge::read_settings(dir.path()).unwrap();
        assert_eq!(settings["other"],42); assert_eq!(settings["star"]["successful_queries"],1);
        storage::write_private(&dir.path().join("settings.json"),b"broken").unwrap();
        assert!(successful_query(dir.path()).is_err());
    }
}
