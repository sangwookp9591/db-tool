// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_neo4j::Neo4jDriver;
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
#[tokio::test]
async fn import_rollback_commit_and_schema() {
    let Ok(value) = std::env::var("AXDB_TEST_NEO4J") else {
        eprintln!("AXDB_TEST_NEO4J unset: management integration not run");
        return;
    };
    let parts = value.split('|').collect::<Vec<_>>();
    assert_eq!(parts[0], "bolt://127.0.0.1:57687");
    let s = Neo4jDriver::new()
        .connect(&ConnectConfig {
            fields: std::collections::BTreeMap::from([
                ("uri".into(), parts[0].into()),
                ("user".into(), parts[1].into()),
            ]),
            secret: Some(parts[2].into()),
            connect_timeout_ms: 3000,
        })
        .await
        .unwrap();
    let label = format!("AxdbW3{}", std::process::id());
    let edit = EditRequest {originals: vec![],
        create_table: None,
        node_id: serde_json::to_string(&["label", &label]).unwrap(),
        edits: vec![RowEdit::Insert {
            values: vec![("name".into(), Value::Text("test".into()))],
        }],
        timeout_ms: 30000,
        preview_sample_limit: 100,
    };
    let count = || async {
        s.execute(
            &req(&format!("MATCH (n:{label}) RETURN count(n) AS count")),
            CancelToken::new(),
        )
        .await
        .unwrap()[0]
            .table
            .as_ref()
            .unwrap()
            .rows[0][0]
            .clone()
    };
    s.import_rows(&edit, true, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(count().await, Value::Int("0".into()));
    s.import_rows(&edit, false, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(count().await, Value::Int("1".into()));
    let snapshot = s.schema_snapshot(None).await.unwrap();
    assert!(snapshot
        .objects
        .iter()
        .any(|o| o.kind == "label" && o.name == label));
    assert!(snapshot_diff(&snapshot, &snapshot)
        .unwrap()
        .changes
        .is_empty());
    s.execute(
        &req(&format!("MATCH (n:{label}) DELETE n")),
        CancelToken::new(),
    )
    .await
    .unwrap();
    s.close().await;
}
