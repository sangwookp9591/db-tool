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
    if viewer_bench_enabled() && matches!(target.as_str(), "qa" | "graph" | "workspace" | "idle" | "small" | "matrix" | "resource" | "resource-leak" | "connections" | "d7" | "d9" | "d10") {
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
            Ok(value) if value.trim() == "abort" => return Err(DbError::internal("Benchmark collector stopped")),
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

pub(crate) struct Span {
    name: &'static str,
    since: std::time::Instant,
    start_ms: u64,
}
pub(crate) fn span(name: &'static str) -> Option<Span> {
    viewer_bench_enabled().then(|| Span {
        name,
        since: std::time::Instant::now(),
        start_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
    })
}
impl Drop for Span {
    fn drop(&mut self) {
        tracing::info!(bench_name = self.name, bench_start_ms = self.start_ms, bench_elapsed_ms = self.since.elapsed().as_secs_f64() * 1000.0, "AXDB_BENCH_TIME");
    }
}
#[cfg(test)]
mod timing_tests {
    use super::*;
    #[test]
    fn records_drop_on_error_without_logging_values() {
        let dir = std::env::temp_dir().join(format!("axdb-bench-timing-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("log");
        let log = storage::open_private_log(&path).unwrap();
        let result: Result<(), DbError> = tracing::subscriber::with_default(crate::file_logger(log), || {
            let _first = Span { name: "fixture-command", since: std::time::Instant::now(), start_ms: 1 };
            { let _second = Span { name: "fixture-keychain", since: std::time::Instant::now(), start_ms: 2 }; }
            Err(DbError::internal("private-row-value"))
        });
        assert!(result.is_err());
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().filter(|line| line.contains("AXDB_BENCH_TIME")).count(), 2);
        assert!(text.contains("fixture-command") && text.contains("fixture-keychain"));
        assert!(!text.contains("private-row-value"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
