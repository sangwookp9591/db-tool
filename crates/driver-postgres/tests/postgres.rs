use dbcore::*;
use driver_postgres::PostgresDriver;
use futures_util::FutureExt;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

fn config() -> Option<ConnectConfig> {
    let dsn = std::env::var("DBTOOL_TEST_PG").ok()?;
    let pg: tokio_postgres::Config = dsn.parse().expect("valid test DSN");
    assert_eq!(
        pg.get_hosts(),
        &[tokio_postgres::config::Host::Tcp("127.0.0.1".into())],
        "test only connects to loopback"
    );
    assert_eq!(
        pg.get_ports(),
        &[55432],
        "test only connects to the isolated test port"
    );
    Some(ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), pg.get_dbname().unwrap().into()),
            ("user".into(), pg.get_user().unwrap().into()),
            ("sslmode".into(), "prefer".into()),
        ]),
        secret: pg
            .get_password()
            .map(|s| String::from_utf8(s.to_vec()).unwrap()),
        connect_timeout_ms: 2000,
    })
}
fn req(text: &str) -> ExecRequest {
    ExecRequest {
        read_only: false,
        text: text.into(),
        params: Default::default(),
        row_limit: 1000,
        graph_node_limit: 300,
        timeout_ms: 30_000,
    }
}
async fn run(s: &dyn Session, text: &str) -> Vec<ResultSet> {
    s.execute(&req(text), CancelToken::new()).await.unwrap()
}

#[tokio::test]
async fn real_postgres_contract() {
    let Some(cfg) = config() else {
        eprintln!("DBTOOL_TEST_PG unset: real PostgreSQL test not run");
        return;
    };
    let driver = PostgresDriver::new();
    let s: Arc<dyn Session> = Arc::from(driver.connect(&cfg).await.unwrap());
    s.ping().await.unwrap();
    assert!(s.server_info().await.unwrap().contains("PostgreSQL"));
    let result=run(&*s,"SELECT 9223372036854775807::int8 AS n, 12345678901234567890.123456789::numeric AS d, NULL::text AS nil, '2026-10-15 12:34:56+09'::timestamptz AS ts, '{\"big\":9223372036854775807}'::jsonb AS j, decode('00ffabcd','hex') AS b, '12345678-1234-5678-9012-123456789abc'::uuid AS u, ARRAY[1,2]::int4[] AS a, 'NaN'::float8 AS f").await;
    let table = result[0].table.as_ref().unwrap();
    assert_eq!(table.columns[0].type_name, "int8");
    let row = &table.rows[0];
    assert_eq!(row[0], Value::Int("9223372036854775807".into()));
    assert_eq!(
        row[1],
        Value::Decimal("12345678901234567890.123456789".into())
    );
    assert_eq!(row[2], Value::Null);
    assert!(matches!(&row[3],Value::DateTimeTz(s) if s.contains("2026-10-15")));
    assert!(matches!(&row[4],Value::Json(s) if s.contains("9223372036854775807")));
    assert_eq!(
        row[5],
        Value::Bytes {
            len: 4,
            hex: "00ffabcd".into()
        }
    );
    assert!(matches!(&row[6], Value::Uuid(_)));
    assert_eq!(
        row[7],
        Value::Other {
            type_name: "int4[]".into(),
            text: "{1,2}".into()
        }
    );
    assert_eq!(row[8], Value::Float(FloatValue::Special("NaN".into())));
    assert_eq!(
        run(&*s, "SELECT 1 -- trailing comment").await[0].row_count,
        1
    );
    let limited = s
        .execute(
            &ExecRequest {
                read_only: false,
                row_limit: 2,
                ..req("SELECT generate_series(1,1000000)")
            },
            CancelToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(limited[0].row_count, 2);
    assert!(limited[0].truncated);
    let error = s
        .execute(&req("SELECT bad syntax here"), CancelToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Query);
    assert!(error.position.is_some());
    assert!(!error.message.is_empty());
    let started = Instant::now();
    let err = s
        .execute(
            &ExecRequest {
                read_only: false,
                timeout_ms: 500,
                ..req("SELECT pg_sleep(10)")
            },
            CancelToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(run(&*s, "SELECT 1").await[0].row_count, 1);
    let cancel = CancelToken::new();
    let c = cancel.clone();
    let started = Instant::now();
    let sleeper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        c.cancel();
    });
    let err = s
        .execute(&req("SELECT pg_sleep(10)"), cancel)
        .await
        .unwrap_err();
    sleeper.await.unwrap();
    assert_eq!(err.kind, ErrorKind::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(run(&*s, "SELECT 1").await[0].row_count, 1);
    // Exercise cancellation at prepare/BEGIN/FETCH boundaries as well as a running query.
    for _ in 0..8 {
        let token = CancelToken::new();
        let trigger = token.clone();
        let stop = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1)).await;
            trigger.cancel();
        });
        let start = Instant::now();
        let error = s
            .execute(&req("SELECT pg_sleep(10); SELECT pg_sleep(10)"), token)
            .await
            .unwrap_err();
        stop.await.unwrap();
        assert_eq!(error.kind, ErrorKind::Cancelled);
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(run(&*s, "SELECT 1").await[0].row_count, 1);
    }
    let name = format!("dbtool_w1_{}", std::process::id());
    run(&*s,&format!("CREATE SCHEMA {name}; CREATE TABLE {name}.items(id int PRIMARY KEY, value text); INSERT INTO {name}.items SELECT i, 'v'||i FROM generate_series(1,5) i")).await;
    let schemas = s.list_children(None).await.unwrap();
    assert!(!schemas.iter().any(|n| n.label == "pg_catalog"
        || n.label == "information_schema"
        || n.label.starts_with("pg_toast")));
    let schema = schemas.iter().find(|n| n.label == name).unwrap();
    let folders = s.list_children(Some(&schema.id)).await.unwrap();
    let tables = s
        .list_children(Some(
            &folders.iter().find(|n| n.label == "Tables").unwrap().id,
        ))
        .await
        .unwrap();
    let table = tables.iter().find(|n| n.label == "items").unwrap();
    assert_eq!(table.actions.len(), 2);
    let children = s.list_children(Some(&table.id)).await.unwrap();
    let cols = s
        .list_children(Some(
            &children.iter().find(|n| n.label == "Columns").unwrap().id,
        ))
        .await
        .unwrap();
    assert!(cols[0].detail.as_ref().unwrap().contains("PK"));
    let indexes = s
        .list_children(Some(
            &children.iter().find(|n| n.label == "Indexes").unwrap().id,
        ))
        .await
        .unwrap();
    assert!(indexes[0]
        .detail
        .as_ref()
        .unwrap()
        .contains("CREATE UNIQUE INDEX"));
    let p = s
        .browse(
            &table.id,
            PageRequest {
                offset: 0,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert!(p.page.unwrap().has_more);
    assert_eq!(p.row_count, 2);
    let p = s
        .browse(
            &table.id,
            PageRequest {
                offset: 4,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert!(!p.page.unwrap().has_more);
    assert_eq!(p.table.unwrap().rows[0][0], Value::Int("5".into()));
    let token = CancelToken::new();
    let stop_token = token.clone();
    let request = req("SELECT pg_sleep(10)");
    let pending = s.execute(&request, token);
    let concurrent_browse = s.browse(
        &table.id,
        PageRequest {
            offset: 0,
            limit: 2,
        },
    );
    let stop = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        stop_token.cancel();
    };
    let (cancelled, browsed, _) = tokio::join!(pending, concurrent_browse, stop);
    assert_eq!(cancelled.unwrap_err().kind, ErrorKind::Cancelled);
    assert_eq!(browsed.unwrap().row_count, 2);
    let returning = s.execute(&ExecRequest {row_limit: 2, ..req(&format!("INSERT INTO {name}.items SELECT i, 'r' FROM generate_series(10,14) i RETURNING id"))}, CancelToken::new()).await.unwrap();
    assert!(returning[0].truncated);
    assert_eq!(returning[0].row_count, 2);
    let deleted = run(&*s, &format!("DELETE FROM {name}.items WHERE id>=10")).await;
    assert_eq!(deleted[0].affected, Some(5));
    run(&*s, "BEGIN").await;
    run(
        &*s,
        &format!("INSERT INTO {name}.items VALUES (99,'uncommitted')"),
    )
    .await;
    run(&*s, "SELECT 1").await;
    run(&*s, "ROLLBACK").await;
    let r = run(
        &*s,
        &format!("SELECT count(*) FROM {name}.items WHERE id=99"),
    )
    .await;
    assert_eq!(
        r[0].table.as_ref().unwrap().rows[0][0],
        Value::Int("0".into())
    );
    run(&*s, &format!("DROP SCHEMA {name} CASCADE")).await;
    s.close().await;
    assert_eq!(s.ping().await.unwrap_err().kind, ErrorKind::Connection);
    let terminated = driver.connect(&cfg).await.unwrap();
    assert_eq!(
        terminated
            .execute(
                &req("SELECT pg_terminate_backend(pg_backend_pid())"),
                CancelToken::new()
            )
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Connection
    );
    let mut bad = cfg.clone();
    bad.secret = Some("definitely-wrong-dbtool-password".into());
    let e = driver
        .connect(&bad)
        .await
        .err()
        .expect("wrong password must fail");
    assert_eq!(e.kind, ErrorKind::Auth);
    assert!(!e.message.contains("definitely-wrong"));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    bad.fields.insert("port".into(), port.to_string());
    assert_eq!(
        driver.connect(&bad).await.err().unwrap().kind,
        ErrorKind::Connection
    );
}

#[tokio::test]
async fn read_only_execution_is_enforced_and_cleans_up() {
    let Some(cfg) = config() else {
        eprintln!("DBTOOL_TEST_PG unset: live read-only test not run");
        return;
    };
    let driver = PostgresDriver::new();
    assert!(driver.manifest().capabilities.read_only_exec);
    let s = driver.connect(&cfg).await.unwrap();
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("agent_ro_{}_{suffix}", std::process::id());
    let checked = std::panic::AssertUnwindSafe(async {
        run(&*s, &format!("CREATE TABLE {name}(id integer)")).await;
        for text in [
            format!("CREATE TABLE {name}_blocked(id integer)"),
            format!("INSERT INTO {name} VALUES(1)"),
        ] {
            let error = s
                .execute(
                    &ExecRequest {
                        read_only: true,
                        ..req(&text)
                    },
                    CancelToken::new(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.kind, ErrorKind::Query);
            assert!(error.message.contains("read-only"), "{error}");
        }
        let result = s
            .execute(
                &ExecRequest {
                    read_only: true,
                    ..req("SELECT 1; SELECT 2")
                },
                CancelToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        for text in [
            "COMMIT; SELECT 1",
            "SET TRANSACTION READ WRITE",
            &format!("ROLLBACK; CREATE TABLE {name}_escaped(id integer)"),
        ] {
            assert_eq!(
                s.execute(
                    &ExecRequest {
                        read_only: true,
                        ..req(text)
                    },
                    CancelToken::new()
                )
                .await
                .unwrap_err()
                .kind,
                ErrorKind::Unsupported
            );
        }
        for text in [
            "SELECT set_config('transaction_read_only','off',true)",
            "DO $$ BEGIN EXECUTE 'SET TRANSACTION READ WRITE'; END $$",
        ] {
            assert_eq!(
                s.execute(
                    &ExecRequest {
                        read_only: true,
                        ..req(text)
                    },
                    CancelToken::new()
                )
                .await
                .unwrap_err()
                .kind,
                ErrorKind::Query
            );
        }
        run(&*s, "BEGIN").await;
        assert_eq!(
            s.execute(
                &ExecRequest {
                    read_only: true,
                    ..req("SELECT 1")
                },
                CancelToken::new()
            )
            .await
            .unwrap_err()
            .kind,
            ErrorKind::Unsupported
        );
        run(&*s, "ROLLBACK").await;
        let error = s
            .execute(
                &ExecRequest {
                    read_only: true,
                    timeout_ms: 50,
                    ..req("SELECT pg_sleep(10)")
                },
                CancelToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        run(&*s, &format!("INSERT INTO {name} VALUES(2)")).await;
    })
    .catch_unwind()
    .await;
    s.close().await;
    let cleanup = driver.connect(&cfg).await.unwrap();
    let cleaned = cleanup
        .execute(
            &req(&format!(
                "DROP TABLE IF EXISTS {name}, {name}_blocked, {name}_escaped"
            )),
            CancelToken::new(),
        )
        .await;
    cleanup.close().await;
    if let Err(panic) = checked {
        std::panic::resume_unwind(panic);
    }
    cleaned.unwrap();
}

#[tokio::test]
async fn cancellation_races_do_not_poison_next_query() {
    let Some(cfg) = config() else {
        return;
    };
    let session = PostgresDriver::new().connect(&cfg).await.unwrap();
    for i in 0..200 {
        let token = CancelToken::new();
        let trigger = token.clone();
        let stop = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_micros((i % 10) * 100)).await;
            trigger.cancel();
        });
        let text = if i % 2 == 0 {
            "SELECT 1"
        } else {
            "SELECT pg_sleep(0.001)"
        };
        let result = session.execute(&req(text), token).await;
        if let Err(error) = result {
            assert_eq!(error.kind, ErrorKind::Cancelled);
        }
        stop.await.unwrap();
        assert_eq!(
            run(&*session, "SELECT 42").await[0].row_count,
            1,
            "iteration {i}"
        );
    }
    session.close().await;
}
