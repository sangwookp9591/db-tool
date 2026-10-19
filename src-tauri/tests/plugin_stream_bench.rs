//! Manual paired timing at the real host -> UI Channel serialization boundary.
use axdb::{
    plugin_host::PluginDriver,
    stream::LiveResults,
};
use dbcore::*;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::ipc::{Channel, InvokeResponseBody, Response};

#[derive(Deserialize)]
struct EventSummary {
    kind: String,
    table: Option<Count>,
}
#[derive(Deserialize)]
struct Count {
    count: usize,
}

async fn measure(session: &dyn Session, req: &ExecRequest) -> serde_json::Value {
    let started = Instant::now();
    let received = Arc::new(Mutex::new((None::<Duration>, 0usize, false)));
    let events = received.clone();
    let channel = Channel::<Response>::new(move |body| {
        let InvokeResponseBody::Raw(bytes) = body else {
            panic!("expected raw MessagePack event")
        };
        let event: EventSummary = rmp_serde::from_slice(&bytes).unwrap();
        let mut state = events.lock().unwrap();
        if event.kind == "rows" {
            state.0.get_or_insert_with(|| started.elapsed());
            state.1 += event.table.unwrap().count;
        }
        if event.kind == "done" {
            state.2 = true;
        }
        Ok(())
    });
    let sink = LiveResults::new(channel, CancelToken::new(), req.row_limit);
    let results = session
        .execute_stream(req, CancelToken::new(), &sink)
        .await
        .unwrap();
    assert_eq!(results[0].row_count, 100000);
    assert!(
        results[0].table.as_ref().unwrap().rows.is_empty(),
        "streamed typed rows are not retained"
    );
    sink.finish().unwrap();
    let total = started.elapsed();
    let state = received.lock().unwrap();
    assert_eq!(state.1, 100000);
    assert!(state.2);
    serde_json::json!({"first_ui_channel_ms":state.0.unwrap().as_secs_f64()*1000.0,"total_ms":total.as_secs_f64()*1000.0})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual performance measurement requires isolated PostgreSQL and built plugin"]
async fn paired_ui_channel_benchmark() {
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
    let remote = plugin.connect(&cfg).await.unwrap();
    let req:ExecRequest=serde_json::from_value(serde_json::json!({"text":"SELECT i::int8 AS n, repeat('x',32) AS text FROM generate_series(1,100000) i","row_limit":100000})).unwrap();
    let mut samples = Vec::new();
    for index in 0..8 {
        let (a, b) = if index % 2 == 0 {
            (measure(&*direct, &req).await, measure(&*remote, &req).await)
        } else {
            let b = measure(&*remote, &req).await;
            let a = measure(&*direct, &req).await;
            (a, b)
        };
        if index > 0 {
            samples.push(serde_json::json!({"in_process":a,"plugin":b}));
        }
    }
    let median = |path: &str, metric: &str| {
        let mut values: Vec<f64> = samples
            .iter()
            .map(|s| s[path][metric].as_f64().unwrap())
            .collect();
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    let first_direct = median("in_process", "first_ui_channel_ms");
    let first_plugin = median("plugin", "first_ui_channel_ms");
    let total_direct = median("in_process", "total_ms");
    let total_plugin = median("plugin", "total_ms");
    let report = serde_json::json!({"profile":"debug","boundary":"actual host LiveResults -> Tauri Channel; excludes browser paint","rows":100000,"pairs":samples,"first_in_process_ms":first_direct,"first_plugin_ms":first_plugin,"first_overhead_percent":(first_plugin/first_direct-1.0)*100.0,"first_budget_pass":first_plugin<=first_direct*1.2,"total_in_process_ms":total_direct,"total_plugin_ms":total_plugin,"total_overhead_percent":(total_plugin/total_direct-1.0)*100.0,"total_budget_pass":total_plugin<=total_direct*1.2});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bench/results-p1-plugin-stream.json");
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    direct.close().await;
    remote.close().await;
}
