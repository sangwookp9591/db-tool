use dbcore::*;
use driver_postgres::PostgresDriver;
use std::{collections::BTreeMap, sync::Mutex, time::Duration};

fn config() -> Option<ConnectConfig> {
    std::env::var("AXDB_TEST_PG").ok()?;
    Some(ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "postgres".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testpw".into()),
        connect_timeout_ms: 3000,
    })
}
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text,"row_limit":3000,"read_only":true}))
        .unwrap()
}
#[derive(Default)]
struct Collected(Mutex<Vec<ResultSet>>);
#[async_trait::async_trait]
impl AsyncResultSink for Collected {
    async fn start(&self, index: usize, r: &ResultSet) -> Result<(), DbError> {
        let mut results = self.0.lock().unwrap();
        assert_eq!(index, results.len());
        assert_eq!(r.row_count, 0);
        results.push(r.clone());
        Ok(())
    }
    async fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        assert!(rows.len() <= 2000);
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
        Ok(())
    }
}
#[tokio::test]
async fn real_incremental_limits_metadata_precision_and_multi_result() {
    let Some(cfg) = config() else {
        eprintln!("AXDB_TEST_PG unset: streaming DB test not run");
        return;
    };
    let session = PostgresDriver::new().connect(&cfg).await.unwrap();
    for count in [0, 1, 199, 200, 201, 2000, 3000, 3001, 5000] {
        let sink = Collected::default();
        let results=session.execute_stream_raw(&req(&format!("SELECT i::int8, 12345678901234567890.123456789::numeric, '{{\"big\":9223372036854775807}}'::jsonb FROM generate_series(1,{count}) i; SHOW server_version")),CancelToken::new(),&sink).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].row_count, count.min(3000));
        assert_eq!(results[0].truncated, count > 3000);
        assert!(results[0].table.as_ref().unwrap().rows.is_empty());
        let values = sink.0.lock().unwrap();
        if count > 0 {
            let row = &values[0].table.as_ref().unwrap().rows[0];
            assert_eq!(row[0], Value::Int("1".into()));
            assert_eq!(
                row[1],
                Value::Decimal("12345678901234567890.123456789".into())
            );
            assert!(matches!(&row[2],Value::Json(v) if v.contains("9223372036854775807")));
        }
    }
    let bad = session
        .execute_stream_raw(
            &req("SET transaction_read_only=off; SELECT 1"),
            CancelToken::new(),
            &Collected::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(bad.kind, ErrorKind::Unsupported);
    session.close().await;
}
struct Slow;
#[async_trait::async_trait]
impl AsyncResultSink for Slow {
    async fn start(&self, _: usize, _: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
    async fn rows(&self, _: usize, _: Vec<Vec<Value>>) -> Result<(), DbError> {
        tokio::time::sleep(Duration::from_millis(30)).await;
        Ok(())
    }
    async fn end(&self, _: usize, _: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
}
#[tokio::test]
async fn real_incremental_cancel_timeout_and_session_reuse() {
    let Some(cfg) = config() else {
        return;
    };
    let session = PostgresDriver::new().connect(&cfg).await.unwrap();
    for timeout in [false, true] {
        let mut request = req("SELECT i FROM generate_series(1,100000) i");
        request.row_limit = 100000;
        let token = CancelToken::new();
        if timeout {
            request.timeout_ms = 40;
        } else {
            let cancel = token.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(40)).await;
                cancel.cancel();
            });
        }
        let error = session
            .execute_stream_raw(&request, token, &Slow)
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
        assert_eq!(
            session
                .execute(&req("SELECT 1"), CancelToken::new())
                .await
                .unwrap()[0]
                .row_count,
            1
        );
    }
    session.close().await;
}
