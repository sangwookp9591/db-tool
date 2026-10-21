//! W4 isolated database contracts; run once in-process and again with AXDB_DRIVER_DIR.
// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use async_trait::async_trait;
use dbcore::*;
use futures_util::FutureExt;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, panic::AssertUnwindSafe, sync::Mutex};
async fn driver(id: &str) -> Box<dyn Driver> {
    if let Some(path) = std::env::var_os("AXDB_DRIVER_DIR") {
        return Box::new(
            axdb::plugin_host::PluginDriver::load(
                std::path::PathBuf::from(path).join(format!("axdb-driver-{id}")),
            )
            .await
            .unwrap(),
        );
    }
    match id {
        "postgres" => Box::new(driver_postgres::PostgresDriver::new()),
        "sqlite" => Box::new(driver_sqlite::SqliteDriver::new()),
        "mongodb" => Box::new(driver_mongodb::MongoDriver::new()),
        "neo4j" => Box::new(driver_neo4j::Neo4jDriver::new()),
        "redis" => Box::new(driver_redis::RedisDriver::new()),
        _ => unreachable!(),
    }
}
fn req(sql: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":sql,"row_limit":100000})).unwrap()
}
async fn exec(s: &dyn Session, sql: &str) {
    s.execute(&req(sql), CancelToken::new()).await.unwrap();
}
struct CancelProgress(CancelToken);
#[async_trait]
impl ProgressSink for CancelProgress {
    async fn progress(&self, done: u64, _: u64) -> Result<(), DbError> {
        if done >= 100 {
            self.0.cancel();
        }
        Ok(())
    }
}
struct Progress(Mutex<Vec<u64>>);
#[async_trait]
impl ProgressSink for Progress {
    async fn progress(&self, done: u64, _: u64) -> Result<(), DbError> {
        self.0.lock().unwrap().push(done);
        Ok(())
    }
}
fn digest(b: &BackupData) -> String {
    format!("{:x}", Sha256::digest(&b.data))
}
#[tokio::test]
async fn admin_sqlite_backup_restore_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let d = driver("sqlite").await;
    let cfg = |name: &str| ConnectConfig {
        fields: BTreeMap::from([(
            "path".into(),
            dir.path().join(name).to_string_lossy().into_owned(),
        )]),
        secret: None,
        connect_timeout_ms: 1000,
    };
    let source = d.connect(&cfg("source.db")).await.unwrap();
    let target = d.connect(&cfg("target.db")).await.unwrap();
    exec(&*source,"CREATE TABLE items(id INTEGER PRIMARY KEY, value TEXT, bytes BLOB); CREATE INDEX by_value ON items(value); INSERT INTO items VALUES(1,'한글',X'00FF'),(2,NULL,NULL)").await;
    let backup = source.backup(CancelToken::new()).await.unwrap();
    assert_eq!(backup.counts, vec![("items".into(), 2)]);
    target
        .restore(&backup, false, CancelToken::new())
        .await
        .unwrap();
    let restored = target.backup(CancelToken::new()).await.unwrap();
    assert_eq!(backup.counts, restored.counts);
    assert_eq!(digest(&backup), digest(&restored));
    assert!(target
        .restore(&backup, false, CancelToken::new())
        .await
        .is_err());
    exec(&*target, "INSERT INTO items VALUES(3,'extra',NULL)").await;
    target
        .restore(&backup, true, CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        target.backup(CancelToken::new()).await.unwrap().counts,
        backup.counts
    );
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert!(source.backup(cancelled).await.is_err());
    source.close().await;
    target.close().await;
}
#[tokio::test]
async fn admin_pg_roundtrip_roles_copy_progress() {
    let Ok(dsn) = std::env::var("AXDB_TEST_PG") else {
        return;
    };
    let pg: tokio_postgres::Config = dsn.parse().unwrap();
    assert_eq!(
        pg.get_hosts(),
        &[tokio_postgres::config::Host::Tcp("127.0.0.1".into())]
    );
    assert_eq!(pg.get_ports(), &[55432]);
    assert_eq!(pg.get_dbname(), Some("axdb_test"));
    let (admin, conn) = pg.connect(tokio_postgres::NoTls).await.unwrap();
    let connection = tokio::spawn(conn);
    let suffix = std::process::id();
    let source_db = format!("axdb_test_w4_src_{suffix}");
    let target_db = format!("axdb_test_w4_dst_{suffix}");
    let role = format!("axdb_test_role_w4_{suffix}");
    for sql in [
        format!("CREATE DATABASE {source_db}"),
        format!("CREATE DATABASE {target_db}"),
        format!("CREATE ROLE {role}"),
    ] {
        admin.batch_execute(&sql).await.unwrap();
    }
    let d = driver("postgres").await;
    let cfg = |db: String| ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), db),
            ("user".into(), pg.get_user().unwrap().into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: pg
            .get_password()
            .map(|p| String::from_utf8(p.to_vec()).unwrap()),
        connect_timeout_ms: 3000,
    };
    let source = d.connect(&cfg(source_db.clone())).await.unwrap();
    let target = d.connect(&cfg(target_db.clone())).await.unwrap();
    let result=AssertUnwindSafe(async {
        exec(&*source,"CREATE TABLE items(id SERIAL PRIMARY KEY, value TEXT UNIQUE NOT NULL, optional TEXT, bytes BYTEA); CREATE INDEX by_optional ON items(optional); CREATE VIEW item_names AS SELECT id,value FROM items").await;
        let request=EditRequest{originals: vec![],node_id:serde_json::json!(["table","public","tables","items"]).to_string(),create_table:None,edits:(0..251).map(|i|RowEdit::Insert{values:vec![("value".into(),Value::Text(format!("value-{i}\t\\N\n한글"))),("optional".into(),if i%2==0{Value::Null}else{Value::Text("\\N".into())}),("bytes".into(),Value::Bytes{len:2,hex:"00ff".into()})]}).collect(),timeout_ms:30000,preview_sample_limit:100};
        source.import_rows(&request,true,CancelToken::new()).await.unwrap();let progress=Progress(Mutex::new(vec![]));
        let applied=source.import_rows_progress(&request,false,CancelToken::new(),&progress).await.unwrap();assert!(matches!(applied,ImportOutput::Applied(r) if r.statements[0].starts_with("COPY ")&&r.affected==vec![251]));assert!(progress.0.lock().unwrap().contains(&100));assert!(progress.0.lock().unwrap().contains(&200));
        let backup=source.backup(CancelToken::new()).await.unwrap();assert_eq!(backup.counts,vec![("\"public\".\"items\"".into(),251)]);target.restore(&backup,false,CancelToken::new()).await.unwrap();let restored=target.backup(CancelToken::new()).await.unwrap();assert_eq!(backup.counts,restored.counts);assert_eq!(digest(&backup),digest(&restored));assert!(target.restore(&backup,false,CancelToken::new()).await.is_err());
        exec(&*target,"INSERT INTO items(value) VALUES('overwrite extra')").await;
        target.restore(&backup,true,CancelToken::new()).await.unwrap();assert_eq!(digest(&backup),digest(&target.backup(CancelToken::new()).await.unwrap()));
        let cancellation=CancelToken::new();let progress_cancel=CancelProgress(cancellation.clone());
        let cancelled=source.import_rows_progress(&request,false,cancellation,&progress_cancel).await;assert!(cancelled.is_err());assert_eq!(source.backup(CancelToken::new()).await.unwrap().counts,backup.counts);

        let change=GrantChange{role:role.clone(),namespace:"public".into(),object:"items".into(),privilege:"SELECT".into(),revoke:false};let before=source.roles().await.unwrap();assert!(!before.matrix.iter().find(|p|p.role==role&&p.object=="items"&&p.privilege=="SELECT").unwrap().allowed);
        let page=source.roles_page(&RolesPageQuery{role:Some(role.clone()),namespace:Some("public".into()),object:Some("items".into()),offset:0,limit:100}).await.unwrap();assert_eq!(page.matrix.len(),7);assert_eq!(page.page.as_ref().unwrap().total,7);assert!(!page.page.as_ref().unwrap().role_required);
        let plan=source.preview_grants(std::slice::from_ref(&change),CancelToken::new()).await.unwrap();assert!(plan.apply_supported);assert_eq!(source.roles().await.unwrap(),before);
        source.apply_grants(std::slice::from_ref(&change),CancelToken::new()).await.unwrap();assert!(source.roles().await.unwrap().matrix.iter().any(|p|p.role==role&&p.object=="items"&&p.privilege=="SELECT"&&p.allowed));
        source.apply_grants(&[GrantChange{revoke:true,..change}],CancelToken::new()).await.unwrap();assert!(!source.roles().await.unwrap().matrix.iter().any(|p|p.role==role&&p.object=="items"&&p.privilege=="SELECT"&&p.allowed));
        let mut bad=request.clone();bad.edits.push(bad.edits[0].clone());assert!(target.import_rows_progress(&bad,false,CancelToken::new(),&progress).await.is_err());assert_eq!(target.backup(CancelToken::new()).await.unwrap().counts,backup.counts);
    }).catch_unwind().await;
    source.close().await;
    target.close().await;
    for sql in [
        format!("DROP DATABASE {source_db} WITH (FORCE)"),
        format!("DROP DATABASE {target_db} WITH (FORCE)"),
        format!("DROP ROLE {role}"),
    ] {
        admin.batch_execute(&sql).await.unwrap();
    }
    connection.abort();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
#[tokio::test]
async fn admin_mongo_roundtrip_indexes_and_roles() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        return;
    };
    assert!(uri.contains("@127.0.0.1:57017/"));
    let raw = mongodb::Client::with_uri_str(&uri).await.unwrap();
    let options = mongodb::options::ClientOptions::parse(&uri).await.unwrap();
    let password = options.credential.and_then(|c| c.password);
    let suffix = std::process::id();
    let source_db = format!("axdb_test_w4_src_{suffix}");
    let target_db = format!("axdb_test_w4_dst_{suffix}");
    let d = driver("mongodb").await;
    let cfg = |db: String| ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), "mongodb://127.0.0.1:57017".into()),
            ("database".into(), db),
            ("user".into(), "axdb".into()),
            ("authSource".into(), "admin".into()),
        ]),
        secret: password.clone(),
        connect_timeout_ms: 3000,
    };
    let source = d.connect(&cfg(source_db.clone())).await.unwrap();
    let target = d.connect(&cfg(target_db.clone())).await.unwrap();
    let result=AssertUnwindSafe(async {
        let collection=raw.database(&source_db).collection::<mongodb::bson::Document>("items");
        collection.insert_many([mongodb::bson::doc!{"_id":1,"value":"한글","decimal":mongodb::bson::Decimal128::from_bytes([0;16])},mongodb::bson::doc!{"_id":2,"value":"second","at":mongodb::bson::DateTime::now()}]).await.unwrap();collection.create_index(mongodb::IndexModel::builder().keys(mongodb::bson::doc!{"value":1}).options(mongodb::options::IndexOptions::builder().name("by_value".to_owned()).unique(true).build()).build()).await.unwrap();
        let backup=source.backup(CancelToken::new()).await.unwrap();target.restore(&backup,false,CancelToken::new()).await.unwrap();let restored=target.backup(CancelToken::new()).await.unwrap();assert_eq!(backup.counts,restored.counts);assert_eq!(digest(&backup),digest(&restored));assert!(target.restore(&backup,false,CancelToken::new()).await.is_err());assert!(target.restore(&backup,true,CancelToken::new()).await.is_err());
        let snapshot=source.roles().await.unwrap();assert!(snapshot.supported);let plan=source.plan_grants(&[GrantChange{role:"does-not-exist".into(),namespace:source_db.clone(),object:"read".into(),privilege:"ROLE".into(),revoke:false}]).await.unwrap();assert!(plan.statements[0].contains("grantRolesToUser"));
    }).catch_unwind().await;
    source.close().await;
    target.close().await;
    raw.database(&source_db).drop().await.unwrap();
    raw.database(&target_db).drop().await.unwrap();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
#[tokio::test]
async fn admin_redis_atomic_import_and_acl_no_secrets() {
    let Ok(uri) = std::env::var("AXDB_TEST_REDIS") else {
        return;
    };
    assert_eq!(uri, "redis://:testredis@127.0.0.1:56379/0");
    let client = redis::Client::open(uri).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    let d = driver("redis").await;
    let s = d
        .connect(&ConnectConfig {
            fields: BTreeMap::from([
                ("host".into(), "127.0.0.1".into()),
                ("port".into(), "56379".into()),
                ("db".into(), "0".into()),
            ]),
            secret: Some("testredis".into()),
            connect_timeout_ms: 2000,
        })
        .await
        .unwrap();
    let prefix = format!("axdb_test_w4_{}:", std::process::id());
    let keys = [format!("{prefix}a"), format!("{prefix}b")];
    let request = EditRequest {originals: vec![],
        node_id: serde_json::json!(["scan", prefix.as_bytes(), "*", 0]).to_string(),
        create_table: None,
        edits: ["a", "b"]
            .iter()
            .map(|k| RowEdit::Insert {
                values: vec![
                    ("key".into(), Value::Text((*k).into())),
                    ("value".into(), Value::Text("payload".into())),
                ],
            })
            .collect(),
        timeout_ms: 3000,
        preview_sample_limit: 100,
    };
    let result = AssertUnwindSafe(async {
        s.import_rows(&request, true, CancelToken::new())
            .await
            .unwrap();
        let exists: u64 = redis::cmd("EXISTS")
            .arg(&keys)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(exists, 0);
        s.import_rows(&request, false, CancelToken::new())
            .await
            .unwrap();
        let exists: u64 = redis::cmd("EXISTS")
            .arg(&keys)
            .query_async(&mut connection)
            .await
            .unwrap();
        assert_eq!(exists, 2);
        let _: u64 = redis::cmd("DEL")
            .arg(&keys[1])
            .query_async(&mut connection)
            .await
            .unwrap();
        assert!(s
            .import_rows(&request, false, CancelToken::new())
            .await
            .is_err());
        let exists: bool = redis::cmd("EXISTS")
            .arg(&keys[1])
            .query_async(&mut connection)
            .await
            .unwrap();
        assert!(!exists);
        let roles = s.roles().await.unwrap();
        let encoded = serde_json::to_string(&roles).unwrap();
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("testredis"));
        let plan = s
            .plan_grants(&[GrantChange {
                role: "analyst".into(),
                namespace: "command".into(),
                object: String::new(),
                privilege: "@read".into(),
                revoke: false,
            }])
            .await
            .unwrap();
        assert!(!plan.apply_supported);
        assert_eq!(plan.statements, vec!["ACL SETUSER analyst +@read"]);
    })
    .catch_unwind()
    .await;
    let _: u64 = redis::cmd("DEL")
        .arg(&keys)
        .query_async(&mut connection)
        .await
        .unwrap();
    s.close().await;
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
#[tokio::test]
async fn admin_neo4j_roundtrip_and_community_reason() {
    let Ok(value) = std::env::var("AXDB_TEST_NEO4J") else {
        return;
    };
    let parts = value.split('|').collect::<Vec<_>>();
    assert_eq!(parts[0], "bolt://127.0.0.1:57687");
    let Ok(target_uri) = std::env::var("AXDB_TEST_NEO4J_RESTORE") else {
        eprintln!("AXDB_TEST_NEO4J_RESTORE unset: Neo4j restore roundtrip not run");
        return;
    };
    assert_eq!(target_uri, "bolt://127.0.0.1:57688");
    let d = driver("neo4j").await;
    let cfg = |uri: &str| ConnectConfig {
        fields: BTreeMap::from([("uri".into(), uri.into()), ("user".into(), parts[1].into())]),
        secret: Some(parts[2].into()),
        connect_timeout_ms: 5000,
    };
    let source = d.connect(&cfg(parts[0])).await.unwrap();
    let target = d.connect(&cfg(&target_uri)).await.unwrap();
    let label = format!("AxdbW4{}", std::process::id());
    let result=AssertUnwindSafe(async {
        // Only W4 fixture labels on the explicitly guarded test server are cleaned.
        exec(&*source,"MATCH (n) WHERE any(label IN labels(n) WHERE label STARTS WITH 'AxdbW4') DETACH DELETE n").await;
        exec(&*source,&format!("CREATE (a:{label} {{name:'한글', number:9007199254740993}})-[:KNOWS {{score:1.25}}]->(b:{label} {{name:'two'}})" )).await;
        let snapshot=source.roles().await.unwrap();assert!(!snapshot.supported);assert!(snapshot.reason.unwrap().contains("Community"));
        let backup=source.backup(CancelToken::new()).await.unwrap();target.restore(&backup,false,CancelToken::new()).await.unwrap();let restored=target.backup(CancelToken::new()).await.unwrap();assert_eq!(backup.counts,restored.counts);assert_eq!(digest(&backup),digest(&restored));assert!(target.restore(&backup,false,CancelToken::new()).await.is_err());
    }).catch_unwind().await;
    // Cleanup must survive the plugin process failing for both original sessions.
    let cleanup_driver = driver("neo4j").await;
    let cleanup_source = cleanup_driver.connect(&cfg(parts[0])).await.unwrap();
    let cleanup_target = cleanup_driver.connect(&cfg(&target_uri)).await.unwrap();
    exec(
        &*cleanup_source,
        &format!("MATCH (n:{label}) DETACH DELETE n"),
    )
    .await;
    // This connection is guarded above to the worker-owned restore container only.
    exec(&*cleanup_target, "MATCH (n) DETACH DELETE n").await;
    for object in cleanup_target.schema_snapshot(None).await.unwrap().objects {
        if ["constraint", "index"].contains(&object.kind.as_str())
            && !object.definition.contains("LOOKUP")
        {
            exec(
                &*cleanup_target,
                &format!(
                    "DROP {} `{}` IF EXISTS",
                    object.kind.to_uppercase(),
                    object.name.replace('`', "``")
                ),
            )
            .await;
        }
    }
    cleanup_source.close().await;
    cleanup_target.close().await;
    source.close().await;
    target.close().await;
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
