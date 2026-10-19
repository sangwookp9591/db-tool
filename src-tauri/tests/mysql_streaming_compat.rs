//! Exercise MySQL's optional stream through PluginDriver and the unchanged 0.2.0 codec.
#[allow(dead_code)]
#[path = "fixtures/protocol_v0_2_0.rs"]
mod old;
use axdb::plugin_host::PluginDriver;
use dbcore::*;
use std::{collections::BTreeMap, process::Stdio, sync::Mutex};

fn config(port: u16) -> ConnectConfig {
    ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), port.to_string()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "root".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testmysql".into()),
        connect_timeout_ms: 5000,
    }
}
fn request() -> ExecRequest {
    let digits = "(SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9)";
    serde_json::from_value(serde_json::json!({"text":format!("SELECT a.n+10*b.n+100*c.n+1000*d.n AS n FROM {digits} a CROSS JOIN {digits} b CROSS JOIN {digits} c CROSS JOIN {digits} d LIMIT 3001; SELECT CAST(18446744073709551615 AS UNSIGNED)"),"row_limit":3000,"read_only":true})).unwrap()
}
#[derive(Default)]
struct Events(Mutex<(Vec<bool>, Vec<usize>, Vec<u64>)>);
impl ResultSink for Events {
    fn start_pending(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        assert_eq!(result.row_count, 0);
        let mut state = self.0.lock().unwrap();
        assert_eq!(index, state.0.len());
        state.0.push(true);
        state.1.push(0);
        Ok(())
    }
    fn start(&self, index: usize, _: &ResultSet) -> Result<(), DbError> {
        let mut state = self.0.lock().unwrap();
        assert_eq!(index, state.0.len());
        state.0.push(false);
        state.1.push(0);
        Ok(())
    }
    fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        self.0.lock().unwrap().1[index] += rows.len();
        Ok(())
    }
    fn end(&self, _: usize, result: &ResultSet) -> Result<(), DbError> {
        self.0.lock().unwrap().2.push(result.row_count);
        Ok(())
    }
}
async fn contract(key: &str, port: u16) {
    if std::env::var_os(key).is_none() {
        return;
    }
    let Some(dir) = std::env::var_os("AXDB_DRIVER_DIR") else {
        return;
    };
    let binary = std::path::PathBuf::from(dir).join("axdb-driver-mysql");
    let driver = PluginDriver::load(binary.clone()).await.unwrap();
    assert!(driver.manifest().capabilities.execute_stream);
    let session = driver.connect(&config(port)).await.unwrap();
    let sink = Events::default();
    let results = session
        .execute_stream(&request(), CancelToken::new(), &sink)
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row_count, 3000);
    assert!(results[0].truncated);
    assert!(results
        .iter()
        .all(|r| r.table.as_ref().unwrap().rows.is_empty()));
    assert_eq!(
        *sink.0.lock().unwrap(),
        (vec![true, true], vec![3000, 1], vec![3000, 1])
    );
    for timeout in [false, true] {
        let mut waiting = request();
        waiting.text = "SELECT SLEEP(10)".into();
        let cancel = CancelToken::new();
        if timeout {
            waiting.timeout_ms = 100;
        } else {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                cancel.cancel();
            });
        }
        let error = session
            .execute_stream(&waiting, cancel, &Events::default())
            .await
            .unwrap_err();
        assert_eq!(
            error.kind,
            if timeout {
                ErrorKind::Timeout
            } else {
                ErrorKind::Cancelled
            }
        );
        session.ping().await.unwrap();
    }
    let mut small = request();
    small.text = "SELECT 1".into();
    small.row_limit = 1000;
    assert_eq!(
        session
            .execute_stream(&small, CancelToken::new(), &Events::default())
            .await
            .unwrap()[0]
            .table
            .as_ref()
            .unwrap()
            .rows
            .len(),
        1,
        "small requests retain legacy direct results"
    );
    session.close().await;

    let mut child = tokio::process::Command::new(binary)
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
            request: old::Request::Connect(config(port)),
        },
    )
    .await
    .unwrap();
    let Some(old::Message::Result {
        result: old::Reply::Connected { session },
        ..
    }) = old::read_frame(&mut output).await.unwrap()
    else {
        panic!("connect reply")
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
            _ => panic!("legacy execution failed"),
        }
    }
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].row_count, 3000);
    assert!(results[0].truncated);
    assert_eq!(results[0].table.as_ref().unwrap().rows.len(), 3000);
    assert_eq!(
        results[1].table.as_ref().unwrap().rows[0][0],
        Value::Int(u64::MAX.to_string())
    );
    child.kill().await.unwrap();
    child.wait().await.unwrap();
}
#[tokio::test]
async fn mysql_plugin_stream_and_old_host_codec() {
    contract("AXDB_TEST_MYSQL", 53306).await;
}
#[tokio::test]
async fn mariadb_plugin_stream_and_old_host_codec() {
    contract("AXDB_TEST_MARIADB", 53307).await;
}

#[tokio::test]
async fn new_host_keeps_pre_stream_mysql_driver_fallback() {
    let Some(binary) = std::env::var_os("AXDB_L3_OLD_DRIVER") else {
        return;
    };
    for (key, port) in [("AXDB_TEST_MYSQL", 53306), ("AXDB_TEST_MARIADB", 53307)] {
        if std::env::var_os(key).is_none() {
            continue;
        }
        let driver = PluginDriver::load(binary.clone().into()).await.unwrap();
        assert!(!driver.manifest().capabilities.execute_stream);
        let session = driver.connect(&config(port)).await.unwrap();
        let sink = Events::default();
        let results = session
            .execute_stream(&request(), CancelToken::new(), &sink)
            .await
            .unwrap();
        assert_eq!(results[0].row_count, 3000);
        assert!(results[0].truncated);
        {
            let received = sink.0.lock().unwrap();
            assert_eq!(received.0, vec![false, false]);
            assert_eq!(received.1, vec![3000, 1]);
        }
        session.close().await;
    }
}
