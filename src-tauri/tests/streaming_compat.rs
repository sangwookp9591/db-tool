//! The fixture is the unchanged protocol codec from d07c12f (v0.2.0).
// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
#[allow(dead_code)]
#[path = "fixtures/protocol_v0_2_0.rs"]
mod old;
use axdb::{plugin_host::PluginDriver, stream::LiveResults};
use dbcore::*;
use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tauri::ipc::{Channel, InvokeResponseBody, Response};
fn config() -> ConnectConfig {
    ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "postgres".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testpw".into()),
        connect_timeout_ms: 3000,
    }
}
fn request() -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":"SELECT i::int8 FROM generate_series(1,3001) i; SELECT 9223372036854775807::int8","row_limit":3000,"read_only":true})).unwrap()
}
#[tokio::test]
async fn old_host_codec_new_driver_keeps_legacy_chunks() {
    let Some(dir) = std::env::var_os("AXDB_DRIVER_DIR") else {
        return;
    };
    let mut child =
        tokio::process::Command::new(std::path::PathBuf::from(dir).join("axdb-driver-postgres"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    old::write_frame(
        &mut input,
        &old::Message::Request {
            id: 1,
            request: old::Request::Handshake { protocol: 1 },
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        old::read_frame(&mut output).await.unwrap(),
        Some(old::Message::Result {
            result: old::Reply::Handshake { protocol: 1, .. },
            ..
        })
    ));
    old::write_frame(
        &mut input,
        &old::Message::Request {
            id: 2,
            request: old::Request::Connect(config()),
        },
    )
    .await
    .unwrap();
    let Some(old::Message::Result {
        result: old::Reply::Connected { session },
        ..
    }) = old::read_frame(&mut output).await.unwrap()
    else {
        panic!("connect reply");
    };
    old::write_frame(
        &mut input,
        &old::Message::Request {
            id: 3,
            request: old::Request::Execute {
                session,
                req: request(),
            },
        },
    )
    .await
    .unwrap();
    let mut results = vec![];
    loop {
        match old::read_frame(&mut output).await.unwrap().unwrap() {
            old::Message::Chunk { chunk, .. } => chunk.append(&mut results).unwrap(),
            old::Message::Result {
                result: old::Reply::Unit,
                ..
            } => break,
            _ => panic!("legacy query failed"),
        }
    }
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row_count, 3000);
    assert!(results[0].truncated);
    assert_eq!(results[0].table.as_ref().unwrap().rows.len(), 3000);
    child.kill().await.unwrap();
    child.wait().await.unwrap();
}
#[tokio::test]
async fn new_host_old_driver_uses_capability_fallback() {
    let Some(binary) = std::env::var_os("AXDB_L2_OLD_DRIVER") else {
        return;
    };
    host_path(binary.into(), false).await;
}
#[tokio::test]
async fn new_host_new_driver_updates_pending_metadata() {
    let Some(dir) = std::env::var_os("AXDB_DRIVER_DIR") else {return;};
    host_path(std::path::PathBuf::from(dir).join("axdb-driver-postgres"), true).await;
}
async fn host_path(binary: std::path::PathBuf, incremental: bool) {
    let driver = PluginDriver::load(binary).await.unwrap();
    assert_eq!(driver.manifest().capabilities.execute_stream, incremental);
    let session = driver.connect(&config()).await.unwrap();
    let received = Arc::new(Mutex::new(0));
    let count = received.clone();
    let channel = Channel::<Response>::new(move |body| {
        let InvokeResponseBody::Raw(bytes) = body else {
            panic!("raw frame")
        };
        let v: serde_json::Value = rmp_serde::from_slice(&bytes).unwrap();
        if v["kind"] == "rows" {
            *count.lock().unwrap() += v["table"]["count"].as_u64().unwrap();
        }
        Ok(())
    });
    let sink = LiveResults::new(channel, CancelToken::new(), 3000);
    let results = session
        .execute_stream(&request(), CancelToken::new(), &sink)
        .await
        .unwrap();
    sink.finish().unwrap();
    assert_eq!(results[0].row_count, 3000);
    assert!(results[0].truncated);
    assert_eq!(*received.lock().unwrap(), 3001);
    session.close().await;
}
