// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_neo4j::Neo4jDriver;
use std::collections::BTreeMap;
#[tokio::test]
async fn explain_and_profile_return_plans_and_rollback() {
    let Ok(connection) = std::env::var("AXDB_TEST_NEO4J") else {
        return;
    };
    let parts: Vec<_> = connection.split('|').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], "bolt://127.0.0.1:57687");
    let s = Neo4jDriver::new()
        .connect(&ConnectConfig {
            fields: BTreeMap::from([
                ("uri".into(), "bolt://127.0.0.1:57687".into()),
                ("user".into(), "neo4j".into()),
            ]),
            secret: Some(parts[2].into()),
            connect_timeout_ms: 5000,
        })
        .await
        .unwrap();
    let p = s
        .explain("MATCH (n) RETURN count(n)", false, CancelToken::new())
        .await
        .unwrap();
    assert!(!p.op.is_empty());
    assert!(!p.children.is_empty());
    let p = s
        .explain(
            "CREATE (n:W2ExplainOnly) RETURN n",
            true,
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert!(!p.children.is_empty());
    let req = serde_json::from_value(
        serde_json::json!({"text":"MATCH (n:W2ExplainOnly) RETURN count(n) AS count"}),
    )
    .unwrap();
    let r = s.execute(&req, CancelToken::new()).await.unwrap();
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][0],
        Value::Int("0".into())
    );
    assert!(s
        .explain("CALL dbms.components()", true, CancelToken::new())
        .await
        .is_err());
    s.close().await;
}
