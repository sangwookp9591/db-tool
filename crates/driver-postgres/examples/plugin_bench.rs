//! Same query and timer for in-process and protocol paths; isolated PG only.
// Reason: this benchmark binary uses assertions/unwraps for setup and prints benchmark results.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
#[path = "../../../src-tauri/src/plugin_host.rs"]
mod plugin_host;
mod storage {
    pub fn redact(text: &str, secret: Option<&str>) -> String {
        match secret.filter(|s| !s.is_empty()) {
            Some(s) => text.replace(s, "***"),
            None => text.into(),
        }
    }
}
use dbcore::*;
use plugin_host::PluginDriver;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn child_ps() -> Vec<String> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,time=,comm="])
        .output()
        .unwrap();
    assert!(output.status.success(), "ps failed");
    let parent = std::process::id().to_string();
    let processes: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            line.split_whitespace().nth(1) == Some(parent.as_str())
                && line.contains("axdb-driver-postgres")
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(processes.len(), 1, "one live driver process required");
    processes
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let dir = std::env::var_os("AXDB_DRIVER_DIR").expect("AXDB_DRIVER_DIR required");
    let plugin = PluginDriver::load(std::path::PathBuf::from(dir).join(format!(
        "axdb-driver-postgres{}",
        std::env::consts::EXE_SUFFIX
    )))
    .await
    .unwrap();
    let cfg = ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "postgres".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testpw".into()),
        connect_timeout_ms: 3000,
    };
    let direct = driver_postgres::PostgresDriver::new()
        .connect(&cfg)
        .await
        .unwrap();
    let remote = plugin.connect_plugin(&cfg).await.unwrap();
    let connected_idle_ps = child_ps();
    let request: ExecRequest = serde_json::from_value(serde_json::json!({"text":"SELECT i::int8 AS n, repeat('x',32) AS text FROM generate_series(1,100000) i", "row_limit":100000})).unwrap();
    let mut rows = Vec::new();
    for run in 0..8 {
        // Alternate ordering to limit temperature and server-cache bias.
        let mut pair = serde_json::Map::new();
        for plugin_first in if run % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = Instant::now();
            if plugin_first {
                let (results, first) = remote
                    .execute_measured(&request, CancelToken::new())
                    .await
                    .unwrap();
                let total = start.elapsed();
                assert_eq!(results[0].table.as_ref().unwrap().rows.len(), 100000);
                pair.insert(
                    "plugin_first_ms".into(),
                    serde_json::json!(first.as_secs_f64() * 1000.0),
                );
                pair.insert(
                    "plugin_total_ms".into(),
                    serde_json::json!(total.as_secs_f64() * 1000.0),
                );
            } else {
                let results = direct.execute(&request, CancelToken::new()).await.unwrap();
                let total = start.elapsed();
                assert_eq!(results[0].table.as_ref().unwrap().rows.len(), 100000);
                pair.insert(
                    "in_process_ms".into(),
                    serde_json::json!(total.as_secs_f64() * 1000.0),
                );
            }
        }
        if run > 0 {
            rows.push(serde_json::Value::Object(pair));
        }
    }
    let median = |key: &str| {
        let mut values: Vec<f64> = rows.iter().map(|r| r[key].as_f64().unwrap()).collect();
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    let direct_ms = median("in_process_ms");
    let first_ms = median("plugin_first_ms");
    let idle_before = child_ps();
    tokio::time::sleep(Duration::from_secs(5)).await;
    let idle_after = child_ps();
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"rows":100000,"samples":rows,"median_in_process_ms":direct_ms,"median_plugin_first_ms":first_ms,"median_plugin_total_ms":median("plugin_total_ms"),"first_overhead_percent":(first_ms/direct_ms-1.0)*100.0,"first_budget_pass":first_ms<=direct_ms*1.2,"connected_idle_ps":connected_idle_ps,"idle_ps_before":idle_before,"idle_ps_after":idle_after,"idle_sample_seconds":5})).unwrap());
    remote.close().await;
    direct.close().await;
}
