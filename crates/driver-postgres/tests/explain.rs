// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_postgres::PostgresDriver;
use std::collections::BTreeMap;
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
#[tokio::test]
async fn explain_analyze_rolls_back_writes_and_recovers_from_errors() {
    let Ok(dsn) = std::env::var("AXDB_TEST_PG") else {
        return;
    };
    let pg: tokio_postgres::Config = dsn.parse().expect("valid test DSN");
    assert_eq!(pg.get_ports(), &[55432]);
    assert_eq!(
        pg.get_hosts(),
        &[tokio_postgres::config::Host::Tcp("127.0.0.1".into())]
    );
    let config = ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("user".into(), "postgres".into()),
            ("database".into(), "axdb_test".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: pg
            .get_password()
            .map(|p| String::from_utf8(p.to_vec()).unwrap()),
        connect_timeout_ms: 5000,
    };
    let s = PostgresDriver::new().connect(&config).await.unwrap();
    s.execute(
        &req("CREATE TEMP TABLE w2_explain(id integer); INSERT INTO w2_explain VALUES(1)"),
        CancelToken::new(),
    )
    .await
    .unwrap();
    let plan = s
        .explain("SELECT * FROM w2_explain", false, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(plan.op, "Seq Scan");
    assert!(plan.est_rows.is_some());
    s.execute(
        &req("CREATE TEMP TABLE w2_large AS SELECT generate_series(1, 10001) AS id; ANALYZE w2_large"),
        CancelToken::new(),
    ).await.unwrap();
    let scan = s
        .explain(
            "SELECT * FROM w2_large WHERE id < 0",
            false,
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert!(scan.est_rows.is_some_and(|rows| rows < 10000.0));
    assert!(scan
        .warnings
        .iter()
        .any(|w| w.starts_with("Large table full scan:")));
    s.explain("UPDATE w2_explain SET id=2", true, CancelToken::new())
        .await
        .unwrap();
    let r = s
        .execute(&req("SELECT id FROM w2_explain"), CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][0],
        Value::Int("1".into())
    );
    assert!(s
        .explain("SELECT * FROM no_such_w2_table", true, CancelToken::new())
        .await
        .is_err());
    assert!(s
        .explain("SELECT 1; DELETE FROM w2_explain", true, CancelToken::new())
        .await
        .is_err());
    s.ping().await.unwrap();
    s.execute(
        &req("BEGIN; UPDATE w2_explain SET id=3"),
        CancelToken::new(),
    )
    .await
    .unwrap();
    s.explain("UPDATE w2_explain SET id=4", true, CancelToken::new())
        .await
        .unwrap();
    let r = s
        .execute(&req("SELECT id FROM w2_explain"), CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][0],
        Value::Int("3".into())
    );
    s.execute(&req("ROLLBACK"), CancelToken::new())
        .await
        .unwrap();
    let token = CancelToken::new();
    let work = s.explain("SELECT pg_sleep(10)", true, token.clone());
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        token.cancel();
    };
    let (result, ()) = tokio::join!(work, cancel);
    assert!(matches!(result.unwrap_err().kind, ErrorKind::Cancelled));
    s.ping().await.unwrap();
    let token = CancelToken::new();
    token.cancel();
    assert!(s.explain("SELECT 1", true, token).await.is_err());
    s.close().await;
}
