//! ETL throughput and memory benchmark (ADR-0005). Ignored by default:
//!   AXDB_ETL_BENCH_ROWS=100000 cargo test -p axdb --release --test etl_bench -- --ignored --nocapture --test-threads=1
//! With AXDB_DRIVER_DIR the drivers run as separate processes, as in the app. With AXDB_TEST_PG
//! the SQLite→PostgreSQL and PostgreSQL→PostgreSQL cases run too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
mod etl_support;
use axdb_etl::engine::{self, RunStatus};
use axdb_etl::spec::{CastType, Cmp, Options, Pipeline, Source, Step, Target, TextFunc};
use dbcore::*;
use etl_support::*;
use std::time::Instant;

/// Rows of a realistic order table: ids, short and long text, decimals as text, timestamps.
fn seed_sql(rows: u64) -> String {
    format!(
        "CREATE TABLE orders(id INTEGER PRIMARY KEY, customer TEXT, email TEXT, amount TEXT, status TEXT, created_at TEXT, note TEXT);
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {rows})
         INSERT INTO orders SELECT i, 'customer-' || (i % 5000), 'user' || i || '@example.com',
           printf('%d.%02d', i % 10000, i % 100),
           CASE i % 4 WHEN 0 THEN 'paid' WHEN 1 THEN 'shipped' WHEN 2 THEN 'cancelled' ELSE 'pending' END,
           datetime('2026-01-01', '+' || (i % 31536000) || ' seconds'),
           lower(hex(randomblob(40)))
         FROM n"
    )
}
const PAYLOAD_SQL: &str = "SELECT sum(8 + length(customer) + length(email) + length(amount) + length(status) + length(created_at) + length(note)) FROM orders";

/// Linux peak RSS since the last reset, in MiB.
fn reset_peak() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}
fn peak_mib() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok()))
        .map_or(f64::NAN, |kb| kb / 1024.0)
}
fn report(case: &str, rows: u64, bytes: f64, seconds: f64, extra: &str) {
    let line = serde_json::json!({
        "case": case, "rows": rows, "seconds": (seconds * 1000.0).round() / 1000.0,
        "rows_per_sec": (rows as f64 / seconds).round(),
        "mib_per_sec": ((bytes / 1048576.0) / seconds * 10.0).round() / 10.0,
        "peak_rss_mib": (peak_mib() * 10.0).round() / 10.0,
        "process_drivers": std::env::var_os("AXDB_DRIVER_DIR").is_some(),
        "note": extra,
    });
    eprintln!("ETL_BENCH {line}");
}
fn pipeline(source: Source, steps: Vec<Step>, target: &str, create: bool, batch: u32) -> Pipeline {
    Pipeline {
        id: "bench".into(),
        name: "bench".into(),
        description: String::new(),
        version: axdb_etl::spec::SPEC_VERSION,
        source,
        steps,
        target: Target { profile_id: "t".into(), node_id: target.into(), create_table: create },
        options: Options { batch_size: batch, max_rows: None, on_error: axdb_etl::OnError::Abort, batch_timeout_ms: 600_000 },
    }
}
fn transforms() -> Vec<Step> {
    vec![
        Step::Cast { column: "amount".into(), to: CastType::Decimal },
        Step::Filter { column: "status".into(), cmp: Cmp::Ne, value: Some(Value::Text("cancelled".into())) },
        Step::Text { column: "customer".into(), func: TextFunc::Upper },
        Step::Constant { column: "migrated".into(), value: Value::Bool(true) },
    ]
}
async fn copy(p: &Pipeline, source: &dyn Session, target: &dyn Session) -> (u64, f64) {
    let Source::Table { node_id, .. } = &p.source else { unreachable!() };
    let mut extractor = Browse { session: source, node: node_id.clone(), offset: 0, done: false };
    let mut loader = Import { session: target, node: p.target.node_id.clone() };
    reset_peak();
    let started = Instant::now();
    let report = engine::run(p, &mut extractor, &mut loader, &(), &CancelToken::new()).await;
    let seconds = started.elapsed().as_secs_f64();
    assert_eq!(report.status, RunStatus::Completed, "{report:?}");
    (report.progress.rows_written, seconds)
}
fn rows() -> u64 {
    std::env::var("AXDB_ETL_BENCH_ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000)
}

#[tokio::test]
#[ignore = "benchmark"]
async fn bench_sqlite_to_sqlite() {
    let n = rows();
    let dir = tempfile::tempdir().unwrap();
    let source = sqlite(&dir.path().join("src.db")).await;
    let started = Instant::now();
    exec(&*source, &seed_sql(n)).await;
    eprintln!("ETL_BENCH_SEED rows={n} seconds={:.2}", started.elapsed().as_secs_f64());
    let bytes: f64 = scalar(&*source, PAYLOAD_SQL).await.parse().unwrap();
    let table = "table:orders";
    // Native upper bound: one INSERT … SELECT inside SQLite.
    let base = sqlite(&dir.path().join("base.db")).await;
    exec(&*base, &format!("ATTACH '{}' AS src", dir.path().join("src.db").display())).await;
    reset_peak();
    let started = Instant::now();
    exec(&*base, "CREATE TABLE orders AS SELECT * FROM src.orders").await;
    report("sqlite→sqlite native CREATE TABLE AS SELECT", n, bytes, started.elapsed().as_secs_f64(), "upper bound");
    for batch in [1_000, 5_000, 10_000] {
        let target = sqlite(&dir.path().join(format!("dst{batch}.db"))).await;
        exec(&*target, "CREATE TABLE orders(id INTEGER PRIMARY KEY, customer TEXT, email TEXT, amount TEXT, status TEXT, created_at TEXT, note TEXT)").await;
        let p = pipeline(Source::Table { profile_id: "s".into(), node_id: table.into() }, vec![], table, false, batch);
        let (written, seconds) = copy(&p, &*source, &*target).await;
        assert_eq!(written, n);
        assert_eq!(scalar(&*target, "SELECT count(*) FROM orders").await, n.to_string());
        report(&format!("sqlite→sqlite table copy batch={batch}"), n, bytes, seconds, "");
    }
    let target = sqlite(&dir.path().join("dst-t.db")).await;
    let p = pipeline(Source::Table { profile_id: "s".into(), node_id: table.into() }, transforms(), "table:orders_clean", true, 5_000);
    let (written, seconds) = copy(&p, &*source, &*target).await;
    assert_eq!(written, n - n / 4 - u64::from(n % 4 >= 2));
    report("sqlite→sqlite 4 transforms + create_table batch=5000", n, bytes, seconds, "rows = read rows");
}

#[tokio::test]
#[ignore = "benchmark"]
async fn bench_sqlite_to_postgres_and_postgres_to_postgres() {
    let Some(admin_cfg) = pg_config("axdb_test") else {
        eprintln!("AXDB_TEST_PG not set; skipped");
        return;
    };
    let n = rows();
    let dir = tempfile::tempdir().unwrap();
    let source = sqlite(&dir.path().join("src.db")).await;
    exec(&*source, &seed_sql(n)).await;
    let bytes: f64 = scalar(&*source, PAYLOAD_SQL).await.parse().unwrap();
    let pg = driver("postgres").await;
    let admin = pg.connect(&admin_cfg).await.unwrap();
    for sql in ["DROP TABLE IF EXISTS public.orders", "DROP TABLE IF EXISTS public.orders_copy", "DROP TABLE IF EXISTS public.orders_native",
        "CREATE TABLE public.orders(id bigint PRIMARY KEY, customer text, email text, amount numeric(12,2), status text, created_at timestamp, note text)"] {
        exec(&*admin, sql).await;
    }
    let p = pipeline(
        Source::Table { profile_id: "s".into(), node_id: "table:orders".into() },
        vec![Step::Cast { column: "amount".into(), to: CastType::Decimal }, Step::Cast { column: "created_at".into(), to: CastType::DateTime }],
        r#"["table","public","tables","orders"]"#,
        false,
        5_000,
    );
    let (written, seconds) = copy(&p, &*source, &*admin).await;
    assert_eq!(written, n);
    report("sqlite→postgres (COPY) batch=5000", n, bytes, seconds, "casts decimal/datetime");
    exec(&*admin, "CREATE TABLE public.orders_copy (LIKE public.orders INCLUDING ALL)").await;
    let p = pipeline(Source::Table { profile_id: "s".into(), node_id: r#"["table","public","tables","orders"]"#.into() }, vec![], r#"["table","public","tables","orders_copy"]"#, false, 10_000);
    let (written, seconds) = copy(&p, &*admin, &*admin).await;
    assert_eq!(written, n);
    report("postgres→postgres table copy batch=10000", n, bytes, seconds, "offset paging");
    reset_peak();
    let started = Instant::now();
    exec(&*admin, "CREATE TABLE public.orders_native AS SELECT * FROM public.orders").await;
    report("postgres native CREATE TABLE AS SELECT", n, bytes, started.elapsed().as_secs_f64(), "upper bound");
    for sql in ["DROP TABLE public.orders_native", "DROP TABLE public.orders_copy", "DROP TABLE public.orders"] {
        exec(&*admin, sql).await;
    }
}
