// Integration tests fail by panicking: unwrap/expect are the intended assertion style here
// (clippy's allow-*-in-tests only covers #[test] functions, not shared helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use dbcore::*;
use driver_postgres::PostgresDriver;
use futures_util::FutureExt;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

fn config() -> Option<ConnectConfig> {
    let dsn = std::env::var("AXDB_TEST_PG").ok()?;
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
        preview_sample_limit: default_preview_sample_limit(),
    }
}
async fn run(s: &dyn Session, text: &str) -> Vec<ResultSet> {
    s.execute(&req(text), CancelToken::new()).await.unwrap()
}

#[tokio::test]
async fn real_postgres_contract() {
    let Some(cfg) = config() else {
        eprintln!("AXDB_TEST_PG unset: real PostgreSQL test not run");
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
    let name = format!("axdb_w1_{}", std::process::id());
    run(&*s,&format!("CREATE SCHEMA {name}; CREATE TABLE {name}.items(id int PRIMARY KEY, value text); INSERT INTO {name}.items SELECT i, 'v'||i FROM generate_series(1,5) i")).await;
    let schemas = s.list_children(None).await.unwrap();
    assert!(!schemas.iter().any(|n| n.label == "pg_catalog"
        || n.label == "information_schema"
        || n.label.starts_with("pg_toast")));
    let schema = schemas.iter().find(|n| n.label == name).unwrap();
    let folders = s.list_children(Some(&schema.id)).await.unwrap();
    let tables = s
        .list_children(Some(
            &folders.iter().find(|n| n.label == "테이블").unwrap().id,
        ))
        .await
        .unwrap();
    let table = tables.iter().find(|n| n.label == "items").unwrap();
    assert_eq!(table.actions.len(), 2);
    let children = s.list_children(Some(&table.id)).await.unwrap();
    let cols = s
        .list_children(Some(
            &children.iter().find(|n| n.label == "컬럼").unwrap().id,
        ))
        .await
        .unwrap();
    assert!(cols[0].detail.as_ref().unwrap().contains("PK"));
    let indexes = s
        .list_children(Some(
            &children.iter().find(|n| n.label == "인덱스").unwrap().id,
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
            PageRequest { filter: vec![],
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
            PageRequest { filter: vec![],
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
        PageRequest { filter: vec![],
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
    bad.secret = Some("definitely-wrong-axdb-password".into());
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
async fn large_privilege_matrix_requires_role_and_pages_one_role() {
    let Some(cfg) = config() else { return; };
    let suffix = format!("{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let schema = format!("axdb_roles_{suffix}");
    let role = format!("axdb_role_{suffix}");
    let s = PostgresDriver::new().connect(&cfg).await.unwrap();
    run(&*s, &format!("CREATE ROLE {role}; CREATE SCHEMA {schema}; DO $$ BEGIN FOR i IN 1..800 LOOP EXECUTE format('CREATE TABLE %I.%I (id integer)', '{schema}', 't_' || lpad(i::text,4,'0')); END LOOP; END $$;")).await;

    let observed = async {
        let legacy = s.roles().await?;
        let query = RolesPageQuery { role: Some(role.clone()), namespace: Some(schema.clone()), object: None, offset: 0, limit: 100 };
        let first = s.roles_page(&query).await?;
        let next = s.roles_page(&RolesPageQuery { offset: 100, ..query.clone() }).await?;
        let filtered = s.roles_page(&RolesPageQuery { object: Some("t_0001".into()), ..query.clone() }).await?;
        Ok::<_, DbError>((legacy, first, next, filtered))
    }.await;
    let _cleanup = run(&*s, &format!("DROP SCHEMA IF EXISTS {schema} CASCADE; DROP ROLE IF EXISTS {role};")).await;
    s.close().await;
    let (legacy, first, next, filtered) = observed.unwrap();
    assert!(legacy.supported);
    assert!(legacy.matrix.is_empty(), "large legacy snapshots must not materialize a cross product");
    assert!(legacy.page.as_ref().is_some_and(|p| p.role_required));
    assert!(legacy.roles.iter().any(|r| r.name == role));
    assert_eq!(first.matrix.len(), 100);
    assert_eq!(first.page.as_ref().unwrap().total, 800 * 7);
    assert!(first.page.as_ref().unwrap().has_more);
    assert!(first.matrix.iter().all(|p| p.role == role && p.namespace == schema));
    assert_eq!(next.matrix.len(), 100);
    assert_ne!(first.matrix.last().unwrap(), next.matrix.first().unwrap());
    assert_eq!(filtered.matrix.len(), 7);
    assert_eq!(filtered.page.as_ref().unwrap().total, 7);
    assert!(!filtered.page.as_ref().unwrap().has_more);
}

#[tokio::test]
async fn read_only_execution_is_enforced_and_cleans_up() {
    let Some(cfg) = config() else {
        eprintln!("AXDB_TEST_PG unset: live read-only test not run");
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

#[tokio::test]
async fn change_preview_rolls_back_rows_schema_and_preserves_user_transaction() {
    let Some(cfg) = config() else {
        return;
    };
    assert_eq!(
        cfg.fields.get("database").map(String::as_str),
        Some("axdb_test")
    );
    let s = PostgresDriver::new().connect(&cfg).await.unwrap();
    let name = format!(
        "preview_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    s.execute(&req(&format!("CREATE SCHEMA {name}; CREATE TABLE {name}.items(id integer primary key, n text); INSERT INTO {name}.items VALUES(1,'before'),(2,'two'),(3,'three'); CREATE INDEX items_n ON {name}.items(n)")), CancelToken::new()).await.unwrap();
    let snapshot = || req(&format!("SELECT * FROM {name}.items ORDER BY id"));
    let original = s.execute(&snapshot(), CancelToken::new()).await.unwrap()[0]
        .table
        .clone();
    for (sql, affected, before, after) in [
        (
            format!("UPDATE {name}.items SET n='after' WHERE id=1"),
            1,
            true,
            true,
        ),
        (
            format!("DELETE FROM {name}.items WHERE id=1"),
            1,
            true,
            false,
        ),
        (
            format!("INSERT INTO {name}.items VALUES(4,'four'),(5,'five')"),
            2,
            false,
            true,
        ),
    ] {
        let mut r = req(&sql);
        r.preview_sample_limit = 1;
        let p = s.preview(&r, CancelToken::new()).await.unwrap();
        assert!(!p.committed);
        let st = &p.statements[0];
        assert_eq!(st.affected, Some(affected));
        assert_eq!(st.before.is_some(), before);
        assert_eq!(st.after.is_some(), after);
        for table in [&st.before, &st.after].into_iter().flatten() {
            assert!(table.rows.len() <= 1);
        }
        if before {
            assert_eq!(
                st.before.as_ref().unwrap().rows[0][1],
                Value::Text("before".into())
            );
        }
        if sql.starts_with("UPDATE") {
            assert_eq!(
                st.after.as_ref().unwrap().rows[0][1],
                Value::Text("after".into())
            );
        }
        assert_eq!(
            s.execute(&snapshot(), CancelToken::new()).await.unwrap()[0].table,
            original
        );
    }
    for sql in [
        format!("ALTER TABLE {name}.items ADD COLUMN extra int"),
        format!("CREATE TABLE {name}.new_table(x int)"),
        format!("DROP INDEX {name}.items_n"),
    ] {
        let p = s.preview(&req(&sql), CancelToken::new()).await.unwrap();
        let diff = p.statements[0].schema_diff.as_ref().unwrap();
        assert!(!diff.added.is_empty() || !diff.removed.is_empty() || !diff.changed.is_empty());
    }
    let schema = s.execute(&req(&format!("SELECT column_name FROM information_schema.columns WHERE table_schema='{name}' ORDER BY table_name,ordinal_position; SELECT indexname FROM pg_indexes WHERE schemaname='{name}' ORDER BY indexname")),CancelToken::new()).await.unwrap();
    assert_eq!(schema[0].row_count, 2);
    assert_eq!(schema[1].row_count, 2);
    for sql in [
        "VACUUM",
        "CREATE DATABASE axdb_preview_must_not_exist",
        &format!("CREATE INDEX CONCURRENTLY denied ON {name}.items(n)"),
        &format!("UPDATE {name}.items SET n='bad'; COMMIT"),
    ] {
        let p = s.preview(&req(sql), CancelToken::new()).await.unwrap();
        assert!(!p.notes.is_empty());
        assert!(p.statements.iter().all(|s| s.affected.is_none()));
    }
    s.execute(
        &req(&format!("BEGIN; INSERT INTO {name}.items VALUES(4,'user')")),
        CancelToken::new(),
    )
    .await
    .unwrap();
    s.preview(
        &req(&format!("DELETE FROM {name}.items")),
        CancelToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        s.execute(&snapshot(), CancelToken::new()).await.unwrap()[0].row_count,
        4
    );
    s.execute(&req("ROLLBACK"), CancelToken::new())
        .await
        .unwrap();
    for timeout in [true, false] {
        let token = CancelToken::new();
        let task_token = token.clone();
        let mut r = req(&format!(
            "UPDATE {name}.items SET n='bad'; SELECT pg_sleep(10)"
        ));
        if timeout {
            r.timeout_ms = 50;
        } else {
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                task_token.cancel();
            });
        }
        let e = s.preview(&r, token).await.unwrap_err();
        assert_eq!(
            e.kind,
            if timeout {
                ErrorKind::Timeout
            } else {
                ErrorKind::Cancelled
            }
        );
        assert_eq!(
            s.execute(&snapshot(), CancelToken::new()).await.unwrap()[0].table,
            original
        );
        // A fresh implicit transaction must not leave the session in read-only/failed transaction state.
        s.execute(
            &req(&format!(
                "BEGIN; INSERT INTO {name}.items VALUES(6,'clean'); ROLLBACK"
            )),
            CancelToken::new(),
        )
        .await
        .unwrap();
    }
    s.execute(
        &req(&format!("BEGIN; INSERT INTO {name}.items VALUES(4,'keep')")),
        CancelToken::new(),
    )
    .await
    .unwrap();
    let mut nested = req(&format!(
        "UPDATE {name}.items SET n='bad'; SELECT pg_sleep(10)"
    ));
    nested.timeout_ms = 50;
    assert_eq!(
        s.preview(&nested, CancelToken::new())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    let preserved = s.execute(&snapshot(), CancelToken::new()).await.unwrap();
    assert_eq!(preserved[0].row_count, 4);
    assert_eq!(
        preserved[0].table.as_ref().unwrap().rows[0][1],
        Value::Text("before".into())
    );
    s.execute(&req("ROLLBACK"), CancelToken::new())
        .await
        .unwrap();
    assert!(s
        .preview(
            &req(&format!(
                "UPDATE {name}.items SET n='bad'; SELECT missing_preview_column"
            )),
            CancelToken::new()
        )
        .await
        .is_err());
    assert_eq!(
        s.execute(&snapshot(), CancelToken::new()).await.unwrap()[0].table,
        original
    );
    s.execute(
        &req(&format!("DROP SCHEMA {name} CASCADE")),
        CancelToken::new(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn schema_graph_and_filtered_browse() {
    let Some(cfg)=config() else {eprintln!("AXDB_TEST_PG unset: V1 integration test not run");return;};
    let s=PostgresDriver::new().connect(&cfg).await.unwrap();
    run(&*s,"DROP SCHEMA IF EXISTS e2e_fk CASCADE; CREATE SCHEMA e2e_fk; CREATE TABLE e2e_fk.parent(tenant text, id bigint, name text, PRIMARY KEY(tenant,id)); CREATE TABLE e2e_fk.child(id int PRIMARY KEY,tenant text,parent_id bigint,FOREIGN KEY(tenant,parent_id) REFERENCES e2e_fk.parent(tenant,id)); INSERT INTO e2e_fk.parent VALUES('a',9223372036854775807,'exact'),('b',1,'other'); INSERT INTO e2e_fk.child VALUES(1,'a',9223372036854775807);").await;
    let graph=s.schema_graph(Some("e2e_fk")).await.unwrap();
    assert_eq!(graph.entities.len(),2);assert_eq!(graph.relations.len(),1);
    let fk=&graph.relations[0];assert_eq!(fk.from_fields,["tenant","parent_id"]);assert_eq!(fk.to_fields,["tenant","id"]);
    let result=s.browse(&fk.to,PageRequest {offset:0,limit:10,filter:vec![("tenant".into(),Value::Text("a".into())),("id".into(),Value::Int(i64::MAX.to_string()))]}).await.unwrap();
    assert_eq!(result.row_count,1);assert_eq!(result.table.unwrap().rows[0][1],Value::Int(i64::MAX.to_string()));
    let result=s.browse(&fk.to,PageRequest {offset:0,limit:10,filter:vec![("tenant".into(),Value::Text("a' OR TRUE--\\".into()))]}).await.unwrap();assert_eq!(result.row_count,0);
    assert!(s.list_children(None).await.unwrap().iter().find(|n|n.label=="e2e_fk").unwrap().actions.iter().any(|a|matches!(a.kind,NodeActionKind::OpenSchemaGraph {..})));
    s.close().await;
}

#[tokio::test]
async fn change_preview_ignores_concurrent_ddl_from_other_sessions() {
    let Some(cfg) = config() else {
        return;
    };
    let driver = PostgresDriver::new();
    let s = driver.connect(&cfg).await.unwrap();
    let other = driver.connect(&cfg).await.unwrap();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let churn = tokio::spawn({
        let stop = stop.clone();
        async move {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                run(&*other, "CREATE TEMP TABLE axdb_race(id int primary key, v text); CREATE INDEX ON axdb_race(v); DROP TABLE axdb_race").await;
            }
            other.close().await;
        }
    });
    for _ in 0..40 {
        let p = s.preview(&req("CREATE TEMP TABLE axdb_mine(x int)"), CancelToken::new()).await.unwrap();
        let diff = p.statements[0].schema_diff.as_ref().unwrap();
        assert!(diff.added.iter().any(|n| n.contains("axdb_mine")));
        assert!(diff.added.iter().chain(&diff.removed).all(|n| !n.contains("axdb_race")), "{diff:?}");
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    churn.await.unwrap();
    s.close().await;
}

fn edit_request(node: &str, edits: Vec<RowEdit>) -> EditRequest {EditRequest{originals: vec![],create_table: None,node_id:node.into(),edits,timeout_ms:5000,preview_sample_limit:2}}
fn edit_key(id:i64)->Vec<(String,Value)>{vec![("id".into(),Value::Int(id.to_string()))]}
#[tokio::test]
async fn c2_bound_edits_preview_apply_conflicts_and_catalog(){

    let Some(cfg)=config() else { eprintln!("AXDB_TEST_PG unset; skipping C2 PostgreSQL integration");return; };
    let s=PostgresDriver::new().connect(&cfg).await.unwrap();
    let table=format!("c2_{}_{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let view=format!("{table}_v");let no_key=format!("{table}_n");
    let node=serde_json::to_string(&["table","public","tables",&table]).unwrap();
    let view_node=serde_json::to_string(&["table","public","views",&view]).unwrap();
    let no_key_node=serde_json::to_string(&["table","public","tables",&no_key]).unwrap();
    let bad_node=serde_json::to_string(&["table","public","tables","missing_c2; DROP TABLE x"]).unwrap();

    c2_run(s.as_ref(),&format!("CREATE TABLE {table}(id INTEGER PRIMARY KEY, t TEXT); INSERT INTO {table} VALUES(1,'old'),(2,'delete'); CREATE VIEW {view} AS SELECT * FROM {table}; CREATE TABLE {no_key}(t TEXT)")).await;
    c2_run(s.as_ref(),&format!("ALTER TABLE {table} DROP CONSTRAINT {table}_pkey; ALTER TABLE {table} ADD PRIMARY KEY(id) INCLUDE(t)")).await;
    let browse=s.browse(&node,PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap();
    assert_eq!(browse.edit_target.unwrap().key_columns,vec!["id"]);
    assert!(s.browse(&view_node,PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap().edit_target.is_none());
    assert!(s.browse(&no_key_node,PageRequest{offset:0,limit:10,filter:vec![]}).await.unwrap().edit_target.is_none());
    assert!(c2_run(s.as_ref(),&format!("SELECT * FROM {table}")).await.edit_target.is_some());
    let injection="'; DROP TABLE x; -- ' \\ 한글 🦀";
    let request=edit_request(&node,vec![
        RowEdit::Update{key:edit_key(1),set:vec![("t".into(),Value::Text(injection.into()))]},
        RowEdit::Insert{values:vec![("id".into(),Value::Int("3".into())),("t".into(),Value::Text("insert".into()))]},
        RowEdit::Delete{key:edit_key(2)},
    ]);
    let preview=s.preview_edits(&request,CancelToken::new()).await.unwrap();
    assert!(!preview.committed);assert_eq!(preview.statements.len(),3);
    assert_eq!(preview.statements[0].affected,Some(1));assert!(preview.statements[0].text.contains("$"));
    assert!(!preview.statements[0].text.contains(injection));
    assert_eq!(preview.statements[0].before.as_ref().unwrap().rows[0][1],Value::Text("old".into()));
    assert_eq!(preview.statements[0].after.as_ref().unwrap().rows[0][1],Value::Text(injection.into()));
    assert_eq!(c2_run(s.as_ref(),&format!("SELECT t FROM {table} WHERE id=1")).await.table.unwrap().rows[0][0],Value::Text("old".into()));
    let result=s.apply_edits(&request,CancelToken::new()).await.unwrap();assert_eq!(result.affected,vec![1,1,1]);
    assert_eq!(c2_run(s.as_ref(),&format!("SELECT id,t FROM {table} ORDER BY id")).await.table.unwrap().rows,vec![vec![Value::Int("1".into()),Value::Text(injection.into())],vec![Value::Int("3".into()),Value::Text("insert".into())]]);
    let conflict=edit_request(&node,vec![RowEdit::Update{key:edit_key(1),set:vec![("t".into(),Value::Text("rollback".into()))]},RowEdit::Delete{key:edit_key(999)}]);
    for preview in [false,true]{
        let error=if preview{s.preview_edits(&conflict,CancelToken::new()).await.unwrap_err()}else{s.apply_edits(&conflict,CancelToken::new()).await.unwrap_err()};
        assert_eq!(error.kind,ErrorKind::Conflict);assert!(error.message.contains("Edit 1"));
        assert_eq!(c2_run(s.as_ref(),&format!("SELECT t FROM {table} WHERE id=1")).await.table.unwrap().rows[0][0],Value::Text(injection.into()));
    }
    for invalid in ["missing", "t\"; DROP TABLE x; --"]{
        let bad=edit_request(&node,vec![RowEdit::Insert{values:vec![(invalid.into(),Value::Text("x".into()))]}]);
        assert_eq!(s.apply_edits(&bad,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Query);
    }
    let conversion=edit_request(&node,vec![RowEdit::Insert{values:edit_key(4)},RowEdit::Insert{values:vec![("id".into(),Value::Int("invalid integer".into()))]}]);
    let error=s.apply_edits(&conversion,CancelToken::new()).await.unwrap_err();assert_eq!(error.kind,ErrorKind::Query);assert!(error.message.contains("Edit 1"));
    assert_eq!(c2_run(s.as_ref(),&format!("SELECT count(*) FROM {table} WHERE id=4")).await.table.unwrap().rows[0][0],Value::Int("0".into()));
    let bad=edit_request(&bad_node,request.edits.clone());assert!(s.apply_edits(&bad,CancelToken::new()).await.is_err());
    let bad=edit_request(&node,vec![RowEdit::Delete{key:vec![]}]);assert!(s.apply_edits(&bad,CancelToken::new()).await.is_err());
    let too_many=edit_request(&node,vec![RowEdit::Delete{key:edit_key(1)};1001]);assert_eq!(s.apply_edits(&too_many,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Unsupported);
    let token=CancelToken::new();token.cancel();assert_eq!(s.apply_edits(&request,token).await.unwrap_err().kind,ErrorKind::Cancelled);s.ping().await.unwrap();
    c2_run(s.as_ref(),&format!("DROP VIEW {view}; DROP TABLE {table}; DROP TABLE {no_key}")).await;
}
#[tokio::test]
async fn c2_nested_multirow_conflict_and_interruption(){
    let Some(cfg)=config() else{return;};let s=PostgresDriver::new().connect(&cfg).await.unwrap();
    let table=format!("c2_nested_{}_{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());let child=format!("{table}_child");let function=format!("{table}_fn");
    c2_run(s.as_ref(),&format!("CREATE TABLE {table}(id INTEGER PRIMARY KEY,t TEXT); CREATE TABLE {child}() INHERITS({table}); INSERT INTO {table} VALUES(1,'parent'); INSERT INTO {child} VALUES(1,'child')")).await;
    let node=serde_json::to_string(&["table","public","tables",&table]).unwrap();
    let request=edit_request(&node,vec![RowEdit::Insert{values:edit_key(7)},RowEdit::Delete{key:edit_key(1)}]);
    let error=s.apply_edits(&request,CancelToken::new()).await.unwrap_err();assert_eq!(error.kind,ErrorKind::Conflict);assert!(error.message.contains("Edit 1"));
    assert_eq!(c2_run(s.as_ref(),&format!("SELECT count(*) FROM {table}")).await.table.unwrap().rows[0][0],Value::Int("2".into()));
    c2_run(s.as_ref(),&format!("DROP TABLE {child}; BEGIN; INSERT INTO {table} VALUES(9,'outer')")).await;
    let request=edit_request(&node,vec![RowEdit::Update{key:edit_key(1),set:vec![("t".into(),Value::Text("new".into()))]}]);
    s.preview_edits(&request,CancelToken::new()).await.unwrap();assert_eq!(c2_run(s.as_ref(),&format!("SELECT t FROM {table} WHERE id=1")).await.table.unwrap().rows[0][0],Value::Text("parent".into()));
    s.apply_edits(&request,CancelToken::new()).await.unwrap();
    let bad=edit_request(&node,vec![RowEdit::Delete{key:edit_key(999)}]);assert_eq!(s.apply_edits(&bad,CancelToken::new()).await.unwrap_err().kind,ErrorKind::Conflict);
    c2_run(s.as_ref(),"COMMIT").await;
    assert_eq!(c2_run(s.as_ref(),&format!("SELECT count(*) FROM {table}")).await.table.unwrap().rows[0][0],Value::Int("2".into()));
    c2_run(s.as_ref(),&format!("CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(10); RETURN NEW; END $$; CREATE TRIGGER c2_sleep BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION {function}()")).await;
    let mut slow=edit_request(&node,vec![RowEdit::Insert{values:edit_key(5)}]);
    for preview in [false,true]{
        slow.timeout_ms=30;let error=if preview{s.preview_edits(&slow,CancelToken::new()).await.unwrap_err()}else{s.apply_edits(&slow,CancelToken::new()).await.unwrap_err()};assert_eq!(error.kind,ErrorKind::Timeout);s.ping().await.unwrap();
        slow.timeout_ms=5000;let token=CancelToken::new();let cancelling=token.clone();tokio::spawn(async move{tokio::time::sleep(Duration::from_millis(30)).await;cancelling.cancel();});
        let error=if preview{s.preview_edits(&slow,token).await.unwrap_err()}else{s.apply_edits(&slow,token).await.unwrap_err()};assert_eq!(error.kind,ErrorKind::Cancelled);s.ping().await.unwrap();
        assert_eq!(c2_run(s.as_ref(),&format!("SELECT count(*) FROM {table} WHERE id=5")).await.table.unwrap().rows[0][0],Value::Int("0".into()));
    }
    c2_run(s.as_ref(),&format!("DROP TABLE {table}; DROP FUNCTION {function}()")).await;
}

#[tokio::test]
async fn c2_binary_edit_binds_and_reads_bytea() {
    let Some(cfg) = config() else { eprintln!("AXDB_TEST_PG unset; skipping C2 binary integration"); return; };
    let s = PostgresDriver::new().connect(&cfg).await.unwrap();
    let table = format!("c2_binary_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    c2_run(s.as_ref(), &format!("CREATE TABLE {table}(id INTEGER PRIMARY KEY, payload bytea); INSERT INTO {table} VALUES(1, decode('00','hex'))")).await;
    let node = serde_json::to_string(&["table", "public", "tables", &table]).unwrap();
    let request = edit_request(&node, vec![RowEdit::Update {
        key: edit_key(1),
        set: vec![("payload".into(), Value::Bytes { len: 4, hex: "00a1b2ff".into() })],
    }]);
    assert_eq!(s.apply_edits(&request, CancelToken::new()).await.unwrap().affected, vec![1]);
    let result = c2_run(s.as_ref(), &format!("SELECT payload FROM {table} WHERE id=1")).await;
    assert_eq!(result.table.unwrap().rows[0][0], Value::Bytes { len: 4, hex: "00a1b2ff".into() });
    c2_run(s.as_ref(), &format!("DROP TABLE {table}")).await;
}

async fn c2_run(s:&dyn Session,sql:&str)->ResultSet{run(s,sql).await.remove(0)}

#[tokio::test]
async fn query_result_edit_provenance_and_conflict() {
    let Some(cfg)=config() else {return;};
    let s=PostgresDriver::new().connect(&cfg).await.unwrap();
    run(&*s,"CREATE TEMP TABLE d10_query(id int PRIMARY KEY, name text); INSERT INTO d10_query VALUES(1,'old'),(2,'old'); CREATE TEMP VIEW d10_view AS SELECT * FROM d10_query").await;
    let result=run(&*s,"SELECT id AS alias_id,name,id+1 AS computed FROM d10_query WHERE id=1").await.remove(0);
    let target=result.edit_target.unwrap();
    assert_eq!(target.columns,vec![Some("id".into()),Some("name".into()),None]);
    for sql in ["SELECT name FROM d10_query", "SELECT DISTINCT id,name FROM d10_query", "SELECT id,count(*) FROM d10_query GROUP BY id", "SELECT a.id,a.name FROM d10_query a JOIN d10_query b ON a.id=b.id", "SELECT * FROM d10_view"] {assert!(run(&*s,sql).await[0].edit_target.is_none(),"{sql}");}
    let request:EditRequest=serde_json::from_value(serde_json::json!({"node_id":target.node_id,"edits":[{"op":"Update","key":[["id",{"t":"Int","v":"1"}]],"set":[["name",{"t":"Text","v":"edited"}]]}],"originals":[[["name",{"t":"Text","v":"old"}]]]})).unwrap();
    s.preview_edits(&request,CancelToken::new()).await.unwrap();
    assert_eq!(run(&*s,"SELECT name FROM d10_query WHERE id=1").await[0].table.as_ref().unwrap().rows[0][0],Value::Text("old".into()));
    s.apply_edits(&request,CancelToken::new()).await.unwrap();
    assert_eq!(run(&*s,"SELECT name FROM d10_query WHERE id=1").await[0].table.as_ref().unwrap().rows[0][0],Value::Text("edited".into()));
    assert!(matches!(s.apply_edits(&request,CancelToken::new()).await,Err(DbError{kind:ErrorKind::Conflict,..})));
    let mut conflicting=request.clone();
    if let RowEdit::Update{set,..}=&mut conflicting.edits[0]{set[0].1=Value::Text("should rollback".into());}
    conflicting.originals=vec![vec![("name".into(),Value::Text("edited".into()))],vec![("name".into(),Value::Text("stale".into()))]];
    conflicting.edits.push(RowEdit::Update{key:vec![("id".into(),Value::Int("2".into()))],set:vec![("name".into(),Value::Text("second".into()))]});
    assert!(matches!(s.apply_edits(&conflicting,CancelToken::new()).await,Err(DbError{kind:ErrorKind::Conflict,..})));
    assert_eq!(run(&*s,"SELECT name FROM d10_query WHERE id=1").await[0].table.as_ref().unwrap().rows[0][0],Value::Text("edited".into()));
    let mut bounded=req("SELECT * FROM d10_query");bounded.row_limit=1;
    assert!(s.execute(&bounded,CancelToken::new()).await.unwrap()[0].edit_target.is_none());
    bounded.row_limit=100;bounded.read_only=true;
    assert!(s.execute(&bounded,CancelToken::new()).await.unwrap()[0].edit_target.is_none());
    s.close().await;
}
