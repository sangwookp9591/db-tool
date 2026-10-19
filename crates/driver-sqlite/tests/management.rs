use dbcore::*;
use driver_sqlite::SqliteDriver;
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
#[tokio::test]
async fn atomic_import_snapshot_and_migration() {
    let dir = tempfile::tempdir().unwrap();
    let driver = SqliteDriver::new();
    let s = driver
        .connect(&ConnectConfig {
            fields: std::collections::BTreeMap::from([(
                "path".into(),
                dir.path()
                    .join("management.db")
                    .to_string_lossy()
                    .into_owned(),
            )]),
            secret: None,
            connect_timeout_ms: 1000,
        })
        .await
        .unwrap();
    s.execute(&req("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL); CREATE INDEX item_name ON items(name); CREATE VIEW names AS SELECT name FROM items"),CancelToken::new()).await.unwrap();
    let make = |names: Vec<&str>| EditRequest {
        create_table: None,
        node_id: "table:items".into(),
        edits: names
            .into_iter()
            .map(|n| RowEdit::Insert {
                values: vec![("name".into(), Value::Text(n.into()))],
            })
            .collect(),
        timeout_ms: 30000,
        preview_sample_limit: 100,
    };
    let changes = make(vec!["a", "b"]);
    let p = s
        .import_rows(&changes, true, CancelToken::new())
        .await
        .unwrap();
    assert!(matches!(
        p,
        ImportOutput::Preview(ChangePreview {
            committed: false,
            ..
        })
    ));
    let count = || async {
        s.execute(&req("SELECT count(*) FROM items"), CancelToken::new())
            .await
            .unwrap()[0]
            .table
            .as_ref()
            .unwrap()
            .rows[0][0]
            .clone()
    };
    assert_eq!(count().await, Value::Int("0".into()));
    assert!(s
        .import_rows(&make(vec!["a", "a"]), false, CancelToken::new())
        .await
        .is_err());
    assert_eq!(count().await, Value::Int("0".into()));
    s.import_rows(&changes, false, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(count().await, Value::Int("2".into()));
    let snap = s.schema_snapshot(None).await.unwrap();
    assert!(snap
        .objects
        .iter()
        .any(|o| o.kind == "index" && o.name == "item_name"));
    assert!(snap.objects.iter().any(|o| o.kind == "view"));
    assert!(snapshot_diff(&snap, &snap).unwrap().changes.is_empty());
    let mut desired = snap.clone();
    desired
        .objects
        .iter_mut()
        .find(|o| o.kind == "table" && o.name == "items")
        .unwrap()
        .columns
        .push(SnapshotColumn {
            name: "extra".into(),
            type_name: "text".into(),
            nullable: true,
            default: None,
        });
    let ddl = s
        .ddl_render(&snapshot_diff(&snap, &desired).unwrap())
        .await
        .unwrap()
        .join(";\n");
    s.preview(&req(&ddl), CancelToken::new()).await.unwrap();
    assert!(!s
        .schema_snapshot(None)
        .await
        .unwrap()
        .objects
        .iter()
        .find(|o| o.name == "items" && o.kind == "table")
        .unwrap()
        .columns
        .iter()
        .any(|c| c.name == "extra"));
    s.apply_migration(&req(&ddl), CancelToken::new())
        .await
        .unwrap();
    assert!(s
        .schema_snapshot(None)
        .await
        .unwrap()
        .objects
        .iter()
        .find(|o| o.name == "items" && o.kind == "table")
        .unwrap()
        .columns
        .iter()
        .any(|c| c.name == "extra"));
    assert!(s
        .apply_migration(
            &req("CREATE TABLE must_rollback(id INTEGER); INSERT INTO missing VALUES (1)"),
            CancelToken::new()
        )
        .await
        .is_err());
    assert!(!s
        .schema_snapshot(None)
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "must_rollback"));
    s.apply_migration(
        &req("CREATE TABLE migrated (id INTEGER)"),
        CancelToken::new(),
    )
    .await
    .unwrap();
    assert!(s
        .schema_snapshot(None)
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "migrated"));
    assert!(s
        .apply_migration(&req("COMMIT; DROP TABLE items"), CancelToken::new())
        .await
        .is_err());
    let token = CancelToken::new();
    token.cancel();
    assert!(s.import_rows(&make(vec!["c"]), false, token).await.is_err());
    assert_eq!(count().await, Value::Int("2".into()));
    let fresh = EditRequest {
        create_table: Some(vec![Column {
            name: "value".into(),
            type_name: "integer".into(),
        }]),
        node_id: "table:new_import".into(),
        edits: vec![RowEdit::Insert {
            values: vec![("value".into(), Value::Int("7".into()))],
        }],
        timeout_ms: 5000,
        preview_sample_limit: 100,
    };
    s.import_rows(&fresh, true, CancelToken::new())
        .await
        .unwrap();
    assert!(!s
        .schema_snapshot(None)
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "new_import"));
    let applied = s
        .import_rows(&fresh, false, CancelToken::new())
        .await
        .unwrap();
    let ImportOutput::Applied(applied) = applied else {
        panic!("Expected applied import")
    };
    assert_eq!(applied.affected.len(), applied.statements.len());
    assert!(s
        .schema_snapshot(None)
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "new_import"));
    assert!(s
        .import_rows(&fresh, false, CancelToken::new())
        .await
        .is_err());
    s.close().await;
}
