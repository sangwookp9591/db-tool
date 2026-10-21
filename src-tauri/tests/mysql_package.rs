//! A newly signed driver package works through the unchanged generic package/host APIs.
// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
use axdb::{driver_install::host_target, plugin_host::PluginDriver};
use axdb_driver_package::{self as package, Manifest, Platform, SigningKey, TrustedKey};
use dbcore::*;
use std::{collections::BTreeMap, sync::Arc, time::Instant};
fn req(sql: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text":sql,"row_limit":100000})).unwrap()
}
fn config() -> ConnectConfig {
    ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "53306".into()),
            ("user".into(), "root".into()),
            ("database".into(), "axdb_test".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testmysql".into()),
        connect_timeout_ms: 5000,
    }
}
#[tokio::test]
async fn signed_mysql_package_loads_without_host_driver_registration() {
    let Some(dir) = std::env::var_os("AXDB_DRIVER_DIR") else {
        eprintln!("AXDB_DRIVER_DIR unset: package runtime not checked");
        return;
    };
    let executable = std::path::PathBuf::from(dir)
        .join(format!("axdb-driver-mysql{}", std::env::consts::EXE_SUFFIX));
    let binary = std::fs::read(&executable).expect("build workspace binaries first");
    let target = host_target().to_owned();
    let manifest = Manifest {
        id: "mysql".into(),
        icon_key: Some("mysql".into()),
        name: "MySQL / MariaDB".into(),
        version: "0.2.0".into(),
        protocol: 1,
        description: "New installable MySQL driver".into(),
        paradigm: "Relational".into(),
        homepage: String::new(),
        license: "MIT".into(),
        platforms: BTreeMap::from([(
            target.clone(),
            Platform {
                path: format!(
                    "bin/{target}/axdb-driver-mysql{}",
                    std::env::consts::EXE_SUFFIX
                ),
                sha256: package::sha256(&binary),
            },
        )]),
    };
    // Public fixture key has no release authority; it is trusted only by this test's local key list.
    let key = SigningKey::from_bytes(&[42; 32]);
    let bytes = package::write_package(
        &manifest,
        &BTreeMap::from([(target.clone(), binary)]),
        Some(&key),
    )
    .unwrap();
    if let Some(path) = std::env::var_os("AXDB_MYSQL_PACKAGE_OUT") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &bytes).unwrap();
    }
    assert!(package::read_package(&bytes, &[], false).is_err());
    let verified = package::read_package(
        &bytes,
        &[TrustedKey {
            public_key: key.verifying_key().to_bytes(),
        }],
        false,
    )
    .unwrap();
    assert!(verified.trusted);
    assert!(verified.manifest.icon_key.is_none());
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("mysql-0.2.0.axdriver"), &bytes).unwrap();
    let staged = verified.stage(root.path(), &target).unwrap();
    let driver = Arc::new(PluginDriver::load(staged.executable.clone()).await.unwrap());
    let registry = Registry::new();
    registry.register(driver.clone());
    assert_eq!(registry.manifests()[0].id, "mysql");
    assert_eq!(registry.manifests()[0].icon_key.as_deref(), Some("mysql"));
    staged.commit();
    if std::env::var_os("AXDB_TEST_MYSQL").is_some() {
        let session = driver.connect(&config()).await.unwrap();
        session.ping().await.unwrap();
        assert!(session.server_info().await.unwrap().contains("8.4"));
        let result = session
            .execute(
                &req("SELECT CAST(18446744073709551615 AS UNSIGNED)"),
                CancelToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            result[0].table.as_ref().unwrap().rows[0][0],
            Value::Int(u64::MAX.to_string())
        );
        session.close().await;
    }
}
struct TimingSink {
    start: Instant,
    state: std::sync::Mutex<(Option<std::time::Duration>, usize)>,
}
impl ResultSink for TimingSink {
    fn start(&self, _index: usize, _result: &ResultSet) -> Result<(), DbError> {
        Ok(())
    }
    fn rows(&self, _index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        let mut state = self.state.lock().unwrap();
        state.0.get_or_insert_with(|| self.start.elapsed());
        state.1 += rows.len();
        Ok(())
    }
}
async fn measure(session: &dyn Session, request: &ExecRequest) -> serde_json::Value {
    let sink = TimingSink {
        start: Instant::now(),
        state: std::sync::Mutex::new((None, 0)),
    };
    let results = session
        .execute_stream(request, CancelToken::new(), &sink)
        .await
        .unwrap();
    let total = sink.start.elapsed();
    assert_eq!(results[0].row_count, 100000);
    assert!(results[0].table.as_ref().unwrap().rows.is_empty());
    let state = sink.state.lock().unwrap();
    assert_eq!(state.1, 100000);
    serde_json::json!({"first_rows_ms":state.0.unwrap().as_secs_f64()*1000.,"total_ms":total.as_secs_f64()*1000.})
}
#[tokio::test]
#[ignore = "manual MySQL 100k measurement after R2 quiet window; requires both test flags"]
async fn mysql_100k_plugin_measurement() {
    unsafe extern "C" {
        fn mi_option_set(option: std::ffi::c_int, value: std::ffi::c_long);
    }
    // Match the app and SDK's immediate purge policy before paired measurements.
    unsafe {
        mi_option_set(15, 0);
    }

    assert!(std::env::var_os("AXDB_TEST_MYSQL").is_some());
    let dir = std::env::var_os("AXDB_DRIVER_DIR").expect("AXDB_DRIVER_DIR");
    let driver = PluginDriver::load(
        std::path::PathBuf::from(dir)
            .join(format!("axdb-driver-mysql{}", std::env::consts::EXE_SUFFIX)),
    )
    .await
    .unwrap();
    let session = driver.connect_plugin(&config()).await.unwrap();
    let name = format!("axdb_m1_bench_{}", std::process::id());
    session.execute(&req(&format!("CREATE TABLE `{name}`(id BIGINT PRIMARY KEY,payload VARCHAR(32)) ENGINE=InnoDB; INSERT INTO `{name}` SELECT a.n+10*b.n+100*c.n+1000*d.n+10000*e.n,REPEAT('x',32) FROM (SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) a CROSS JOIN (SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) b CROSS JOIN (SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) c CROSS JOIN (SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) d CROSS JOIN (SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) e")),CancelToken::new()).await.unwrap();
    let read = req(&format!("SELECT * FROM `{name}` ORDER BY id"));
    let direct = driver_mysql::MysqlDriver::new()
        .connect(&config())
        .await
        .unwrap();
    let mut samples = vec![];
    for i in 0..4 {
        let (in_process, plugin) = if i % 2 == 0 {
            (
                measure(&*direct, &read).await,
                measure(&session, &read).await,
            )
        } else {
            let plugin = measure(&session, &read).await;
            (measure(&*direct, &read).await, plugin)
        };
        if i > 0 {
            samples.push(serde_json::json!({"in_process":in_process,"plugin":plugin}));
        }
    }
    direct.close().await;
    let processes = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,comm="])
        .output()
        .unwrap();
    let processes = String::from_utf8(processes.stdout).unwrap();
    let rss = processes
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse::<u32>().ok()?;
            let ppid = parts.next()?.parse::<u32>().ok()?;
            let rss_kib = parts.next()?.parse::<u64>().ok()?;
            let command = parts.collect::<Vec<_>>().join(" ");
            if ppid == std::process::id() && command.ends_with("axdb-driver-mysql") {
                Some(serde_json::json!({"pid":pid,"rss_kib":rss_kib}))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    session
        .execute(&req(&format!("DROP TABLE `{name}`")), CancelToken::new())
        .await
        .unwrap();
    session.close().await;
    let footprints=rss.iter().filter_map(|p|p["pid"].as_u64()).map(|pid| {
        let output=std::process::Command::new("/usr/bin/footprint").args(["-p",&pid.to_string(),"--noCategories","-f","bytes"]).output();
        match output {Ok(o)=>serde_json::json!({"pid":pid,"exit_code":o.status.code(),"output":String::from_utf8_lossy(&o.stdout),"error":String::from_utf8_lossy(&o.stderr)}),Err(e)=>serde_json::json!({"pid":pid,"error":e.to_string()})}
    }).collect::<Vec<_>>();
    let report = serde_json::json!({"allocator_policy":"mimalloc purge_delay=0 (app and SDK policy)","physical_footprint":footprints,"database":"MySQL 8.4 test container","profile":"debug","rows":100000,"boundary":"Session ResultSink first row chunk; DB collects before first chunk; excludes UI paint","samples":samples,"driver_processes_after_fetch":rss});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bench/results-m1-mysql.json");
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{report}");
}
