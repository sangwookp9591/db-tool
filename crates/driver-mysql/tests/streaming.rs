// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_mysql::MysqlDriver;
use std::{collections::BTreeMap, sync::Mutex, time::Duration};

fn config(key: &str, port: u16) -> Option<ConnectConfig> {
    std::env::var(key).ok().filter(|v| !v.is_empty())?;
    Some(ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), port.to_string()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "root".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testmysql".into()),
        connect_timeout_ms: 5000,
    })
}
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text,"row_limit":3000,"read_only":true}))
        .unwrap()
}
fn series(count: u32) -> String {
    let digits = "(SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9)";
    format!("SELECT a.n+10*b.n+100*c.n+1000*d.n+10000*e.n AS n FROM {digits} a CROSS JOIN {digits} b CROSS JOIN {digits} c CROSS JOIN {digits} d CROSS JOIN {digits} e WHERE a.n+10*b.n+100*c.n+1000*d.n+10000*e.n < {count}")
}
#[derive(Default)]
struct Collected(Mutex<Vec<ResultSet>>);
#[async_trait::async_trait]
impl AsyncResultSink for Collected {
    async fn start(&self, index: usize, r: &ResultSet) -> Result<(), DbError> {
        let mut results = self.0.lock().unwrap();
        assert_eq!(index, results.len());
        assert_eq!(r.row_count, 0);
        assert!(r.table.as_ref().is_none_or(|t| t.rows.is_empty()));
        results.push(r.clone());
        Ok(())
    }
    async fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        assert!(!rows.is_empty() && rows.len() <= 2000);
        // The byte budget permits a single oversized cell without splitting its value.
        let bytes: usize = rows
            .iter()
            .flatten()
            .map(|v| match v {
                Value::Text(s) => s.len(),
                _ => 0,
            })
            .sum();
        assert!(bytes <= 256 * 1024 || rows.len() == 1);
        self.0.lock().unwrap()[index]
            .table
            .as_mut()
            .unwrap()
            .rows
            .extend(rows);
        Ok(())
    }
    async fn end(&self, index: usize, r: &ResultSet) -> Result<(), DbError> {
        let mut results = self.0.lock().unwrap();
        let target = &mut results[index];
        assert_eq!(
            target.table.as_ref().map_or(0, |t| t.rows.len()) as u64,
            r.row_count
        );
        target.row_count = r.row_count;
        target.truncated = r.truncated;
        target.affected = r.affected;
        target.elapsed_ms = r.elapsed_ms;
        Ok(())
    }
}
async fn contract(key: &str, port: u16) {
    let Some(cfg) = config(key, port) else {
        return;
    };
    let driver = MysqlDriver::new();
    assert!(driver.manifest().capabilities.execute_stream);
    let s = driver.connect(&cfg).await.unwrap();
    for count in [0, 1, 199, 200, 201, 2000, 3000, 3001, 5000] {
        let request = req(&format!("{}; SELECT CAST(18446744073709551615 AS UNSIGNED), CAST('12345678901234567890.123456789' AS DECIMAL(40,9)), JSON_OBJECT('big',CAST(18446744073709551615 AS UNSIGNED))", series(count)));
        let legacy = s.execute(&request, CancelToken::new()).await.unwrap();
        let sink = Collected::default();
        let results = s
            .execute_stream_raw(&request, CancelToken::new(), &sink)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].row_count, count.min(3000) as u64);
        assert_eq!(results[0].truncated, count > 3000);
        assert!(results
            .iter()
            .all(|r| r.table.as_ref().unwrap().rows.is_empty()));
        let received = sink.0.lock().unwrap();
        for (received, result) in received.iter().zip(&results) {
            assert_eq!(received.elapsed_ms, result.elapsed_ms);
        }
        for (a, b) in received.iter().zip(legacy) {
            assert_eq!(a.table, b.table);
        }
        let row = &received[1].table.as_ref().unwrap().rows[0];
        assert_eq!(row[0], Value::Int(u64::MAX.to_string()));
        assert_eq!(
            row[1],
            Value::Decimal("12345678901234567890.123456789".into())
        );
    }
    let mut prepared = req("SELECT CAST(:n AS UNSIGNED), :text");
    prepared.params = serde_json::json!({"n":{"t":"Int","v":"18446744073709551615"},"text":"x'; DROP TABLE absent; --"}).as_object().unwrap().clone();
    let sink = Collected::default();
    s.execute_stream_raw(&prepared, CancelToken::new(), &sink)
        .await
        .unwrap();
    assert_eq!(
        sink.0.lock().unwrap()[0].table.as_ref().unwrap().rows[0][0],
        Value::Int(u64::MAX.to_string())
    );
    for sql in [
        format!("SELECT '{}' FROM ({}) a", "x".repeat(20000), series(201)),
        format!("SELECT '{}'", "x".repeat(300000)),
    ] {
        s.execute_stream_raw(&req(&sql), CancelToken::new(), &Collected::default())
            .await
            .unwrap();
    }
    for sql in [
        "SET TRANSACTION READ WRITE; SELECT 1",
        "DELETE FROM nonexistent",
        "SELECT 1 INTO @x",
        "/*! SET autocommit=0 */ SELECT 1",
    ] {
        assert!(
            s.execute_stream_raw(&req(sql), CancelToken::new(), &Collected::default())
                .await
                .is_err(),
            "{sql}"
        );
    }
    let mut normal = req("SET @axdb_l3=1; SELECT @axdb_l3; DO 1; SELECT 2");
    normal.read_only = false;
    let sink = Collected::default();
    let results = s
        .execute_stream_raw(&normal, CancelToken::new(), &sink)
        .await
        .unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].affected, Some(0));
    assert!(results[0].table.is_none());
    let table = format!("axdb_l3_ts_{}_{}", std::process::id(), port);
    normal.text = format!("CREATE TABLE `{table}` (ts TIMESTAMP(6))");
    s.execute(&normal, CancelToken::new()).await.unwrap();
    normal.text = format!(
        "INSERT INTO `{table}` VALUES ('2026-01-02 03:04:05.123456'); SET time_zone='+09:00'"
    );
    let written = s
        .execute_stream_raw(&normal, CancelToken::new(), &Collected::default())
        .await
        .unwrap();
    assert_eq!(written[0].affected, Some(1));
    assert_eq!(written[0].row_count, 0);
    assert!(written[0].table.is_none());
    for read_only in [false, true] {
        let mut request = req(&format!("SELECT ts FROM `{table}`"));
        request.read_only = read_only;
        let sink = Collected::default();
        s.execute_stream_raw(&request, CancelToken::new(), &sink)
            .await
            .unwrap();
        let received = sink.0.lock().unwrap();
        assert_eq!(received[0].row_count, 1, "streamed write executes once");
        assert_eq!(
            received[0].table.as_ref().unwrap().rows[0][0],
            if read_only {
                Value::DateTimeTz("2026-01-02 03:04:05.123456+00:00".into())
            } else {
                Value::DateTime("2026-01-02 12:04:05.123456".into())
            }
        );
    }
    prepared.read_only = false;
    prepared.params =
        serde_json::json!({"n":{"t":"DateTimeTz","v":"2026-01-02T03:04:05Z"},"text":"instant"})
            .as_object()
            .unwrap()
            .clone();
    assert!(s
        .execute_stream_raw(&prepared, CancelToken::new(), &Collected::default())
        .await
        .is_err());
    normal.text = format!("SET time_zone='+00:00'; DROP TABLE `{table}`; BEGIN");
    s.execute(&normal, CancelToken::new()).await.unwrap();
    assert!(s
        .execute_stream_raw(&req("SELECT 1"), CancelToken::new(), &Collected::default())
        .await
        .is_err());
    normal.text = "ROLLBACK".into();
    s.execute(&normal, CancelToken::new()).await.unwrap();
    normal.text = "SELECT 1; DEFINITELY INVALID".into();
    assert!(s
        .execute_stream_raw(&normal, CancelToken::new(), &Collected::default())
        .await
        .is_err());
    s.ping().await.unwrap();
    s.close().await;
}
struct Blocked;
#[async_trait::async_trait]
impl AsyncResultSink for Blocked {
    async fn start(&self, _: usize, _: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
    async fn rows(&self, _: usize, _: Vec<Vec<Value>>) -> Result<(), DbError> {
        std::future::pending().await
    }
    async fn end(&self, _: usize, _: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
}
struct Failed;
#[async_trait::async_trait]
impl AsyncResultSink for Failed {
    async fn start(&self, _: usize, _: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
    async fn rows(&self, _: usize, _: Vec<Vec<Value>>) -> Result<(), DbError> {
        Err(DbError::query("sink closed"))
    }
    async fn end(&self, _: usize, _: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
}
async fn interruptions(key: &str, port: u16) {
    let Some(cfg) = config(key, port) else {
        return;
    };
    let s = MysqlDriver::new().connect(&cfg).await.unwrap();
    for read_only in [false, true] {
        for server_wait in [false, true] {
            for timeout in [false, true] {
                let mut request = req(&if server_wait {
                    "SELECT SLEEP(10)".into()
                } else {
                    series(100000)
                });
                request.row_limit = 100000;
                request.read_only = read_only;
                let token = CancelToken::new();
                if timeout {
                    request.timeout_ms = 100;
                } else {
                    let token = token.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        token.cancel();
                    });
                }
                let error = tokio::time::timeout(
                    Duration::from_secs(5),
                    s.execute_stream_raw(&request, token, &Blocked),
                )
                .await
                .unwrap()
                .unwrap_err();
                assert_eq!(
                    error.kind,
                    if timeout {
                        ErrorKind::Timeout
                    } else {
                        ErrorKind::Cancelled
                    }
                );
                s.ping().await.unwrap();
                assert_eq!(
                    s.execute(&req("SELECT 1"), CancelToken::new())
                        .await
                        .unwrap()[0]
                        .row_count,
                    1
                );
            }
        }
        let mut request = req(&series(5000));
        request.read_only = read_only;
        let error = s
            .execute_stream_raw(&request, CancelToken::new(), &Failed)
            .await
            .unwrap_err();
        assert_eq!(error.message, "sink closed");
        s.ping().await.unwrap();
    }
    let token = CancelToken::new();
    token.cancel();
    assert_eq!(
        s.execute_stream_raw(&req("SELECT 1"), token, &Collected::default())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Cancelled
    );
    s.close().await;
}
#[tokio::test]
async fn mysql_incremental_contract() {
    contract("AXDB_TEST_MYSQL", 53306).await;
}
#[tokio::test]
async fn mariadb_incremental_contract() {
    contract("AXDB_TEST_MARIADB", 53307).await;
}
#[tokio::test]
async fn mysql_incremental_interruptions() {
    interruptions("AXDB_TEST_MYSQL", 53306).await;
}
#[tokio::test]
async fn mariadb_incremental_interruptions() {
    interruptions("AXDB_TEST_MARIADB", 53307).await;
}
