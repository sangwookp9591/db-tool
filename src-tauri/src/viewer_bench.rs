use crate::{storage, CommandResult, Host};
use dbcore::DbError;
#[tauri::command]
pub fn viewer_bench_enabled() -> bool {
    std::env::var("AXDB_BENCH").as_deref() == Ok("1")
}
#[tauri::command]
pub fn viewer_bench_write(
    host: tauri::State<'_, Host>,
    report: serde_json::Value,
) -> CommandResult<()> {
    if !viewer_bench_enabled() {
        return Err(DbError::unsupported("Viewer benchmark is disabled"));
    }
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|_| DbError::internal("Cannot encode benchmark report"))?;
    if bytes.len() > 10_000_000 {
        return Err(DbError::internal("Benchmark report too large"));
    }
    storage::write_private(&host.paths.config.join("perf.json"), &bytes)
}
#[tauri::command]
pub fn viewer_bench_target() -> String {
    let target = std::env::var("AXDB_BENCH_TARGET").unwrap_or_default();
    if viewer_bench_enabled() && matches!(target.as_str(), "graph" | "workspace" | "idle" | "small" | "matrix" | "resource" | "resource-leak") {
        target
    } else {
        "all".into()
    }
}
#[tauri::command]
pub async fn viewer_bench_checkpoint(host: tauri::State<'_, Host>, checkpoint: u32) -> CommandResult<()> {
    if !viewer_bench_enabled() {
        return Err(DbError::unsupported("Viewer benchmark is disabled"));
    }
    let path = host.paths.config.join("resource-ack");
    let until = tokio::time::Instant::now() + std::time::Duration::from_secs(660);
    loop {
        match tokio::fs::read_to_string(&path).await {
            Ok(value) if value.trim().parse::<u32>().ok() == Some(checkpoint) => return Ok(()),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(DbError::internal("Cannot read benchmark acknowledgement")),
        }
        if tokio::time::Instant::now() >= until {
            return Err(DbError::internal("Resource benchmark collector timed out"));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
#[tauri::command]
pub fn viewer_bench_load() -> Option<String> {
    if !viewer_bench_enabled() {
        return None;
    }
    let output = std::process::Command::new("uptime").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
#[tauri::command]
pub fn viewer_bench_config() -> CommandResult<Option<serde_json::Value>> {
    if !viewer_bench_enabled() {
        return Ok(None);
    }
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../bench/fixtures.json"))
            .map_err(|_| DbError::internal("Invalid benchmark fixture"))?;
    if fixture["table"]["fields"]["host"] != "127.0.0.1"
        || fixture["table"]["fields"]["port"] != "55432"
        || fixture["graph"]["fields"]["uri"] != "bolt://127.0.0.1:57687"
    {
        return Err(DbError::unsupported(
            "Benchmark fixtures must use isolated loopback endpoints",
        ));
    }
    Ok(Some(fixture))
}
