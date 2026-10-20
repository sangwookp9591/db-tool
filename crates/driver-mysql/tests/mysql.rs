use dbcore::*;
use driver_mysql::MysqlDriver;
use std::{collections::BTreeMap, sync::Mutex, time::Duration};
fn req(sql: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":sql})).unwrap()
}
fn cfg(key: &str, port: u16) -> Option<ConnectConfig> {
    if std::env::var(key).ok()?.is_empty() {
        return None;
    }
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
async fn run(s: &dyn Session, sql: &str) -> Vec<ResultSet> {
    s.execute(&req(sql), CancelToken::new()).await.unwrap()
}
fn scalar(r: &[ResultSet]) -> &Value {
    &r[0].table.as_ref().unwrap().rows[0][0]
}
fn edit(node: &str, edits: Vec<RowEdit>) -> EditRequest {
    EditRequest {originals: vec![],
        node_id: node.into(),
        create_table: None,
        edits,
        timeout_ms: 5000,
        preview_sample_limit: 10,
    }
}
struct Bytes(Mutex<Vec<u8>>);
#[async_trait::async_trait]
impl BackupSink for Bytes {
    async fn write(&self, b: &[u8]) -> Result<(), DbError> {
        assert!(b.len() <= BACKUP_CHUNK_BYTES);
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(())
    }
}
async fn contract(key: &str, port: u16) {
    let Some(mut config) = cfg(key, port) else {
        eprintln!("{key} unset: real database contract skipped");
        return;
    };
    let driver = MysqlDriver::new();
    assert_eq!(driver.manifest().id, "mysql");
    let admin = driver.connect(&config).await.unwrap();
    let db = format!(
        "axdb_m1_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    run(&*admin, &format!("CREATE DATABASE `{db}`")).await;
    config.fields.insert("database".into(), db.clone());
    let s = driver.connect(&config).await.unwrap();
    s.ping().await.unwrap();
    assert!(!s.server_info().await.unwrap().is_empty());
    run(&*s,"CREATE TABLE types(id BIGINT UNSIGNED PRIMARY KEY, d DECIMAL(40,20), dt DATETIME(6), ts TIMESTAMP(6) NULL, j JSON, b BLOB, bits BIT(64), e ENUM('a','b'), st SET('x','y'), n VARCHAR(100)) ENGINE=InnoDB; INSERT INTO types VALUES(18446744073709551615,12345678901234567890.12345678901234567890,'2026-01-02 03:04:05.123456','2026-01-02 03:04:05.123456','{\"n\":18446744073709551615}',X'00ff',b'1111111111111111111111111111111111111111111111111111111111111111','a','x,y',NULL)").await;
    let r = run(&*s, "SELECT * FROM types").await;
    let row = &r[0].table.as_ref().unwrap().rows[0];
    assert_eq!(row[0], Value::Int(u64::MAX.to_string()));
    assert_eq!(
        row[1],
        Value::Decimal("12345678901234567890.12345678901234567890".into())
    );
    assert_eq!(row[2], Value::DateTime("2026-01-02 03:04:05.123456".into()));
    assert_eq!(row[3], Value::DateTime("2026-01-02 03:04:05.123456".into()));
    assert_eq!(
        row[5],
        Value::Bytes {
            len: 2,
            hex: "00ff".into()
        }
    );
    assert_eq!(row[6], Value::Int(u64::MAX.to_string()));
    assert_eq!(row[7], Value::Text("a".into()));
    assert_eq!(row[8], Value::Text("x,y".into()));
    assert_eq!(row[9], Value::Null);
    let zoned = run(
        &*s,
        "SET time_zone='+09:00'; SELECT ts FROM types; SET time_zone='+00:00'",
    )
    .await;
    assert_eq!(
        zoned[1].table.as_ref().unwrap().rows[0][0],
        Value::DateTime("2026-01-02 12:04:05.123456".into())
    );
    run(&*s, "SET time_zone='+09:00'").await;
    let mut instant = req("SELECT :instant");
    instant.params = serde_json::json!({"instant":{"t":"DateTimeTz","v":"2026-01-02T03:04:05Z"}})
        .as_object()
        .unwrap()
        .clone();
    assert!(s.execute(&instant, CancelToken::new()).await.is_err());
    run(&*s, "SET time_zone='+00:00'").await;
    let mut p = req("SELECT CAST(:u AS UNSIGNED), CAST(:d AS DECIMAL(40,20)), :s");
    p.params=serde_json::json!({"u":{"t":"Int","v":"18446744073709551615"},"d":{"t":"Decimal","v":"12345678901234567890.12345678901234567890"},"s":"x'; DROP TABLE types; --"}).as_object().unwrap().clone();
    let r = s.execute(&p, CancelToken::new()).await.unwrap();
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][0],
        Value::Int(u64::MAX.to_string())
    );
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][1],
        Value::Decimal("12345678901234567890.12345678901234567890".into())
    );
    assert_eq!(
        run(&*s, "SELECT 'semi;colon'; /* ; */ SELECT 2")
            .await
            .len(),
        2
    );
    assert_eq!(
        run(
            &*s,
            "SELECT 1; UPDATE types SET n='x'; UPDATE types SET n=NULL"
        )
        .await
        .len(),
        3
    );
    assert!(s
        .execute(&req("SELECT 1; DEFINITELY INVALID"), CancelToken::new())
        .await
        .is_err());
    s.ping().await.unwrap();
    let weird = "odd IS NOT DISTINCT FROM DEFAULT VALUES VALUES table";
    run(
        &*s,
        &format!("CREATE TABLE `{weird}`(id INT PRIMARY KEY,n VARCHAR(20)) ENGINE=InnoDB"),
    )
    .await;
    let weird_node = serde_json::json!(["table", db, weird]).to_string();
    let mut weird_edits = edit(
        &weird_node,
        vec![
            RowEdit::Insert {
                values: vec![
                    ("id".into(), Value::Int("1".into())),
                    ("n".into(), Value::Text("one".into())),
                ],
            },
            RowEdit::Insert {
                values: vec![
                    ("id".into(), Value::Int("2".into())),
                    ("n".into(), Value::Text("two".into())),
                ],
            },
        ],
    );
    s.apply_edits(&weird_edits, CancelToken::new())
        .await
        .unwrap();
    weird_edits.edits = vec![RowEdit::Update {
        key: vec![("id".into(), Value::Int("1".into()))],
        set: vec![("n".into(), Value::Text("updated".into()))],
    }];
    s.preview_edits(&weird_edits, CancelToken::new())
        .await
        .unwrap();
    s.apply_edits(&weird_edits, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        scalar(&run(&*s, &format!("SELECT n FROM `{weird}` WHERE id=1")).await),
        &Value::Text("updated".into())
    );
    run(&*s, &format!("DROP TABLE `{weird}`")).await;
    run(&*s,"CREATE TABLE precise(id BIGINT PRIMARY KEY,d DECIMAL(40,20),j JSON) ENGINE=InnoDB; INSERT INTO precise VALUES(1,9007199254740992.00000000000000000001,'{\"n\":1}'),(2,9007199254740992.00000000000000000002,'{\"n\":2}')").await;
    let precise = serde_json::json!(["table", db, "precise"]).to_string();
    let first = s
        .browse(
            &precise,
            PageRequest {
                offset: 0,
                limit: 10,
                filter: vec![(
                    "d".into(),
                    Value::Decimal("9007199254740992.00000000000000000001".into()),
                )],
            },
        )
        .await
        .unwrap();
    assert_eq!(first.row_count, 1);
    assert_eq!(first.table.unwrap().rows[0][0], Value::Int("1".into()));
    let first = s
        .browse(
            &precise,
            PageRequest {
                offset: 0,
                limit: 10,
                filter: vec![("j".into(), Value::Json("{\"n\":1}".into()))],
            },
        )
        .await
        .unwrap();
    assert_eq!(first.row_count, 1);
    assert_eq!(first.table.unwrap().rows[0][0], Value::Int("1".into()));
    run(&*s, "DROP TABLE precise").await;
    let mut limited = req("SELECT 1 AS a UNION ALL SELECT 2 UNION ALL SELECT 3");
    limited.row_limit = 1;
    let r = s.execute(&limited, CancelToken::new()).await.unwrap();
    assert_eq!(r[0].row_count, 1);
    assert!(r[0].truncated);
    assert!(s
        .execute(&req("THIS IS INVALID SQL"), CancelToken::new())
        .await
        .is_err());
    s.ping().await.unwrap();
    let node = serde_json::json!(["table", db, "types"]).to_string();
    let page = s
        .browse(
            &node,
            PageRequest {
                offset: 0,
                limit: 1,
                filter: vec![("id".into(), Value::Int(u64::MAX.to_string()))],
            },
        )
        .await
        .unwrap();
    assert!(!page.page.as_ref().unwrap().has_more);
    assert!(page.edit_target.is_some());
    assert_eq!(
        page.table.as_ref().unwrap().rows[0][3],
        Value::DateTimeTz("2026-01-02 03:04:05.123456+00:00".into())
    );
    assert!(
        matches!(page.table.as_ref().unwrap().rows[0][4],Value::Json(ref j) if j.contains("18446744073709551615"))
    );
    assert!(s
        .browse(
            &node,
            PageRequest {
                offset: 0,
                limit: 1,
                filter: vec![("missing".into(), Value::Null)]
            }
        )
        .await
        .is_err());
    assert!(s
        .list_children(None)
        .await
        .unwrap()
        .iter()
        .any(|n| n.label == db));
    assert_eq!(s.list_children(Some(&node)).await.unwrap().len(), 3);
    run(&*s,"CREATE TABLE child(a BIGINT UNSIGNED,b INT,PRIMARY KEY(a,b),CONSTRAINT fk_parent FOREIGN KEY(a) REFERENCES types(id)) ENGINE=InnoDB; CREATE VIEW v_types AS SELECT id FROM types").await;
    let graph = s.schema_graph(Some(&db)).await.unwrap();
    assert_eq!(graph.entities.len(), 3);
    assert_eq!(graph.relations.len(), 1);
    let snap = s.schema_snapshot(Some(&db)).await.unwrap();
    assert_eq!(snap.dialect, "mysql");
    assert_eq!(snap.objects.len(), 3);
    assert!(snap
        .objects
        .iter()
        .any(|o| o.definition.contains("FOREIGN KEY")));
    let ddl = s
        .ddl_render(&SnapshotDiff {
            dialect: "mysql".into(),
            changes: vec![SchemaChange {
                action: "remove".into(),
                before: Some(
                    snap.objects
                        .iter()
                        .find(|o| o.name == "child")
                        .unwrap()
                        .clone(),
                ),
                after: None,
                risk: SchemaRisk::Destructive,
            }],
        })
        .await
        .unwrap();
    assert!(ddl[0].contains("DROP TABLE"));
    assert_eq!(
        s.schema_snapshot(Some(&db)).await.unwrap(),
        snap,
        "plans do not execute"
    );
    let preview = s
        .preview(&req("UPDATE types SET n='preview'"), CancelToken::new())
        .await
        .unwrap();
    assert!(!preview.committed);
    assert_eq!(preview.statements[0].affected, Some(1));
    assert_eq!(scalar(&run(&*s, "SELECT n FROM types").await), &Value::Null);
    let rejected=s.preview(&req("INSERT INTO child VALUES(18446744073709551615,1); CREATE TABLE must_not_exist(id INT)"),CancelToken::new()).await.unwrap();
    assert!(!rejected.notes.is_empty());
    assert_eq!(
        scalar(&run(&*s, "SELECT COUNT(*) FROM child").await),
        &Value::Int("0".into())
    );
    let update = edit(
        &node,
        vec![RowEdit::Update {
            key: vec![("id".into(), Value::Int(u64::MAX.to_string()))],
            set: vec![("n".into(), Value::Text("changed".into()))],
        }],
    );
    let p = s.preview_edits(&update, CancelToken::new()).await.unwrap();
    assert!(p.statements[0].before.is_some());
    assert_eq!(scalar(&run(&*s, "SELECT n FROM types").await), &Value::Null);
    assert_eq!(
        s.apply_edits(&update, CancelToken::new())
            .await
            .unwrap()
            .affected,
        vec![1]
    );
    let mut conflict = update.clone();
    if let RowEdit::Update { set, .. } = &mut conflict.edits[0] {
        set[0].1 = Value::Text("must rollback".into());
    }
    conflict.edits.push(RowEdit::Delete {
        key: vec![("id".into(), Value::Int("999".into()))],
    });
    assert!(matches!(
        s.apply_edits(&conflict, CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    ));
    assert_eq!(
        scalar(&run(&*s, "SELECT n FROM types").await),
        &Value::Text("changed".into())
    );
    run(
        &*s,
        "CREATE TABLE imported(id BIGINT PRIMARY KEY,n VARCHAR(50) UNIQUE) ENGINE=InnoDB",
    )
    .await;
    let imported = serde_json::json!(["table", db, "imported"]).to_string();
    let inserts = edit(
        &imported,
        (1..=1001)
            .map(|i| RowEdit::Insert {
                values: vec![
                    ("id".into(), Value::Int(i.to_string())),
                    ("n".into(), Value::Text(format!("n{i}"))),
                ],
            })
            .collect(),
    );
    s.import_rows(&inserts, true, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        scalar(&run(&*s, "SELECT COUNT(*) FROM imported").await),
        &Value::Int("0".into())
    );
    s.import_rows(&inserts, false, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        scalar(&run(&*s, "SELECT COUNT(*) FROM imported").await),
        &Value::Int("1001".into())
    );
    let page = s
        .browse(
            &imported,
            PageRequest {
                offset: 1000,
                limit: 10,
                filter: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(page.row_count, 1);
    assert!(!page.page.unwrap().has_more);
    let mut ro = req("SELECT COUNT(*) FROM imported");
    ro.read_only = true;
    assert!(s.execute(&ro, CancelToken::new()).await.is_ok());
    for sql in [
        "INSERT INTO imported VALUES(2000,'bad')",
        "COMMIT; INSERT INTO imported VALUES(2000,'bad')",
        "SET SESSION TRANSACTION READ WRITE; INSERT INTO imported VALUES(2000,'bad')",
        "/*! INSERT INTO imported VALUES(2000,'bad') */ SELECT 1",
    ] {
        ro.text = sql.into();
        assert!(s.execute(&ro, CancelToken::new()).await.is_err(), "{sql}");
    }
    run(&*s, "BEGIN; UPDATE imported SET n='uncommitted' WHERE id=1").await;
    assert!(s
        .preview(
            &req("UPDATE imported SET n='preview' WHERE id=2"),
            CancelToken::new()
        )
        .await
        .is_err());
    assert_eq!(
        scalar(&run(&*s, "SELECT n FROM imported WHERE id=1").await),
        &Value::Text("uncommitted".into())
    );
    run(&*s, "ROLLBACK").await;
    let plan = s
        .explain(
            "SELECT * FROM imported WHERE id=1",
            false,
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert!(!plan.op.is_empty());
    let analyze = s
        .explain(
            "SELECT * FROM imported WHERE id=1",
            true,
            CancelToken::new(),
        )
        .await;
    if key == "AXDB_TEST_MYSQL" {
        assert!(analyze.unwrap().actual_rows.is_some());
    } else {
        assert!(matches!(analyze.unwrap_err().kind, ErrorKind::Unsupported));
    }
    let token = CancelToken::new();
    let sleep_req = req("SELECT SLEEP(10)");
    let exec = s.execute(&sleep_req, token.clone());
    tokio::pin!(exec);
    let (r, ()) = tokio::join!(exec, async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });
    assert!(matches!(r.unwrap_err().kind, ErrorKind::Cancelled));
    s.ping().await.unwrap();
    let mut timeout = req("SELECT BENCHMARK(1000000000,SHA2('x',512))");
    timeout.timeout_ms = 50;
    assert!(matches!(
        s.execute(&timeout, CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    ));
    s.ping().await.unwrap();
    run(&*s,"CREATE TABLE nontransactional(id INT) ENGINE=MyISAM; INSERT INTO nontransactional VALUES(1)").await;
    assert!(s
        .preview(&req("UPDATE nontransactional SET id=2"), CancelToken::new())
        .await
        .is_err());
    assert_eq!(
        scalar(&run(&*s, "SELECT id FROM nontransactional").await),
        &Value::Int("1".into())
    );
    run(&*s,"DROP TABLE nontransactional; CREATE TRIGGER m1_trigger BEFORE UPDATE ON imported FOR EACH ROW SET NEW.n=NEW.n").await;
    assert!(s
        .preview(
            &req("UPDATE imported SET n='bad' WHERE id=1"),
            CancelToken::new()
        )
        .await
        .is_err());
    run(&*s, "DROP TRIGGER m1_trigger; DROP VIEW v_types").await;
    let sink = Bytes(Mutex::new(vec![]));
    let summary = s.backup_stream(CancelToken::new(), &sink).await.unwrap();
    assert_eq!(summary.format, "mysql-sql-v1");
    assert!(summary
        .counts
        .iter()
        .any(|(name, n)| name == "imported" && *n == 1001));
    let records = {
        let bytes = sink.0.lock().unwrap();
        let mut cursor = std::io::Cursor::new(&*bytes);
        let mut ended = false;
        let mut records = vec![];
        while let Some((tag, data)) = read_backup_record(&mut cursor).unwrap() {
            if tag == b'Z' {
                ended = true;
            } else {
                assert_eq!(tag, b'S');
                records.push(String::from_utf8(data).unwrap());
            }
        }
        assert!(ended && records.len() > 1000);
        records
    };
    let restored_db = format!("{db}_copy");
    run(&*admin, &format!("CREATE DATABASE `{restored_db}`")).await;
    let mut restored_config = config.clone();
    restored_config
        .fields
        .insert("database".into(), restored_db.clone());
    let restored = driver.connect(&restored_config).await.unwrap();
    for sql in records {
        run(&*restored, &sql).await;
    }
    let restored_types = run(&*restored, "SELECT * FROM types").await;
    let original_types = run(&*s, "SELECT * FROM types").await;
    assert_eq!(
        restored_types[0].table, original_types[0].table,
        "logical backup preserves full exact values"
    );
    assert_eq!(
        scalar(&run(&*restored, "SELECT COUNT(*) FROM imported").await),
        &Value::Int("1001".into())
    );
    restored.close().await;
    run(&*admin, &format!("DROP DATABASE `{restored_db}`")).await;
    let reader = format!("axdb_m1_{}_{}", std::process::id(), port);
    run(&*s,&format!("CREATE USER '{reader}'@'%' ACCOUNT LOCK; GRANT SELECT ON `{db}`.types TO '{reader}'@'%'")).await;
    let roles = s.roles().await.unwrap();
    assert!(roles
        .roles
        .iter()
        .any(|r| r.name == format!("{reader}@%") && !r.login));
    assert!(roles.matrix.iter().any(|p| p.role == format!("{reader}@%")
        && p.namespace == db
        && p.object == "types"
        && p.privilege == "SELECT"));
    assert!(roles.supported);
    assert!(roles
        .roles
        .iter()
        .any(|r| r.name.starts_with("root@") && r.superuser));
    let root = roles
        .roles
        .iter()
        .find(|r| r.name.starts_with("root@"))
        .unwrap()
        .name
        .clone();
    let grants = s
        .plan_grants(&[GrantChange {
            role: root,
            namespace: db.clone(),
            object: "types".into(),
            privilege: "SELECT".into(),
            revoke: false,
        }])
        .await
        .unwrap();
    assert!(!grants.apply_supported);
    assert!(grants.statements[0].starts_with("GRANT SELECT"));
    run(&*s, &format!("DROP USER '{reader}'@'%'")).await;
    s.close().await;
    assert!(s.ping().await.is_err());
    run(&*admin, &format!("DROP DATABASE `{db}`")).await;
    admin.close().await;
}
#[tokio::test]
async fn mysql_contract() {
    contract("AXDB_TEST_MYSQL", 53306).await;
}
#[tokio::test]
async fn mariadb_contract() {
    contract("AXDB_TEST_MARIADB", 53307).await;
}

#[tokio::test]
async fn mysql_tls_authentication_and_validation() {
    let Some(mut config) = cfg("AXDB_TEST_MYSQL", 53306) else {
        return;
    };
    let driver = MysqlDriver::new();
    let mut server_config = config.clone();
    server_config.fields.remove("database");
    let server = driver.connect(&server_config).await.unwrap();
    assert_eq!(
        scalar(&run(&*server, "SELECT DATABASE()").await),
        &Value::Null
    );
    assert!(server
        .explain("SELECT 1", false, CancelToken::new())
        .await
        .is_ok());
    assert!(server.schema_snapshot(None).await.is_err());
    server.close().await;
    config.fields.insert("sslmode".into(), "require".into());
    let s = driver.connect(&config).await.unwrap();
    let cipher = run(&*s, "SHOW SESSION STATUS LIKE 'Ssl_cipher'").await;
    assert!(
        matches!(&cipher[0].table.as_ref().unwrap().rows[0][1],Value::Text(cipher) if !cipher.is_empty())
    );
    s.close().await;
    if let Some(ca) = std::env::var_os("AXDB_TEST_MYSQL_CA") {
        config.fields.insert("sslmode".into(), "verify-full".into());
        config
            .fields
            .insert("tls_server_name".into(), "host.docker.internal".into());
        config
            .fields
            .insert("sslrootcert".into(), ca.to_string_lossy().into_owned());
        let s = driver.connect(&config).await.unwrap();
        s.ping().await.unwrap();
        s.close().await;
        config.fields.remove("tls_server_name");
        assert!(
            driver.connect(&config).await.is_err(),
            "test SAN certificate does not match 127.0.0.1 without the tunnel hostname hint"
        );
    }
    config.fields.insert("sslmode".into(), "disable".into());
    config.secret = Some("deliberately-invalid-test-password".into());
    let e = match driver.connect(&config).await {
        Ok(_) => panic!("authentication unexpectedly succeeded"),
        Err(e) => e,
    };
    assert!(matches!(e.kind, ErrorKind::Auth));
    assert!(!e.message.contains("deliberately-invalid-test-password"));
    config.fields.insert("port".into(), "0".into());
    assert!(driver.connect(&config).await.is_err());
}

async fn query_edit_contract(key: &str, port: u16) {
    let Some(config)=cfg(key,port) else {return;};
    let s=MysqlDriver::new().connect(&config).await.unwrap();
    let name=format!("d10_query_{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    run(&*s,&format!("CREATE TABLE {name}(id int PRIMARY KEY,name varchar(100)) ENGINE=InnoDB; INSERT INTO {name} VALUES(1,'old'),(2,'old')")).await;
    let result=run(&*s,&format!("SELECT id AS alias_id,name,id+1 AS computed FROM {name} WHERE id=1")).await.remove(0);
    let target=result.edit_target.unwrap();
    assert_eq!(target.columns,vec![Some("id".into()),Some("name".into()),None]);
    for sql in [format!("SELECT name FROM {name}"),format!("SELECT DISTINCT id,name FROM {name}"),format!("SELECT id,count(*) FROM {name} GROUP BY id"),format!("SELECT a.id,a.name FROM {name} a JOIN {name} b ON a.id=b.id")] {assert!(run(&*s,&sql).await[0].edit_target.is_none(),"{sql}");}
    let request:EditRequest=serde_json::from_value(serde_json::json!({"node_id":target.node_id,"edits":[{"op":"Update","key":[["id",{"t":"Int","v":"1"}]],"set":[["name",{"t":"Text","v":"edited"}]]}],"originals":[[["name",{"t":"Text","v":"old"}]]]})).unwrap();
    s.preview_edits(&request,CancelToken::new()).await.unwrap();
    assert_eq!(scalar(&run(&*s,&format!("SELECT name FROM {name} WHERE id=1")).await),&Value::Text("old".into()));
    s.apply_edits(&request,CancelToken::new()).await.unwrap();
    assert_eq!(scalar(&run(&*s,&format!("SELECT name FROM {name} WHERE id=1")).await),&Value::Text("edited".into()));
    assert!(matches!(s.apply_edits(&request,CancelToken::new()).await,Err(DbError{kind:ErrorKind::Conflict,..})));
    let mut conflicting=request.clone();
    if let RowEdit::Update{set,..}=&mut conflicting.edits[0]{set[0].1=Value::Text("should rollback".into());}
    conflicting.originals=vec![vec![("name".into(),Value::Text("edited".into()))],vec![("name".into(),Value::Text("stale".into()))]];
    conflicting.edits.push(RowEdit::Update{key:vec![("id".into(),Value::Int("2".into()))],set:vec![("name".into(),Value::Text("second".into()))]});
    assert!(matches!(s.apply_edits(&conflicting,CancelToken::new()).await,Err(DbError{kind:ErrorKind::Conflict,..})));
    assert_eq!(scalar(&run(&*s,&format!("SELECT name FROM {name} WHERE id=1")).await),&Value::Text("edited".into()));
    let mut bounded=req(&format!("SELECT * FROM {name}"));bounded.row_limit=1;
    assert!(s.execute(&bounded,CancelToken::new()).await.unwrap()[0].edit_target.is_none());
    run(&*s,&format!("DROP TABLE {name}")).await;s.close().await;
}
#[tokio::test] async fn mysql_query_result_edits(){query_edit_contract("AXDB_TEST_MYSQL",53306).await;}
#[tokio::test] async fn mariadb_query_result_edits(){query_edit_contract("AXDB_TEST_MARIADB",53307).await;}
