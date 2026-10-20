use dbcore::*;
use driver_postgres::PostgresDriver;
fn req(text: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":text})).unwrap()
}
#[tokio::test]
async fn atomic_import_snapshot_and_migration() {
    let Ok(dsn) = std::env::var("AXDB_TEST_PG") else {
        eprintln!("AXDB_TEST_PG unset: management integration not run");
        return;
    };
    let pg: tokio_postgres::Config = dsn.parse().unwrap();
    assert_eq!(
        pg.get_hosts(),
        &[tokio_postgres::config::Host::Tcp("127.0.0.1".into())]
    );
    assert_eq!(pg.get_ports(), &[55432]);
    assert_eq!(pg.get_dbname(), Some("axdb_test"));
    let cfg = ConnectConfig {
        fields: std::collections::BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), pg.get_user().unwrap().into()),
            ("sslmode".into(), "prefer".into()),
        ]),
        secret: pg
            .get_password()
            .map(|p| String::from_utf8(p.to_vec()).unwrap()),
        connect_timeout_ms: 2000,
    };
    let s = PostgresDriver::new().connect(&cfg).await.unwrap();
    let name = format!("axdb_w3_{}", std::process::id());
    s.execute(&req(&format!("CREATE SCHEMA {name}; CREATE TABLE {name}.items (id integer PRIMARY KEY, name text UNIQUE NOT NULL); CREATE INDEX item_name ON {name}.items(name); CREATE VIEW {name}.names AS SELECT name FROM {name}.items")),CancelToken::new()).await.unwrap();
    let make = |names: Vec<&str>| EditRequest {originals: vec![],
        create_table: None,
        node_id: serde_json::to_string(&["table", &name, "tables", "items"]).unwrap(),
        edits: names
            .into_iter()
            .enumerate()
            .map(|(i, n)| RowEdit::Insert {
                values: vec![
                    ("id".into(), Value::Int((i + 1).to_string())),
                    ("name".into(), Value::Text(n.into())),
                ],
            })
            .collect(),
        timeout_ms: 30000,
        preview_sample_limit: 100,
    };
    let count = || async {
        s.execute(
            &req(&format!("SELECT count(*) FROM {name}.items")),
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
    s.import_rows(&make(vec!["a", "b"]), true, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(count().await, Value::Int("0".into()));
    assert!(s
        .import_rows(&make(vec!["a", "a"]), false, CancelToken::new())
        .await
        .is_err());
    assert_eq!(count().await, Value::Int("0".into()));
    s.import_rows(&make(vec!["a", "b"]), false, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(count().await, Value::Int("2".into()));
    s.execute(&req("BEGIN"), CancelToken::new()).await.unwrap();
    assert!(matches!(
        s.import_rows(&make(vec!["pending"]), false, CancelToken::new())
            .await,
        Err(DbError {
            kind: ErrorKind::Unsupported,
            ..
        })
    ));
    s.execute(&req("ROLLBACK"), CancelToken::new())
        .await
        .unwrap();
    let snapshot = s.schema_snapshot(Some(&name)).await.unwrap();
    for kind in ["table", "index", "constraint", "view"] {
        assert!(
            snapshot.objects.iter().any(|o| o.kind == kind),
            "missing {kind}"
        );
    }
    let mut desired = snapshot.clone();
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
        .ddl_render(&snapshot_diff(&snapshot, &desired).unwrap())
        .await
        .unwrap()
        .join(";\n");
    s.preview(&req(&ddl), CancelToken::new()).await.unwrap();
    assert!(!s
        .schema_snapshot(Some(&name))
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
        .schema_snapshot(Some(&name))
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
            &req(&format!(
                "CREATE TABLE {name}.must_rollback(id int); INSERT INTO {name}.missing VALUES (1)"
            )),
            CancelToken::new()
        )
        .await
        .is_err());
    assert!(!s
        .schema_snapshot(Some(&name))
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "must_rollback"));
    s.apply_migration(
        &req(&format!("CREATE TABLE {name}.migrated(id int)")),
        CancelToken::new(),
    )
    .await
    .unwrap();
    assert!(s
        .schema_snapshot(Some(&name))
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "migrated"));
    let fresh = EditRequest {originals: vec![],
        create_table: Some(vec![Column {
            name: "value".into(),
            type_name: "integer".into(),
        }]),
        node_id: serde_json::to_string(&["table", &name, "tables", "new_import"]).unwrap(),
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
        .schema_snapshot(Some(&name))
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
        .schema_snapshot(Some(&name))
        .await
        .unwrap()
        .objects
        .iter()
        .any(|o| o.name == "new_import"));
    assert!(s
        .import_rows(&fresh, false, CancelToken::new())
        .await
        .is_err());
    s.execute(
        &req(&format!("DROP SCHEMA {name} CASCADE")),
        CancelToken::new(),
    )
    .await
    .unwrap();
    s.close().await;
}
