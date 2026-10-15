use crate::{storage, CommandResult, Host};
use dbcore::DbError;
#[tauri::command]
pub fn viewer_bench_enabled() -> bool {
    std::env::var("DBTOOL_BENCH").as_deref() == Ok("1")
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
    let target = std::env::var("DBTOOL_BENCH_TARGET").unwrap_or_default();
    if viewer_bench_enabled() && matches!(target.as_str(), "graph" | "workspace" | "idle") {
        target
    } else {
        "all".into()
    }
}
