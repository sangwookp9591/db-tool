//! File-streaming contracts. Only explicitly configured isolated test servers are used.
use async_trait::async_trait;
use axdb::backup_file;
use dbcore::*;
use futures_util::FutureExt;
use std::{
    collections::BTreeMap,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::Duration,
};
async fn driver(id: &str) -> Box<dyn Driver> {
    if let Some(dir) = std::env::var_os("AXDB_DRIVER_DIR") {
        Box::new(
            axdb::plugin_host::PluginDriver::load(
                std::path::PathBuf::from(dir).join(format!("axdb-driver-{id}")),
            )
            .await
            .unwrap(),
        )
    } else {
        match id {
            "postgres" => Box::new(driver_postgres::PostgresDriver::new()),
            "mongodb" => Box::new(driver_mongodb::MongoDriver::new()),
            "sqlite" => Box::new(driver_sqlite::SqliteDriver::new()),
            "neo4j" => Box::new(driver_neo4j::Neo4jDriver::new()),
            _ => unreachable!(),
        }
    }
}
fn request(sql: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":sql,"row_limit":100,"timeout_ms":3600000}))
        .unwrap()
}
async fn exec(s: &dyn Session, sql: &str) {
    s.execute(&request(sql), CancelToken::new()).await.unwrap();
}
struct Progress {
    values: Mutex<Vec<u64>>,
    cancel: Option<CancelToken>,
}
#[async_trait]
impl ProgressSink for Progress {
    async fn progress(&self, n: u64, _: u64) -> Result<(), DbError> {
        let mut values = self.values.lock().unwrap();
        if values.len() < 32 {
            values.push(n);
        }
        drop(values);
        if n > 1024 * 1024 {
            if let Some(cancel) = &self.cancel {
                cancel.cancel();
            }
        }
        Ok(())
    }
}
impl Progress {
    fn new() -> Self {
        Self {
            values: Mutex::new(vec![]),
            cancel: None,
        }
    }
}
async fn archive(
    source: &dyn Session,
    target: &dyn Session,
    path: &std::path::Path,
) -> backup_file::Manifest {
    let progress = Progress::new();
    let manifest = backup_file::create(source, path, CancelToken::new(), &progress)
        .await
        .unwrap();
    let updates = progress.values.lock().unwrap().len();
    assert!(updates > 0);
    if manifest.bytes > BACKUP_CHUNK_BYTES as u64 {
        assert!(updates > 1);
    }
    let mut stage =
        backup_file::stage(&mut std::fs::File::open(path).unwrap(), &CancelToken::new()).unwrap();
    assert_eq!(stage.manifest, manifest);
    target
        .restore_stream(
            &manifest.summary(),
            &mut stage.payload,
            false,
            CancelToken::new(),
        )
        .await
        .unwrap();
    manifest
}
#[tokio::test]
async fn streaming_sqlite_file_cancel_and_tamper() {
    let dir = tempfile::tempdir().unwrap();
    let d = driver("sqlite").await;
    let cfg = |name: &str| ConnectConfig {
        fields: BTreeMap::from([(
            "path".into(),
            dir.path().join(name).to_string_lossy().into_owned(),
        )]),
        secret: None,
        connect_timeout_ms: 3000,
    };
    let source = d.connect(&cfg("source.db")).await.unwrap();
    let target = d.connect(&cfg("target.db")).await.unwrap();
    exec(&*source,"CREATE TABLE items(id INTEGER PRIMARY KEY,value BLOB); INSERT INTO items VALUES(1,zeroblob(12000000))").await;
    let path = dir.path().join("stream.axbackup");
    let manifest = archive(&*source, &*target, &path).await;
    assert!(manifest.bytes > 8 * 1024 * 1024);
    let restored = target
        .execute(
            &request("SELECT length(value) FROM items"),
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        restored[0].table.as_ref().unwrap().rows[0][0],
        Value::Int("12000000".into())
    );
    let cancel = CancelToken::new();
    let progress = Progress {
        values: Mutex::new(vec![]),
        cancel: Some(cancel.clone()),
    };
    let failed = dir.path().join("cancel.axbackup");
    std::fs::write(&failed, b"previous file").unwrap();
    assert!(backup_file::create(&*source, &failed, cancel, &progress)
        .await
        .is_err());
    assert_eq!(std::fs::read(&failed).unwrap(), b"previous file");
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|f| f
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".axdb-backup-")));
    let mut bytes = std::fs::read(&path).unwrap();
    let pos = bytes.len() - backup_file::MAGIC.len() - 9;
    bytes[pos] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    assert!(backup_file::stage(
        &mut std::fs::File::open(&path).unwrap(),
        &CancelToken::new()
    )
    .is_err());
    source.close().await;
    target.close().await;
}
fn rss(pid: u32) -> u64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}
async fn measure<T>(work: impl std::future::Future<Output = T>) -> (T, serde_json::Value) {
    let host = std::process::id();
    let child = std::process::Command::new("pgrep")
        .args(["-P", &host.to_string(), "-f", "axdb-driver-postgres"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next()?.parse::<u32>().ok());
    let stop = CancelToken::new();
    let token = stop.clone();
    let samples = Arc::new(Mutex::new((rss(host), child.map(rss).unwrap_or(0))));
    let sink = samples.clone();
    let task = tokio::spawn(async move {
        loop {
            let h = rss(host);
            let d = child.map(rss).unwrap_or(0);
            {
                let mut peak = sink.lock().unwrap();
                peak.0 = peak.0.max(h);
                peak.1 = peak.1.max(d);
            }
            tokio::select! {_=token.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
        }
    });
    let result = work.await;
    stop.cancel();
    task.await.unwrap();
    let peaks = *samples.lock().unwrap();
    (
        result,
        serde_json::json!({"host_peak_rss_bytes":peaks.0,"driver_peak_rss_bytes":peaks.1,"driver_pid":child}),
    )
}
#[tokio::test]
async fn streaming_pg_large_objects_and_memory() {
    let Ok(dsn) = std::env::var("AXDB_TEST_PG") else {
        return;
    };
    let config: tokio_postgres::Config = dsn.parse().unwrap();
    assert_eq!(
        config.get_hosts(),
        &[tokio_postgres::config::Host::Tcp("127.0.0.1".into())]
    );
    assert_eq!(config.get_ports(), &[55432]);
    let (admin, connection) = config.connect(tokio_postgres::NoTls).await.unwrap();
    let connection = tokio::spawn(connection);
    let id = std::process::id();
    let owner_role = format!("axdb_test_w4b_owner_{id}");
    admin
        .batch_execute(&format!("CREATE ROLE {owner_role}"))
        .await
        .unwrap();
    let source_db = format!("axdb_test_w4b_src_{id}");
    let small_db = format!("axdb_test_w4b_small_{id}");
    let target_db = format!("axdb_test_w4b_dst_{id}");
    admin
        .batch_execute(&format!("CREATE DATABASE {source_db} TEMPLATE dbtool_test"))
        .await
        .unwrap();
    for db in [&small_db, &target_db] {
        admin
            .batch_execute(&format!("CREATE DATABASE {db}"))
            .await
            .unwrap();
    }
    let d = driver("postgres").await;
    let cfg = |database: &str| ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("user".into(), config.get_user().unwrap().into()),
            ("database".into(), database.into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: config
            .get_password()
            .map(|p| String::from_utf8(p.to_vec()).unwrap()),
        connect_timeout_ms: 3000,
    };
    let source = d.connect(&cfg(&source_db)).await.unwrap();
    let small = d.connect(&cfg(&small_db)).await.unwrap();
    let target = d.connect(&cfg(&target_db)).await.unwrap();
    let result=AssertUnwindSafe(async {
  let dir=tempfile::tempdir().unwrap();let small_path=dir.path().join("small.axbackup");let (small_manifest,small_rss)=measure(archive(&*source,&*small,&small_path)).await;
  assert!(small_manifest.counts.contains(&("\"e2e\".\"big\"".into(),250000)));
  exec(&*source,"CREATE SCHEMA w4b; CREATE TYPE w4b.mood AS ENUM ('calm','활기'); CREATE TYPE w4b.pair AS (x integer,y text); CREATE DOMAIN w4b.positive AS bigint CHECK(VALUE>0); CREATE FUNCTION w4b.twice(v integer) RETURNS integer LANGUAGE sql IMMUTABLE AS 'SELECT v*2'; CREATE TABLE w4b.objects(id bigint GENERATED ALWAYS AS IDENTITY(START WITH 7 INCREMENT BY 3),v integer,doubled integer GENERATED ALWAYS AS(w4b.twice(v)) STORED,mood w4b.mood,coords w4b.pair,amount w4b.positive DEFAULT 1,note text DEFAULT '',PRIMARY KEY(id)); CREATE FUNCTION w4b.touch() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN NEW.note := NEW.note || ''!''; RETURN NEW; END'; CREATE TRIGGER mark BEFORE INSERT ON w4b.objects FOR EACH ROW EXECUTE FUNCTION w4b.touch(); CREATE PROCEDURE w4b.add_item(v integer) LANGUAGE sql AS 'INSERT INTO w4b.objects(v) VALUES(v)'; INSERT INTO w4b.objects(v,mood,coords) VALUES(5,'활기'::text::w4b.mood,ROW(3,'pair'));").await;
  exec(&*source,&format!("CREATE FUNCTION w4b.owner_identity() RETURNS name LANGUAGE sql SECURITY DEFINER AS 'SELECT current_user'; ALTER FUNCTION w4b.owner_identity() OWNER TO {owner_role}")).await;
  exec(&*source,"CREATE TABLE w4b.serial_items(id serial PRIMARY KEY,note text); INSERT INTO w4b.serial_items(note) VALUES('serial'); CREATE TABLE w4b.all_generated(g integer GENERATED ALWAYS AS(5) STORED); INSERT INTO w4b.all_generated DEFAULT VALUES; CREATE TABLE w4b.events(id integer CHECK(id>0),day date,mood w4b.mood) PARTITION BY RANGE(day); CREATE TABLE w4b.events_2026 PARTITION OF w4b.events FOR VALUES FROM ('2026-01-01') TO ('2027-01-01') PARTITION BY LIST(mood); CREATE TABLE w4b.events_calm PARTITION OF w4b.events_2026 FOR VALUES IN ('calm'); INSERT INTO w4b.events VALUES(1,'2026-10-19','calm'); CREATE MATERIALIZED VIEW w4b.totals AS SELECT count(*) AS n FROM w4b.events; CREATE MATERIALIZED VIEW w4b.z_total AS SELECT count(*) AS n FROM w4b.events; CREATE MATERIALIZED VIEW w4b.a_total AS SELECT n FROM w4b.z_total; CREATE VIEW w4b.z_view AS SELECT * FROM w4b.objects; CREATE VIEW w4b.a_depends_z AS SELECT * FROM w4b.z_view; CREATE EXTENSION postgres_fdw; CREATE SERVER w4b_foreign FOREIGN DATA WRAPPER postgres_fdw OPTIONS(host '127.0.0.1',port '55432',dbname 'axdb_test'); CREATE FOREIGN TABLE w4b.external(id integer) SERVER w4b_foreign; CREATE TABLE w4b.large AS SELECT i AS id,repeat(md5(i::text),40) AS payload FROM generate_series(1,120000) AS i").await;
  let size=source.execute(&request("SELECT pg_total_relation_size('w4b.large')"),CancelToken::new()).await.unwrap();let Value::Int(physical)=&size[0].table.as_ref().unwrap().rows[0][0]else{panic!("Expected table byte size");};let physical=physical.parse::<u64>().unwrap();assert!(physical>100*1024*1024,"Large fixture must occupy more than 100 MiB on PostgreSQL");
  exec(&*source,"SET DateStyle='German,DMY'; SET IntervalStyle=sql_standard; SET standard_conforming_strings=off; SET extra_float_digits=0").await;
  let path=dir.path().join("large.axbackup");let (manifest,large_rss)=measure(archive(&*source,&*target,&path)).await;assert!(manifest.bytes>150*1024*1024);assert!(manifest.warnings.iter().any(|w|w.contains("Foreign table")));assert!(manifest.warnings.iter().any(|w|w.contains("Extension")));
  let setting=source.execute(&request("SHOW DateStyle"),CancelToken::new()).await.unwrap();assert_eq!(setting[0].table.as_ref().unwrap().rows[0][0],Value::Text("German, DMY".into()));
  exec(&*source,"SET DateStyle='ISO,YMD'; SET IntervalStyle=postgres; SET standard_conforming_strings=on; SET extra_float_digits=3").await;
  for table in ["e2e.big","w4b.large","w4b.objects","w4b.serial_items","w4b.all_generated","w4b.events","w4b.totals","w4b.a_total"]{let sql=format!("SELECT count(*)::text,md5(string_agg(md5(t::text),'' ORDER BY t::text)) FROM {table} t");let a=source.execute(&request(&sql),CancelToken::new()).await.unwrap();let b=target.execute(&request(&sql),CancelToken::new()).await.unwrap();assert_eq!(a[0].table.as_ref().unwrap().rows,b[0].table.as_ref().unwrap().rows,"{table}");}
  let owner=target.execute(&request("SELECT w4b.owner_identity()::text"),CancelToken::new()).await.unwrap();assert_eq!(owner[0].table.as_ref().unwrap().rows[0][0],Value::Text(owner_role.clone()));
  exec(&*target,"CALL w4b.add_item(9)").await;let row=target.execute(&request("SELECT id,doubled,note FROM w4b.objects ORDER BY id DESC LIMIT 1"),CancelToken::new()).await.unwrap();assert_eq!(row[0].table.as_ref().unwrap().rows[0],vec![Value::Int("10".into()),Value::Int("18".into()),Value::Text("!".into())]);
  let mut staged=backup_file::stage(&mut std::fs::File::open(path).unwrap(),&CancelToken::new()).unwrap();assert!(target.restore_stream(&manifest.summary(),&mut staged.payload,false,CancelToken::new()).await.is_err());
  let evidence=serde_json::json!({"mode":if std::env::var_os("AXDB_DRIVER_DIR").is_some(){"plugin"}else{"native"},"small_payload_bytes":small_manifest.bytes,"large_payload_bytes":manifest.bytes,"large_table_physical_bytes":physical,"small":small_rss,"large":large_rss,"checked":["e2e.big 250000 rows","w4b.large 120000 rows and >100 MiB","row counts and ordered row checksums","function procedure enum composite domain trigger identity generated multilevel partitions materialized view dependent views","foreign table and extension warnings"]});
  assert!(small_rss["host_peak_rss_bytes"].as_u64().unwrap()>0&&large_rss["host_peak_rss_bytes"].as_u64().unwrap()>0,"Host RSS was not sampled");
  assert!(large_rss["host_peak_rss_bytes"].as_u64().unwrap()<=small_rss["host_peak_rss_bytes"].as_u64().unwrap()+64*1024*1024,"Host RSS grew with the archive");
  if std::env::var_os("AXDB_DRIVER_DIR").is_some(){assert!(large_rss["driver_peak_rss_bytes"].as_u64().unwrap()>0,"Driver RSS was not sampled");assert!(large_rss["driver_peak_rss_bytes"].as_u64().unwrap()<=small_rss["driver_peak_rss_bytes"].as_u64().unwrap()+64*1024*1024,"Driver RSS grew with the archive");}
  eprintln!("W4B_MEMORY {evidence}");
  let mode=if std::env::var_os("AXDB_DRIVER_DIR").is_some(){"plugin"}else{"native"};let output=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../bench/results-w4b-streaming-{mode}.json"));std::fs::write(output,serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
 }).catch_unwind().await;
    source.close().await;
    small.close().await;
    target.close().await;
    for db in [&source_db, &small_db, &target_db] {
        admin
            .batch_execute(&format!("DROP DATABASE {db} WITH (FORCE)"))
            .await
            .unwrap();
    }
    admin
        .batch_execute(&format!("DROP ROLE {owner_role}"))
        .await
        .unwrap();
    connection.abort();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
#[tokio::test]
async fn streaming_mongo_new_database_preserves_existing_target() {
    let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
        return;
    };
    assert!(uri.contains("@127.0.0.1:57017/"));
    let client = mongodb::Client::with_uri_str(&uri).await.unwrap();
    let parsed = mongodb::options::ClientOptions::parse(&uri).await.unwrap();
    let secret = parsed.credential.and_then(|c| c.password);
    let id = std::process::id();
    let source_db = format!("axdb_test_w4b_msrc_{id}");
    let target_db = format!("axdb_test_w4b_mdst_{id}");
    let new_db = format!("axdb_test_w4b_new_{id}");
    let d = driver("mongodb").await;
    let cfg = |db: &str| ConnectConfig {
        fields: BTreeMap::from([
            ("uri".into(), "mongodb://127.0.0.1:57017".into()),
            ("database".into(), db.into()),
            ("user".into(), "axdb".into()),
            ("authSource".into(), "admin".into()),
        ]),
        secret: secret.clone(),
        connect_timeout_ms: 3000,
    };
    let source = d.connect(&cfg(&source_db)).await.unwrap();
    let target = d.connect(&cfg(&target_db)).await.unwrap();
    let result=AssertUnwindSafe(async {
        use mongodb::bson::doc;
        let collection=client.database(&source_db).collection::<mongodb::bson::Document>("items");
        collection.insert_many([doc!{"_id":1,"v":"한글"},doc!{"_id":2,"v":mongodb::bson::Bson::Binary(mongodb::bson::Binary{subtype:mongodb::bson::spec::BinarySubtype::Generic,bytes:vec![0,255]})}]).await.unwrap();
        collection.create_index(mongodb::IndexModel::builder().keys(doc!{"v":1}).options(mongodb::options::IndexOptions::builder().name("w4b_by_v".to_owned()).build()).build()).await.unwrap();
        client.database(&target_db).collection::<mongodb::bson::Document>("sentinel").insert_one(doc!{"keep":true}).await.unwrap();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("mongo.axbackup");
        let manifest=backup_file::create(&*source,&path,CancelToken::new(),&Progress::new()).await.unwrap();
        let mut staged=backup_file::stage(&mut std::fs::File::open(path).unwrap(),&CancelToken::new()).unwrap();
        assert!(!target.restore_options().await.unwrap().overwrite);
        assert!(target.restore_stream(&manifest.summary(),&mut staged.payload,true,CancelToken::new()).await.is_err());
        use std::io::{Seek,SeekFrom};staged.payload.seek(SeekFrom::Start(0)).unwrap();
        target.restore_database(&manifest.summary(),&mut staged.payload,&new_db,CancelToken::new()).await.unwrap();
        assert_eq!(client.database(&new_db).collection::<mongodb::bson::Document>("items").count_documents(doc!{}).await.unwrap(),2);
        assert_eq!(client.database(&target_db).collection::<mongodb::bson::Document>("sentinel").count_documents(doc!{}).await.unwrap(),1);
        let restored=d.connect(&cfg(&new_db)).await.unwrap();
        let copy=backup_file::create(&*restored,&dir.path().join("restored.axbackup"),CancelToken::new(),&Progress::new()).await.unwrap();
        assert_eq!(copy.counts,manifest.counts);assert_eq!(copy.sha256,manifest.sha256);
        restored.close().await;
    }).catch_unwind().await;
    source.close().await;
    target.close().await;
    for db in [&source_db, &target_db, &new_db] {
        client.database(db).drop().await.unwrap();
    }
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
#[tokio::test]
async fn streaming_neo4j_empty_target_and_community_guidance() {
    let Ok(value) = std::env::var("AXDB_TEST_NEO4J") else {
        return;
    };
    let Ok(target_uri) = std::env::var("AXDB_TEST_NEO4J_RESTORE") else {
        return;
    };
    let parts = value.split('|').collect::<Vec<_>>();
    assert_eq!(parts[0], "bolt://127.0.0.1:57687");
    assert_eq!(target_uri, "bolt://127.0.0.1:57688");
    let d = driver("neo4j").await;
    let cfg = |uri: &str| ConnectConfig {
        fields: BTreeMap::from([("uri".into(), uri.into()), ("user".into(), parts[1].into())]),
        secret: Some(parts[2].into()),
        connect_timeout_ms: 5000,
    };
    let source = d.connect(&cfg(parts[0])).await.unwrap();
    let target = d.connect(&cfg(&target_uri)).await.unwrap();
    let label = format!("AxdbW4b{}", std::process::id());
    let result=AssertUnwindSafe(async{
  exec(&*source,&format!("CREATE (a:{label} {{name:'한글',number:9007199254740993}})-[:W4B_LINK {{score:1.25}}]->(b:{label} {{name:'two'}})")).await;
  let options=source.restore_options().await.unwrap();assert!(!options.overwrite);assert!(!options.new_database);assert!(options.guidance.contains("Community"));
  let dir=tempfile::tempdir().unwrap();let path=dir.path().join("neo4j.axbackup");let manifest=archive(&*source,&*target,&path).await;
  let sql=format!("MATCH(n:{label}) RETURN n.name,n.number ORDER BY n.name");let a=source.execute(&request(&sql),CancelToken::new()).await.unwrap();let b=target.execute(&request(&sql),CancelToken::new()).await.unwrap();assert_eq!(a[0].table.as_ref().unwrap().rows,b[0].table.as_ref().unwrap().rows);
  let sql=format!("MATCH(:{label})-[r:W4B_LINK]->(:{label}) RETURN count(r),sum(r.score)");let a=source.execute(&request(&sql),CancelToken::new()).await.unwrap();let b=target.execute(&request(&sql),CancelToken::new()).await.unwrap();assert_eq!(a[0].table.as_ref().unwrap().rows,b[0].table.as_ref().unwrap().rows);
  let mut staged=backup_file::stage(&mut std::fs::File::open(path).unwrap(),&CancelToken::new()).unwrap();assert!(target.restore_stream(&manifest.summary(),&mut staged.payload,true,CancelToken::new()).await.is_err());assert!(target.restore_database(&manifest.summary(),&mut staged.payload,"w4b_new",CancelToken::new()).await.is_err());
 }).catch_unwind().await;
    let cleanup = driver("neo4j").await;
    let a = cleanup.connect(&cfg(parts[0])).await.unwrap();
    let b = cleanup.connect(&cfg(&target_uri)).await.unwrap();
    exec(&*a, &format!("MATCH(n:{label}) DETACH DELETE n")).await;
    exec(&*b, "MATCH(n) DETACH DELETE n").await;
    for o in b.schema_snapshot(None).await.unwrap().objects {
        if ["constraint", "index"].contains(&o.kind.as_str()) && !o.definition.contains("LOOKUP") {
            exec(
                &*b,
                &format!(
                    "DROP {} `{}` IF EXISTS",
                    o.kind.to_uppercase(),
                    o.name.replace('`', "``")
                ),
            )
            .await;
        }
    }
    a.close().await;
    b.close().await;
    source.close().await;
    target.close().await;
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
