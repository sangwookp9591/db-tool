//! Migration failure modes seen in practice, run through the ETL engine and real drivers.
//! SQLite cases always run; PostgreSQL cases need AXDB_TEST_PG. AXDB_DRIVER_DIR switches to the
//! plugin processes used by the app.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
mod etl_support;
use axdb_etl::engine::{self, RunReport, RunStatus};
use axdb_etl::spec::{CastType, ColumnMap, OnError, Options, Pipeline, Source, Step, Target};
use dbcore::*;
use etl_support::*;

fn pipeline(source: &str, target: &str, steps: Vec<Step>, create: bool, batch: u32, on_error: OnError) -> Pipeline {
    Pipeline {
        id: "m".into(),
        name: "migration".into(),
        description: String::new(),
        version: axdb_etl::spec::SPEC_VERSION,
        source: Source::Table { profile_id: "s".into(), node_id: source.into() },
        steps,
        target: Target { profile_id: "t".into(), node_id: target.into(), create_table: create },
        options: Options { batch_size: batch, max_rows: None, on_error, batch_timeout_ms: 60_000 },
    }
}
async fn go(p: &Pipeline, source: &dyn Session, target: &dyn Session) -> RunReport {
    let Source::Table { node_id, .. } = &p.source else { unreachable!() };
    let mut extractor = Browse { session: source, node: node_id.clone(), offset: 0, done: false };
    let mut loader = Import { session: target, node: p.target.node_id.clone() };
    engine::run(p, &mut extractor, &mut loader, &(), &CancelToken::new()).await
}
async fn pair(dir: &std::path::Path, source_sql: &str, target_sql: &str) -> (Box<dyn Session>, Box<dyn Session>) {
    let source = sqlite(&dir.join("src.db")).await;
    exec(&*source, source_sql).await;
    let target = sqlite(&dir.join("dst.db")).await;
    exec(&*target, target_sql).await;
    (source, target)
}
const SEED_2500: &str = "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
    WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2500)
    INSERT INTO t SELECT i, CASE WHEN i = 1500 THEN NULL ELSE 'n' || i END FROM n";

#[tokio::test]
async fn not_null_violation_fails_one_batch_and_keeps_earlier_ones() {
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = pair(dir.path(), SEED_2500, "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT NOT NULL)").await;
    let report = go(&pipeline("table:t", "table:t", vec![], false, 1000, OnError::Abort), &*source, &*target).await;
    assert_eq!(report.status, RunStatus::Failed, "{report:?}");
    assert_eq!((report.progress.batches_committed, report.progress.rows_written), (1, 1000));
    // The failing batch (rows 1001..2000) is atomic: none of it is visible.
    assert_eq!(scalar(&*target, "SELECT count(*) FROM t").await, "1000");
    assert_eq!(scalar(&*target, "SELECT max(id) FROM t").await, "1000");
}

#[tokio::test]
async fn rerunning_into_a_filled_table_fails_without_partial_rows() {
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = pair(dir.path(), SEED_2500, "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT)").await;
    let p = pipeline("table:t", "table:t", vec![], false, 1000, OnError::Abort);
    assert_eq!(go(&p, &*source, &*target).await.status, RunStatus::Completed);
    let again = go(&p, &*source, &*target).await;
    assert_eq!((again.status, again.progress.batches_committed), (RunStatus::Failed, 0), "{again:?}");
    assert_eq!(scalar(&*target, "SELECT count(*) FROM t").await, "2500");
}

#[tokio::test]
async fn text_survives_unicode_long_values_and_empty_vs_null() {
    let dir = tempfile::tempdir().unwrap();
    let long = "가".repeat(400_000); // 1.2 MB of UTF-8
    let source_sql = format!(
        "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
         INSERT INTO t VALUES (1, '👩‍👩‍👧 emoji ZWJ'), (2, 'e\u{301} combining'), (3, 'עברית RTL'), (4, ''), (5, NULL),
           (6, 'quote '' and \\ backslash'), (7, '{long}'), (8, 'tab\tnew\nline')"
    );
    let (source, target) = pair(dir.path(), &source_sql, "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)").await;
    let report = go(&pipeline("table:t", "table:t", vec![], false, 3, OnError::Abort), &*source, &*target).await;
    assert_eq!(report.status, RunStatus::Completed, "{report:?}");
    exec(&*target, &format!("ATTACH '{}' AS src", dir.path().join("src.db").display())).await;
    let same = "SELECT count(*) FROM t JOIN src.t s USING (id) WHERE t.v IS s.v";
    assert_eq!(scalar(&*target, same).await, "8");
    assert_eq!(scalar(&*target, "SELECT count(*) FROM t WHERE v = ''").await, "1");
    assert_eq!(scalar(&*target, "SELECT count(*) FROM t WHERE v IS NULL").await, "1");
}

#[tokio::test]
async fn hostile_and_reserved_column_names_are_quoted() {
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = pair(
        dir.path(),
        "CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b TEXT, c TEXT); INSERT INTO t VALUES (1, 'x', 'y', 'z')",
        "CREATE TABLE victim(id INTEGER); INSERT INTO victim VALUES (1)",
    )
    .await;
    let hostile = r#"x"); DROP TABLE victim; --"#;
    let steps = vec![Step::Select {
        columns: vec![
            ColumnMap { from: "id".into(), to: "order".into() },
            ColumnMap { from: "a".into(), to: "select".into() },
            ColumnMap { from: "b".into(), to: "한글 컬럼".into() },
            ColumnMap { from: "c".into(), to: hostile.into() },
        ],
    }];
    let report = go(&pipeline("table:t", "table:weird", steps, true, 10, OnError::Abort), &*source, &*target).await;
    assert_eq!(report.status, RunStatus::Completed, "{report:?}");
    assert_eq!(scalar(&*target, "SELECT count(*) FROM victim").await, "1");
    let quoted = hostile.replace('"', "\"\"");
    assert_eq!(scalar(&*target, &format!("SELECT \"{quoted}\" FROM weird")).await, "z");
    assert_eq!(scalar(&*target, "SELECT \"select\" || \"한글 컬럼\" FROM weird WHERE \"order\" = 1").await, "xy");
}

#[tokio::test]
async fn dirty_numbers_are_rejected_or_cleaned_by_rule() {
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = pair(
        dir.path(),
        "CREATE TABLE t(id INTEGER PRIMARY KEY, qty TEXT);
         INSERT INTO t VALUES (1, ' 42 '), (2, '1,234'), (3, '7.0'), (4, 'N/A'), (5, NULL), (6, '+8'), (7, '9e2')",
        "CREATE TABLE t(id INTEGER PRIMARY KEY, qty INTEGER)",
    )
    .await;
    let steps = vec![Step::Cast { column: "qty".into(), to: CastType::Int }];
    let strict = go(&pipeline("table:t", "table:t", steps.clone(), false, 100, OnError::Abort), &*source, &*target).await;
    assert_eq!((strict.status, strict.progress.rows_written), (RunStatus::Failed, 0));
    assert!(strict.error.unwrap().contains("소스 행 2"));
    let lenient = go(&pipeline("table:t", "table:t", steps, false, 100, OnError::SkipRow), &*source, &*target).await;
    assert_eq!(lenient.status, RunStatus::Completed, "{lenient:?}");
    // '1,234', 'N/A' and '9e2' are not integers; NULL stays NULL.
    assert_eq!((lenient.progress.rows_written, lenient.progress.rows_rejected), (4, 3));
    assert_eq!(lenient.rejected.iter().map(|r| r.row).collect::<Vec<_>>(), [2, 4, 7]);
    assert_eq!(scalar(&*target, "SELECT group_concat(qty) FROM (SELECT qty FROM t WHERE qty IS NOT NULL ORDER BY id)").await, "42,7,8");
}

#[tokio::test]
async fn source_schema_drift_between_runs_is_caught_at_compile_time() {
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = pair(dir.path(), "CREATE TABLE t(id INTEGER PRIMARY KEY, amount TEXT); INSERT INTO t VALUES (1, '5')", "CREATE TABLE t(id INTEGER, amount TEXT)").await;
    let p = pipeline("table:t", "table:t", vec![Step::Cast { column: "amount".into(), to: CastType::Decimal }], false, 10, OnError::Abort);
    exec(&*source, "ALTER TABLE t RENAME COLUMN amount TO total").await;
    let report = go(&p, &*source, &*target).await;
    assert_eq!((report.status, report.progress.rows_written), (RunStatus::Failed, 0));
    assert!(report.error.unwrap().contains("amount"));
}

/// Binary values longer than the 64-byte preview must never be written truncated.
#[tokio::test]
async fn long_binary_values_fail_instead_of_being_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = pair(
        dir.path(),
        "CREATE TABLE t(id INTEGER PRIMARY KEY, b BLOB); INSERT INTO t VALUES (1, x'00ff'), (2, randomblob(4096))",
        "CREATE TABLE t(id INTEGER PRIMARY KEY, b BLOB)",
    )
    .await;
    let report = go(&pipeline("table:t", "table:t", vec![], false, 1, OnError::Abort), &*source, &*target).await;
    assert_eq!(report.status, RunStatus::Failed, "{report:?}");
    assert_eq!(report.progress.rows_written, 1, "the short blob is complete and may be copied");
    assert_eq!(scalar(&*target, "SELECT count(*) FROM t WHERE id = 2").await, "0");
}

#[tokio::test]
async fn postgres_precision_time_zones_and_json_round_trip() {
    let Some(cfg) = pg_config("axdb_test") else {
        eprintln!("AXDB_TEST_PG not set; skipped");
        return;
    };
    let pg = driver("postgres").await.connect(&cfg).await.unwrap();
    let suffix = std::process::id();
    let (src, dst) = (format!("etl_m_src_{suffix}"), format!("etl_m_dst_{suffix}"));
    exec(&*pg, &format!(
        "CREATE TABLE public.{src}(id bigint PRIMARY KEY, big bigint, amount numeric(38,10), at timestamptz, doc jsonb, d date, flag boolean);
         INSERT INTO public.{src} VALUES
           (1, 9007199254740993, 12345678901234567890.1234567891, '2026-03-29 01:30:00+09', '{{\"n\": 12345678901234567890, \"s\": \"한글\"}}', '2024-02-29', true),
           (2, -9223372036854775808, -0.0000000001, '1999-12-31 23:59:59.999999+00', '[]', '0001-01-01', NULL);
         CREATE TABLE public.{dst} (LIKE public.{src} INCLUDING ALL)"
    )).await;
    let node = |t: &str| serde_json::json!(["table", "public", "tables", t]).to_string();
    let report = go(&pipeline(&node(&src), &node(&dst), vec![], false, 1000, OnError::Abort), &*pg, &*pg).await;
    let diff = scalar(&*pg, &format!("SELECT count(*) FROM (SELECT * FROM public.{src} EXCEPT SELECT * FROM public.{dst}) d")).await;
    let copied = scalar(&*pg, &format!("SELECT count(*) FROM public.{dst}")).await;
    exec(&*pg, &format!("DROP TABLE public.{src}; DROP TABLE public.{dst}")).await;
    assert_eq!(report.status, RunStatus::Completed, "{report:?}");
    assert_eq!((copied.as_str(), diff.as_str()), ("2", "0"), "every value must match exactly");
}

#[tokio::test]
async fn sqlite_text_types_load_into_typed_postgres_columns() {
    let Some(cfg) = pg_config("axdb_test") else {
        eprintln!("AXDB_TEST_PG not set; skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = sqlite(&dir.path().join("src.db")).await;
    exec(&*source, "CREATE TABLE o(id INTEGER PRIMARY KEY, amount TEXT, at TEXT, paid TEXT);
        INSERT INTO o VALUES (1, '12.50', '2026-11-04 10:00:00', 'yes'), (2, '0.10', '2026-11-04T23:59:59', 'no')").await;
    let pg = driver("postgres").await.connect(&cfg).await.unwrap();
    let table = format!("etl_typed_{}", std::process::id());
    exec(&*pg, &format!("CREATE TABLE public.{table}(id bigint PRIMARY KEY, amount numeric(10,2), at timestamp, paid boolean)")).await;
    let steps = vec![
        Step::Cast { column: "amount".into(), to: CastType::Decimal },
        Step::Cast { column: "at".into(), to: CastType::DateTime },
        Step::Cast { column: "paid".into(), to: CastType::Bool },
    ];
    let node = serde_json::json!(["table", "public", "tables", table]).to_string();
    let report = go(&pipeline("table:o", &node, steps, false, 100, OnError::Abort), &*source, &*pg).await;
    let check = scalar(&*pg, &format!("SELECT string_agg(amount::text || '|' || at::text || '|' || paid::text, ';' ORDER BY id) FROM public.{table}")).await;
    exec(&*pg, &format!("DROP TABLE public.{table}")).await;
    assert_eq!(report.status, RunStatus::Completed, "{report:?}");
    assert_eq!(check, "12.50|2026-11-04 10:00:00|true;0.10|2026-11-04 23:59:59|false");
}
