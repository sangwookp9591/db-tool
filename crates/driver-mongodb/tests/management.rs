use dbcore::*;
use driver_mongodb::MongoDriver;
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
#[tokio::test]
async fn snapshot_indexes_and_standalone_import_refusal() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        eprintln!("AXDB_TEST_MONGO unset: management integration not run");
        return;
    };
    assert!(uri.contains("@127.0.0.1:57017/"));
    let options = mongodb::options::ClientOptions::parse(&uri).await.unwrap();
    let s = MongoDriver::new()
        .connect(&ConnectConfig {
            fields: std::collections::BTreeMap::from([
                ("uri".into(), "mongodb://127.0.0.1:57017".into()),
                ("user".into(), "axdb".into()),
                ("authSource".into(), "admin".into()),
                ("database".into(), "axdb_test".into()),
            ]),
            secret: options.credential.and_then(|c| c.password),
            connect_timeout_ms: 3000,
        })
        .await
        .unwrap();
    let name = format!("axdb_w3_{}", std::process::id());
    s.execute(
        &req(&serde_json::json!({"create":name}).to_string()),
        CancelToken::new(),
    )
    .await
    .unwrap();
    s.execute(&req(&serde_json::json!({"createIndexes":name,"indexes":[{"key":{"name":1},"name":"by_name","unique":true}]}).to_string()),CancelToken::new()).await.unwrap();
    let snapshot = s.schema_snapshot(None).await.unwrap();
    assert!(snapshot
        .objects
        .iter()
        .any(|o| o.kind == "index" && o.name == format!("{name}.by_name")));
    assert!(snapshot_diff(&snapshot, &snapshot)
        .unwrap()
        .changes
        .is_empty());
    let edit = EditRequest {
        create_table: None,
        node_id: serde_json::to_string(&["collection", "axdb_test", &name]).unwrap(),
        edits: vec![RowEdit::Insert {
            values: vec![("name".into(), Value::Text("test".into()))],
        }],
        timeout_ms: 3000,
        preview_sample_limit: 100,
    };
    let result = s.import_rows(&edit, true, CancelToken::new()).await;
    match result {
        Err(e) => assert_eq!(e.kind, ErrorKind::Unsupported),
        Ok(ImportOutput::Preview(p)) => assert!(!p.committed),
        _ => panic!("Import preview unexpectedly committed"),
    };
    let r = s
        .execute(
            &req(&serde_json::json!({"count":name}).to_string()),
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][0],
        Value::Int("0".into())
    );
    s.execute(
        &req(&serde_json::json!({"drop":name}).to_string()),
        CancelToken::new(),
    )
    .await
    .unwrap();
    s.close().await;
}
