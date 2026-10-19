use dbcore::*;
use driver_sqlite::SqliteDriver;
use std::collections::BTreeMap;
#[tokio::test]
async fn query_plan_is_a_tree_and_never_executes_dml() {
    let s = SqliteDriver::new()
        .connect(&ConnectConfig {
            fields: BTreeMap::from([("path".into(), ":memory:".into())]),
            secret: None,
            connect_timeout_ms: 1000,
        })
        .await
        .unwrap();
    let req=serde_json::from_value(serde_json::json!({"text":"CREATE TABLE example(id integer); INSERT INTO example VALUES(1)"})).unwrap();
    s.execute(&req, CancelToken::new()).await.unwrap();
    let p = s
        .explain("SELECT * FROM example", false, CancelToken::new())
        .await
        .unwrap();
    assert!(!p.children.is_empty());
    assert!(p.children[0].op.contains("SCAN"));
    assert!(s
        .explain("DELETE FROM example; SELECT 1", false, CancelToken::new())
        .await
        .is_err());
    assert!(s
        .explain("SELECT * FROM example", true, CancelToken::new())
        .await
        .is_err());
    let p = s
        .explain("UPDATE example SET id=2", false, CancelToken::new())
        .await
        .unwrap();
    assert!(!p.children.is_empty());
    let req = serde_json::from_value(serde_json::json!({"text":"SELECT id FROM example"})).unwrap();
    assert_eq!(
        s.execute(&req, CancelToken::new()).await.unwrap()[0]
            .table
            .as_ref()
            .unwrap()
            .rows[0][0],
        Value::Int("1".into())
    );
}
