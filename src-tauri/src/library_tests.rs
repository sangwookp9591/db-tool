use super::*;
use std::{io::Write, time::Instant};

fn entry(index: usize) -> HistoryEntry {
    HistoryEntry {
        timestamp: now(),
        kind: "execute".into(),
        source: HistorySource::Ui,
        profile_id: Some(format!("profile-{}", index % 4)),
        profile_name: "Local database".into(),
        text: format!(
            "SELECT customer_id, total_amount FROM user_treatment_log WHERE customer_id = {index}; -- 국가별시술조회 고객정보"
        ),
        elapsed_ms: 12,
        row_count: 1,
        error: None,
    }
}
fn search(store: &Store, query: &str, limit: usize) -> HistorySearchResult {
    store
        .history_search(HistoryFilter {
            query: Some(query.into()),
            limit,
            ..Default::default()
        })
        .unwrap()
}
#[test]
fn history_pages_filters_search_and_exact_get_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    for i in 0..205 {
        let mut e = entry(i);
        if i % 2 == 0 {
            e.error = Some("example error".into());
            e.kind = "preview".into();
            e.source = HistorySource::Agent;
        }
        store.insert(&e).unwrap();
    }
    assert_eq!(store.history_list(1000).unwrap().len(), PAGE_MAX);
    assert_eq!(store.history_list(0).unwrap().len(), 0);
    let first = store
        .history_search(HistoryFilter {
            limit: 100,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first.items.len(), 100);
    let cursor = first.next_cursor.unwrap();
    store.insert(&entry(999)).unwrap(); // Inserts between pages do not shift the next page.
    let second = store
        .history_search(HistoryFilter {
            limit: 200,
            before_id: Some(cursor),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(second.items.len(), 105);
    assert!(second.items.iter().all(|h| h.id < cursor));
    assert!(second.next_cursor.is_none());
    let filtered = store
        .history_search(HistoryFilter {
            query: Some("treat".into()),
            profile_id: Some("profile-0".into()),
            status: HistoryStatus::Error,
            kind: Some("preview".into()),
            limit: 50,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(filtered.items.len(), 50);
    assert!(filtered.items.iter().all(|h| h.error.is_some()
        && h.kind == "preview"
        && h.profile_id.as_deref() == Some("profile-0")));
    assert!(
        search(&store, "별시술", 10).items.iter().all(|h| h
            .highlight
            .as_ref()
            .unwrap()
            .contains("<mark>"))
    );
    assert_eq!(search(&store, "시술", 10).items.len(), 10);
    assert_eq!(search(&store, "treatment", 10).items.len(), 10);
    assert_eq!(search(&store, "a\" OR *", 10).items.len(), 0);
    assert_eq!(search(&store, "'", 10).items.len(), 0);
    let mut huge = entry(206);
    huge.text = format!(
        "{} unique_marker_needle {}",
        "한".repeat(20_000),
        "x".repeat(20_000)
    );
    let id = store.insert(&huge).unwrap();
    let list = store.history_list(1).unwrap();
    assert!(list[0].text.len() <= 4096);
    assert_eq!(store.history_get(id).unwrap().text, huge.text);
    let found = search(&store, "marker_needle", 1);
    assert!(found.items[0].text.len() <= 4096);
    assert!(found.items[0].highlight.as_ref().unwrap().len() <= 4096);
    assert!(
        found.items[0]
            .highlight
            .as_ref()
            .unwrap()
            .contains("<mark>")
    );
    let json = serde_json::to_value(&list[0]).unwrap();
    assert!(json.get("timestamp").is_some() && json.get("text").is_some());
    assert_eq!(json["id"], id);
    assert!(json.get("entry").is_none());
    // The legacy triple also works on the bounded text returned to old clients.
    store
        .history_delete(
            None,
            Some(huge.timestamp),
            Some(huge.profile_name),
            Some(list[0].text.clone()),
        )
        .unwrap();
    assert!(store.history_get(id).is_err());
    let id = first.items[0].id;
    store.history_delete(Some(id), None, None, None).unwrap();
    assert!(store.history_get(id).is_err());
    store.history_clear(Some("profile-0".into())).unwrap();
    assert!(
        store
            .history_search(HistoryFilter {
                profile_id: Some("profile-0".into()),
                limit: 200,
                ..Default::default()
            })
            .unwrap()
            .items
            .is_empty()
    );
    store.history_clear(None).unwrap();
    assert!(search(&store, "treatment", 10).items.is_empty());
    assert!(store.insert(&entry(0)).unwrap() > id); // IDs cannot be reused across clear/delete.
}
#[test]
fn short_search_returns_scan_cursor_even_when_window_has_no_matches() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let wanted = store.insert(&entry(0)).unwrap();
    let mut other = entry(0);
    other.text = "SELECT 1".into();
    for _ in 0..SHORT_SCAN_ROWS {
        store.insert(&other).unwrap();
    }
    let first = search(&store, "시술", 10);
    assert!(first.items.is_empty());
    let next = store
        .history_search(HistoryFilter {
            query: Some("시술".into()),
            before_id: first.next_cursor,
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(next.items[0].id, wanted);
}
#[test]
fn snippets_roundtrip_tags_profile_search_and_fts_update_delete() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let first = store
        .snippet_save(SnippetSave {
            id: None,
            title: "국가별시술조회".into(),
            body: "select * from user_treatment_log".into(),
            tags: vec!["customers".into(), "tag_with_underscore".into()],
            profile_id: None,
            language: "sql".into(),
            pinned: true,
        })
        .unwrap();
    assert!(first.pinned);
    assert!(first.profile_id.is_none());
    for query in ["treat", "별시술", "시술", "customers"] {
        let result = store
            .snippet_list(SnippetFilter {
                query: Some(query.into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.len(), 1, "{query}");
    }
    assert_eq!(
        store
            .snippet_list(SnippetFilter {
                tag: Some("tag_with_underscore".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .snippet_list(SnippetFilter {
                tag: Some("customer".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .snippet_list(SnippetFilter {
                profile_id: Some("other".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap()
            .is_empty()
    );
    let body = "x".repeat(30_000);
    let updated = store
        .snippet_save(SnippetSave {
            id: Some(first.id),
            title: "Updated".into(),
            body: body.clone(),
            tags: vec!["new".into()],
            profile_id: Some("other".into()),
            language: "postgresql".into(),
            pinned: false,
        })
        .unwrap();
    assert_eq!(updated.created_at, first.created_at);
    assert_eq!(updated.body, body);
    assert_eq!(store.snippet_get(first.id).unwrap().body, body);
    store
        .snippet_save(SnippetSave {
            id: None,
            title: "Other".into(),
            body: "SELECT 2".into(),
            tags: Vec::new(),
            profile_id: None,
            language: "sql".into(),
            pinned: false,
        })
        .unwrap();
    let page = store
        .snippet_list(SnippetFilter {
            limit: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    let older = store
        .snippet_list(SnippetFilter {
            before_id: page.next_cursor.map(Into::into),
            limit: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(older.items[0].id, first.id);
    assert!(older.next_cursor.is_none());
    assert!(
        store
            .snippet_list(SnippetFilter {
                limit: 10,
                ..Default::default()
            })
            .unwrap()[0]
            .body
            .len()
            <= 4096
    );
    assert!(
        store
            .snippet_list(SnippetFilter {
                query: Some("treat".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap()
            .is_empty()
    );
    store.snippet_delete(first.id).unwrap();
    assert!(store.snippet_get(first.id).is_err());
    assert!(
        store
            .snippet_list(SnippetFilter {
                query: Some("Updated".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap()
            .is_empty()
    );
}
#[test]
fn migration_streams_tolerates_corruption_preserves_backup_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    let row = serde_json::to_string(&entry(0)).unwrap();
    let bytes = [row.as_bytes(), b"\n{invalid\n\xff\n", row.as_bytes(), b"\n"].concat();
    std::fs::write(&path, &bytes).unwrap();
    {
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.history_list(10).unwrap().len(), 2);
        assert_eq!(search(&store, "treat", 10).items.len(), 2);
        assert!(!path.exists());
        assert_eq!(
            std::fs::read(dir.path().join("history.jsonl.migrated")).unwrap(),
            bytes
        );
    }
    // Crash after committing migration but before the rename: marker prevents duplicates.
    std::fs::rename(dir.path().join("history.jsonl.migrated"), &path).unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.history_list(10).unwrap().len(), 2);
    assert!(!path.exists());
    store.insert(&entry(1)).unwrap();
    assert_eq!(search(&store, "treat", 10).items.len(), 3); // Insert trigger survives bulk migration and a reopen.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("library.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(dir.path().join("library.sqlite3-wal"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn count_age_retention_is_batched_and_fts_remains_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    store.keep_rows = 10;
    store.keep_days = 0;
    for i in 0..255 {
        store.insert(&entry(i)).unwrap();
    }
    assert_eq!(store.rows, 255); // No count/read/prune on every insert.
    store.insert(&entry(255)).unwrap();
    assert_eq!(store.rows, 10);
    assert_eq!(search(&store, "treat", 200).items.len(), 10);
    store.keep_days = 1;
    store.keep_rows = 1000;
    let mut old = entry(0);
    old.timestamp = "2000-01-01T00:00:00Z".into();
    for _ in 0..256 {
        store.insert(&old).unwrap();
    }
    assert!(
        store
            .history_list(200)
            .unwrap()
            .iter()
            .all(|h| h.timestamp != old.timestamp)
    );
    store
        .connection
        .execute(
            "INSERT INTO history_fts(history_fts,rank) VALUES('integrity-check',1)",
            [],
        )
        .unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn connection_is_shared_by_concurrent_blocking_jobs_and_redaction_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::new(dir.path().into());
    let mut handles = Vec::new();
    for i in 0..10 {
        let library = library.clone();
        handles.push(tokio::spawn(async move {
            let mut row = entry(i);
            row.text = crate::storage::redact("SELECT 'secret-value'", Some("secret-value"));
            library.run(move |s| s.insert(&row)).await.unwrap()
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let entries = library.run(|s| s.history_list(200)).await.unwrap();
    assert_eq!(entries.len(), 10);
    assert!(entries.iter().all(|h| h.text == "SELECT '***'"));
    let wal = std::fs::read(dir.path().join("library.sqlite3-wal")).unwrap();
    assert!(
        !wal.windows(b"secret-value".len())
            .any(|w| w == b"secret-value")
    );
}

fn rss_bytes() -> u64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<u64>()
        .unwrap()
        * 1024
}
// Both supported Unix ABIs place ru_maxrss after the two timeval fields.
// An oversized, aligned buffer avoids mirroring platform-only trailing fields.
#[cfg(unix)]
fn lifetime_peak_rss_bytes() -> u64 {
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut std::ffi::c_void) -> i32;
    }
    let mut usage = [0i64; 32];
    assert_eq!(unsafe { getrusage(0, usage.as_mut_ptr().cast()) }, 0);
    let units = if cfg!(target_os = "macos") { 1 } else { 1024 };
    usage[4].max(0) as u64 * units
}
#[cfg(not(unix))]
fn lifetime_peak_rss_bytes() -> u64 {
    rss_bytes()
}
fn cpu_time() -> String {
    let output = std::process::Command::new("ps")
        .args(["-o", "time=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn p95_us(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[(values.len() - 1) * 95 / 100]
}
fn timed_query(store: &Store, query: &str) -> (f64, f64) {
    let mut timings = Vec::new();
    for _ in 0..100 {
        let started = Instant::now();
        assert!(!search(store, query, 100).items.is_empty());
        timings.push(started.elapsed().as_secs_f64() * 1e6);
    }
    let max = timings.iter().copied().fold(0., f64::max);
    (p95_us(timings), max)
}
/// Run in isolation: cargo test -p axdb --release library::tests::library_benchmark -- --ignored --exact --nocapture --test-threads=1
/// Measures the store itself; excludes the unchanged query-usage settings write.
#[test]
#[ignore = "100k-row disk/RSS performance benchmark; run explicitly in release"]
fn library_benchmark() -> Result<(), DbError> {
    assert!(!cfg!(debug_assertions), "run this benchmark with --release");
    let dir = tempfile::tempdir().unwrap();
    let baseline = rss_bytes();
    let mut peak = baseline;
    let started = Instant::now();
    let mut store = Store::open(dir.path()).unwrap();
    store.keep_rows = 110_000; // Preserve the requested 100k working set.
    let open_us = started.elapsed().as_secs_f64() * 1e6;
    let initial_peak = lifetime_peak_rss_bytes();
    let mut inserts = Vec::with_capacity(100_000);
    for i in 0..100_000 {
        let e = entry(i);
        let started = Instant::now();
        store.insert(&e).unwrap();
        inserts.push(started.elapsed().as_secs_f64() * 1e6);
        if i % 5000 == 0 {
            peak = peak.max(rss_bytes());
        }
    }
    peak = peak.max(rss_bytes());
    inserts.sort_unstable_by(f64::total_cmp);
    let insert_total_ms = inserts.iter().sum::<f64>() / 1000.;
    let insert_p50 = inserts[(inserts.len() - 1) * 50 / 100];
    let insert_p99 = inserts[(inserts.len() - 1) * 99 / 100];
    let insert_max = *inserts.last().unwrap();
    let insert_p95 = p95_us(inserts);
    let mut steady = Vec::new();
    for i in 100_000..102_000 {
        let e = entry(i);
        let started = Instant::now();
        store.insert(&e).unwrap();
        steady.push(started.elapsed().as_secs_f64() * 1e6);
    }
    steady.sort_unstable_by(f64::total_cmp);
    let steady_p50 = steady[(steady.len() - 1) * 50 / 100];
    let steady_p99 = steady[(steady.len() - 1) * 99 / 100];
    let steady_p95 = p95_us(steady);
    let mut pages = Vec::new();
    let mut cursor = None;
    for _ in 0..100 {
        let started = Instant::now();
        let result = store
            .history_search(HistoryFilter {
                before_id: cursor,
                limit: 100,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.items.len(), 100);
        pages.push(started.elapsed().as_secs_f64() * 1e6);
        cursor = result.next_cursor;
    }
    let page_max = pages.iter().copied().fold(0., f64::max);
    let page_p95 = p95_us(pages);
    let (fts_p95, fts_max) = timed_query(&store, "treatment");
    let (rare_p95, rare_max) = timed_query(&store, "99999;");
    let (korean_p95, korean_max) = timed_query(&store, "별시술");
    let (short_p95, short_max) = timed_query(&store, "시술");
    let (date_page_p95, date_fts_p95, date_short_p95, pinned_page_p95) =
        benchmark_new_filters(&mut store)?;
    peak = peak.max(rss_bytes());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let library = Library {
        directory: dir.path().into(),
        store: Arc::new(Mutex::new(Some(store))),
        gate: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    let mut dispatch = Vec::new();
    runtime.block_on(async {
        for i in 102_000..103_000 {
            let e = entry(i);
            let started = Instant::now();
            library.run(move |s| s.insert(&e)).await.unwrap();
            dispatch.push(started.elapsed().as_secs_f64() * 1e6);
        }
    });
    let dispatch_p95 = p95_us(dispatch);
    let store = library.store.lock().unwrap().take().unwrap();
    drop(runtime);
    // SQLite dbstat reports allocated index pages rather than the whole DB/WAL.
    let index_bytes: i64 = store
        .connection
        .query_row(
            "SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name LIKE 'history_fts%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let all_index_bytes: i64 = store.connection.query_row("SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name NOT IN ('history','snippets','library_meta','sqlite_sequence')", [], |r| r.get(0)).unwrap();
    let database_bytes: i64 = store
        .connection
        .query_row(
            "SELECT page_count*page_size FROM pragma_page_count(),pragma_page_size()",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let cache_size: i64 = store
        .connection
        .query_row("PRAGMA cache_size", [], |r| r.get(0))
        .unwrap();
    let checkpoint_pages: i64 = store
        .connection
        .query_row("PRAGMA wal_autocheckpoint", [], |r| r.get(0))
        .unwrap();
    let fts_automerge: i64 = store
        .connection
        .query_row(
            "SELECT v FROM history_fts_config WHERE k='automerge'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let fts_crisismerge: i64 = store
        .connection
        .query_row(
            "SELECT v FROM history_fts_config WHERE k='crisismerge'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let idle_cpu_before = cpu_time();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let idle_cpu_after = cpu_time();
    drop(store);
    let migration_dir = tempfile::tempdir().unwrap();
    {
        let mut file = std::io::BufWriter::new(
            std::fs::File::create(migration_dir.path().join("history.jsonl")).unwrap(),
        );
        for i in 0..50_000 {
            serde_json::to_writer(&mut file, &entry(i)).unwrap();
            file.write_all(b"\n").unwrap();
        }
        file.flush().unwrap();
    }
    // Measure first-open migration in a fresh process, not in the allocator and
    // SQLite caches warmed by the 100k insertion benchmark.
    let migration_report_path = migration_dir.path().join("migration-result.json");
    let started = Instant::now();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "library::tests::cold_migration_benchmark",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AXDB_LIBRARY_MIGRATION_DIRECTORY", migration_dir.path())
        .env("AXDB_LIBRARY_MIGRATION_REPORT", &migration_report_path)
        .output()
        .unwrap();
    let migration_process_wall_ms = started.elapsed().as_secs_f64() * 1e3;
    let migration: serde_json::Value =
        serde_json::from_slice(&std::fs::read(migration_report_path).unwrap()).unwrap();
    let migration_ms = migration["migration_ms"].as_f64().unwrap();
    let kernel_peak = lifetime_peak_rss_bytes();
    let parts = [
        serde_json::json!({
            "lto_override":std::env::var("CARGO_PROFILE_RELEASE_LTO").unwrap_or_else(|_| "workspace default".into()),"codegen_units_override":std::env::var("CARGO_PROFILE_RELEASE_CODEGEN_UNITS").unwrap_or_else(|_| "workspace default".into()),"platform": std::env::consts::OS, "arch": std::env::consts::ARCH, "build":"release",
            "history_rows":103000,"migration_rows":50000,"sql_bytes_min":entry(0).text.len(),"sql_bytes_max":entry(102999).text.len(),"retention_override_keep_rows":110000,"default_retention_keep_rows":KEEP_ROWS,"default_retention_keep_days":KEEP_DAYS,
            "sqlite_cache_size_pragma":cache_size,"wal_autocheckpoint_pages":checkpoint_pages,"fts_automerge":fts_automerge,"fts_crisismerge":fts_crisismerge,
            "configuration_reference":"https://www.sqlite.org/fts5.html#the_automerge_configuration_option",
        }),
        serde_json::json!({
            "open_us":open_us,"insert_p50_us":insert_p50,"insert_p95_us":insert_p95,"insert_p99_us":insert_p99,"insert_max_us":insert_max,"insert_100k_total_ms":insert_total_ms,
            "insert_at_100k_p50_us":steady_p50,"insert_at_100k_p95_us":steady_p95,"insert_at_100k_p99_us":steady_p99,"insert_with_spawn_blocking_p95_us":dispatch_p95,"keyset_page_100_p95_us":page_p95,"keyset_page_100_max_us":page_max,
            "fts_rare_p95_us":rare_p95,"fts_rare_max_us":rare_max,"fts_identifier_infix_p95_us":fts_p95,"fts_identifier_infix_max_us":fts_max,
            "fts_korean_infix_p95_us":korean_p95,"fts_korean_infix_max_us":korean_max,
            "korean_two_character_p95_us":short_p95,"korean_two_character_max_us":short_max,
            "date_range_page100_p95_us":date_page_p95,"date_range_fts_p95_us":date_fts_p95,"date_range_short_p95_us":date_short_p95,"pinned_first_page100_p95_us":pinned_page_p95,"snippet_rows":400,
        }),
        serde_json::json!({
            "fts_index_bytes":index_bytes,"all_index_bytes":all_index_bytes,"database_bytes":database_bytes,
            "rss_baseline_bytes":baseline,"initial_process_peak_rss_bytes":initial_peak,"process_peak_rss_bytes":kernel_peak,"process_peak_rss_growth_bytes":kernel_peak.saturating_sub(baseline),"rss_sampled_peak_bytes":peak,"rss_growth_bytes":peak.saturating_sub(baseline),
            "migration_ms":migration_ms,"migration_process_wall_ms":migration_process_wall_ms,"cold_migration":migration,
            "idle_cpu_time_before":idle_cpu_before,"idle_cpu_time_after":idle_cpu_after,
            "notes":"RSS sampled every 5000 inserts and phase boundaries, not a high-frequency peak. Kernel getrusage also records the absolute process lifetime RSS peak on Unix. Timing vectors and test harness included. Synthetic SQL is 117-122 bytes with four profiles and successful executions; count retention is raised to 110000 only for the 100k working-set benchmark. Short queries scan at most 5000 candidate rows per cursor. Migration runs in a fresh process with no previous database, but JSONL is in OS cache. Cold storage-device cache was not flushed. Separate insert_with_spawn_blocking includes dispatch overhead; all timings exclude unchanged star/settings writes. Idle CPU is measured for the store without an app window or live DB drivers, at ps CPU-time resolution (0.01 s)."
        }),
    ];
    let mut fields = serde_json::Map::new();
    for part in parts {
        if let serde_json::Value::Object(part) = part {
            fields.extend(part);
        }
    }
    let report = serde_json::Value::Object(fields);

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if let Some(path) = std::env::var_os("AXDB_LIBRARY_BENCH_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    // Report every number before asserting so regressions retain useful evidence.
    assert!(
        child.status.success(),
        "migration child failed: {} {}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(
        steady_p95 < 1000. && dispatch_p95 < 1000.,
        "insert p95 exceeds 1 ms"
    );
    assert!(
        page_p95 < 5000. && date_page_p95 < 5000. && pinned_page_p95 < 5000.,
        "page p95 exceeds 5 ms"
    );
    assert!(
        fts_p95 < 30000.
            && rare_p95 < 30000.
            && korean_p95 < 30000.
            && short_p95 < 30000.
            && date_fts_p95 < 30000.
            && date_short_p95 < 30000.,
        "search p95 exceeds 30 ms"
    );
    assert!(
        kernel_peak.saturating_sub(baseline) < 10_000_000,
        "process peak RSS growth exceeds 10 MB"
    );
    assert!(
        migration_ms < 2000. && migration_process_wall_ms < 2000.,
        "migration exceeds 2 s"
    );
    assert_eq!(
        idle_cpu_before, idle_cpu_after,
        "CPU time grew during idle (ps resolution applies)"
    );
    Ok(())
}

/// Separate-process migration entry point used by library_benchmark.
#[test]
#[ignore = "cold-process migration benchmark helper; run from library_benchmark"]
fn cold_migration_benchmark() {
    assert!(!cfg!(debug_assertions));
    let path =
        std::env::var_os("AXDB_LIBRARY_MIGRATION_DIRECTORY").expect("benchmark input directory");
    let baseline = rss_bytes();
    let initial_peak = lifetime_peak_rss_bytes();
    let started = Instant::now();
    let store = Store::open(Path::new(&path)).unwrap();
    let elapsed_ms = started.elapsed().as_secs_f64() * 1e3;
    let peak = lifetime_peak_rss_bytes();
    assert_eq!(store.rows, 50_000);
    let report = serde_json::json!({"migration_ms":elapsed_ms,"rows":store.rows,"rss_baseline_bytes":baseline,"initial_process_peak_rss_bytes":initial_peak,"process_peak_rss_bytes":peak,"process_peak_rss_growth_bytes":peak.saturating_sub(baseline),"fresh_process":true,"disk_cache":"warm"});
    println!("{report}");
    if let Some(path) = std::env::var_os("AXDB_LIBRARY_MIGRATION_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert!(
        peak.saturating_sub(baseline) < 10_000_000,
        "migration peak RSS growth exceeds 10 MB"
    );
    assert!(elapsed_ms < 2000., "migration exceeds 2 s");
}

fn fixture_snippet(store: &mut Store, id: Option<i64>, pinned: bool) -> Result<Snippet, DbError> {
    store.snippet_save(SnippetSave {
        id,
        title: "needle 시술".into(),
        body: "select needle 시술".into(),
        tags: vec![],
        profile_id: None,
        language: "sql".into(),
        pinned,
    })
}

#[test]
fn pinned_pages_freeze_full_order_across_pin_toggles_and_reopen() -> Result<(), DbError> {
    let dir = tempfile::tempdir().map_err(|_| DbError::internal("Test directory unavailable"))?;
    let mut store = Store::open(dir.path())?;
    let mut original = Vec::new();
    for i in 0..12 {
        original.push(fixture_snippet(&mut store, None, i % 3 == 0)?);
    }
    for query in [None, Some("needle"), Some("시술")] {
        let expected = store
            .snippet_list(SnippetFilter {
                query: query.map(str::to_owned),
                limit: 100,
                ..Default::default()
            })?
            .items;
        assert!(
            expected
                .windows(2)
                .all(|w| (w[0].pinned, &w[0].updated_at, w[0].id)
                    >= (w[1].pinned, &w[1].updated_at, w[1].id))
        );
        let first = store.snippet_list(SnippetFilter {
            query: query.map(str::to_owned),
            limit: 3,
            ..Default::default()
        })?;
        let mut seen: Vec<_> = first.items.iter().map(|s| s.id).collect();
        let mut cursor = first.next_cursor;
        // Move both an already delivered row and an unseen row across the boundary.
        fixture_snippet(&mut store, Some(expected[0].id), !expected[0].pinned)?;
        fixture_snippet(&mut store, Some(expected[10].id), !expected[10].pinned)?;
        let added = fixture_snippet(&mut store, None, true)?;
        drop(store);
        store = Store::open(dir.path())?;
        while let Some(next) = cursor {
            let page = store.snippet_list(SnippetFilter {
                query: query.map(str::to_owned),
                before_id: Some(next.into()),
                limit: 3,
                ..Default::default()
            })?;
            seen.extend(page.items.iter().map(|s| s.id));
            cursor = page.next_cursor;
        }
        assert_eq!(seen, expected.iter().map(|s| s.id).collect::<Vec<_>>());
        assert!(!seen.contains(&added.id));
    }
    let legacy: SnippetCursorInput =
        serde_json::from_str("12").map_err(|_| DbError::internal("Test cursor unavailable"))?;
    let result = store.snippet_list(SnippetFilter {
        before_id: Some(legacy),
        limit: 1,
        ..Default::default()
    });
    assert!(
        matches!(result, Err(error) if error.kind == dbcore::ErrorKind::Query && error.message == "Legacy snippet cursor is unsupported; restart pagination")
    );
    assert!(
        store
            .snippet_list(SnippetFilter {
                before_id: Some("bad".to_owned().into()),
                limit: 1,
                ..Default::default()
            })
            .is_err()
    );
    let index: String = store
        .connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='snippets_pinned_updated'",
            [],
            |r| r.get(0),
        )
        .map_err(database_error)?;
    assert_eq!(index, "snippets_pinned_updated");
    Ok(())
}

#[test]
fn history_date_boundaries_validation_and_combined_filters() -> Result<(), DbError> {
    let dir = tempfile::tempdir().map_err(|_| DbError::internal("Test directory unavailable"))?;
    let mut store = Store::open(dir.path())?;
    store.keep_days = 0;
    for i in 0..4 {
        let mut row = entry(i);
        row.timestamp = format!("2026-10-21T00:00:0{i}Z");
        row.profile_id = Some("selected".into());
        row.error = Some("failure".into());
        row.kind = "preview".into();
        store.insert(&row)?;
    }
    for query in [None, Some("treatment"), Some("시술")] {
        let result = store.history_search(HistoryFilter {
            since: Some("2026-10-21T00:00:01Z".into()),
            until: Some("2026-10-21T00:00:03Z".into()),
            query: query.map(str::to_owned),
            profile_id: Some("selected".into()),
            status: HistoryStatus::Error,
            kind: Some("preview".into()),
            limit: 100,
            ..Default::default()
        })?;
        assert_eq!(
            result
                .items
                .iter()
                .map(|r| r.timestamp.as_str())
                .collect::<Vec<_>>(),
            vec!["2026-10-21T00:00:02Z", "2026-10-21T00:00:01Z"]
        );
    }
    for (since, until, message) in [
        (
            "bad",
            "2026-10-21T00:00:03Z",
            "Invalid history timestamp; expected RFC3339",
        ),
        (
            "2026-10-21T00:00:01Z",
            "bad",
            "Invalid history timestamp; expected RFC3339",
        ),
        (
            "2026-10-21T00:00:03Z",
            "2026-10-21T00:00:01Z",
            "Invalid history date range: since must not exceed until",
        ),
    ] {
        let result = store.history_search(HistoryFilter {
            since: Some(since.into()),
            until: Some(until.into()),
            limit: 100,
            ..Default::default()
        });
        assert!(
            matches!(result, Err(error) if error.kind == dbcore::ErrorKind::Query && error.message == message)
        );
    }
    let equal = store.history_search(HistoryFilter {
        since: Some("2026-10-21T00:00:01Z".into()),
        until: Some("2026-10-21T00:00:01Z".into()),
        limit: 100,
        ..Default::default()
    })?;
    assert!(equal.items.is_empty());
    // Prove range access remains indexed for both forms, rather than a history scan.
    for (sql, index) in [
        (
            "EXPLAIN QUERY PLAN SELECT id FROM history INDEXED BY history_ts WHERE ts >= '2026-10-21' AND ts < '2026-10-22' ORDER BY id DESC LIMIT 5000",
            "history_ts",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM history INDEXED BY history_profile_ts WHERE profile_id='selected' AND ts >= '2026-10-21' AND ts < '2026-10-22' ORDER BY id DESC LIMIT 5000",
            "history_profile_ts",
        ),
    ] {
        let mut statement = store.connection.prepare(sql).map_err(database_error)?;
        let plans = statement
            .query_map([], |r| r.get::<_, String>(3))
            .map_err(database_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(database_error)?;
        assert!(
            plans.iter().any(|plan| plan.contains("SEARCH")
                && plan.contains(index)
                && plan.contains("ts>"))
        );
    }
    Ok(())
}

#[test]
fn ranged_search_bounds_candidates_before_matching() -> Result<(), DbError> {
    let dir = tempfile::tempdir().map_err(|_| DbError::internal("Test directory unavailable"))?;
    let mut store = Store::open(dir.path())?;
    store.keep_days = 0;
    let mut wanted = entry(0);
    wanted.timestamp = "2026-10-21T00:00:01Z".into();
    let id = store.insert(&wanted)?;
    let mut other = wanted.clone();
    other.text = "select 1".into();
    for _ in 0..SHORT_SCAN_ROWS {
        store.insert(&other)?;
    }
    // Newer rows outside the date range must not consume candidate slots.
    other.timestamp = "2026-10-22T00:00:01Z".into();
    for _ in 0..20 {
        store.insert(&other)?;
    }
    for query in ["시술", "treatment"] {
        let filter = || HistoryFilter {
            since: Some("2026-10-21T00:00:00Z".into()),
            until: Some("2026-10-22T00:00:00Z".into()),
            query: Some(query.into()),
            limit: 10,
            ..Default::default()
        };
        let first = store.history_search(filter())?;
        assert!(first.items.is_empty());
        assert!(first.next_cursor.is_some());
        let next = store.history_search(HistoryFilter {
            before_id: first.next_cursor,
            ..filter()
        })?;
        assert_eq!(next.items.len(), 1);
        assert_eq!(next.items[0].id, id);
        assert!(next.next_cursor.is_none());
    }
    Ok(())
}

fn benchmark_new_filters(store: &mut Store) -> Result<(f64, f64, f64, f64), DbError> {
    let since = (time::OffsetDateTime::now_utc() - time::Duration::days(1))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| DbError::internal("Benchmark timestamp unavailable"))?;
    let until = (time::OffsetDateTime::now_utc() + time::Duration::days(1))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| DbError::internal("Benchmark timestamp unavailable"))?;
    let mut timings = Vec::new();
    for query in [None, Some("treatment"), Some("시술")] {
        let mut runs = Vec::new();
        for _ in 0..100 {
            let started = Instant::now();
            let page = store.history_search(HistoryFilter {
                query: query.map(str::to_owned),
                since: Some(since.clone()),
                until: Some(until.clone()),
                profile_id: Some("profile-0".into()),
                limit: 100,
                ..Default::default()
            })?;
            assert_eq!(page.items.len(), 100);
            runs.push(started.elapsed().as_secs_f64() * 1e6);
        }
        timings.push(p95_us(runs));
    }
    for i in 0..400 {
        fixture_snippet(store, None, i % 3 == 0)?;
    }
    let mut runs = Vec::new();
    for _ in 0..100 {
        let started = Instant::now();
        let page = store.snippet_list(SnippetFilter {
            limit: 100,
            ..Default::default()
        })?;
        assert_eq!(page.items.len(), 100);
        assert!(page.items.iter().all(|s| s.pinned));
        runs.push(started.elapsed().as_secs_f64() * 1e6);
    }
    Ok((timings[0], timings[1], timings[2], p95_us(runs)))
}

#[test]
fn date_bounds_preserve_nanoseconds_and_accept_timezone_offsets() -> Result<(), DbError> {
    let dir = tempfile::tempdir().map_err(|_| DbError::internal("Test directory unavailable"))?;
    let mut store = Store::open(dir.path())?;
    store.keep_days = 0;
    for stamp in [
        "2026-10-21T00:00:00Z",
        "2026-10-21T00:00:00.1Z",
        "2026-10-21T00:00:00.100000001Z",
        "2026-10-21T00:00:00.11Z",
        "2026-10-21T00:00:01Z",
    ] {
        let mut row = entry(0);
        row.timestamp = stamp.into();
        store.insert(&row)?;
    }
    let page = store.history_search(HistoryFilter {
        since: Some("2026-10-21T09:00:00.100000000+09:00".into()),
        until: Some("2026-10-21T00:00:00.11Z".into()),
        limit: 100,
        ..Default::default()
    })?;
    assert_eq!(
        page.items
            .iter()
            .map(|r| r.timestamp.as_str())
            .collect::<Vec<_>>(),
        vec!["2026-10-21T00:00:00.100000001Z", "2026-10-21T00:00:00.1Z"]
    );
    let page = store.history_search(HistoryFilter {
        since: Some("2026-10-21T00:00:00.100000001Z".into()),
        until: Some("2026-10-21T00:00:01Z".into()),
        limit: 100,
        ..Default::default()
    })?;
    assert_eq!(page.items.len(), 2);
    Ok(())
}

#[test]
fn ordering_schema_upgrades_existing_snippets_idempotently() -> Result<(), DbError> {
    let dir = tempfile::tempdir().map_err(|_| DbError::internal("Test directory unavailable"))?;
    let mut store = Store::open(dir.path())?;
    let pinned = fixture_snippet(&mut store, None, true)?;
    fixture_snippet(&mut store, None, false)?;
    store.connection.execute_batch("DROP TRIGGER snippet_order_ai; DROP TRIGGER snippet_order_au; DROP TABLE snippet_order; DROP INDEX snippets_pinned_updated;").map_err(database_error)?;
    drop(store);
    for _ in 0..2 {
        let store = Store::open(dir.path())?;
        let page = store.snippet_list(SnippetFilter {
            limit: 100,
            ..Default::default()
        })?;
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, pinned.id);
        let revisions: i64 = store
            .connection
            .query_row("SELECT count(*) FROM snippet_order", [], |r| r.get(0))
            .map_err(database_error)?;
        assert_eq!(revisions, 2);
    }
    Ok(())
}

#[test]
fn short_snippet_search_keeps_sorted_empty_window_cursor() -> Result<(), DbError> {
    let dir = tempfile::tempdir().map_err(|_| DbError::internal("Test directory unavailable"))?;
    let mut store = Store::open(dir.path())?;
    let wanted = store.snippet_save(SnippetSave {
        id: None,
        title: "찾".into(),
        body: "select 1".into(),
        tags: vec![],
        profile_id: None,
        language: "sql".into(),
        pinned: false,
    })?;
    for _ in 0..SHORT_SCAN_ROWS {
        store.snippet_save(SnippetSave {
            id: None,
            title: "other".into(),
            body: "select 1".into(),
            tags: vec![],
            profile_id: None,
            language: "sql".into(),
            pinned: true,
        })?;
    }
    let first = store.snippet_list(SnippetFilter {
        query: Some("찾".into()),
        limit: 10,
        ..Default::default()
    })?;
    assert!(first.items.is_empty());
    assert!(first.next_cursor.is_some());
    let second = store.snippet_list(SnippetFilter {
        query: Some("찾".into()),
        before_id: first.next_cursor.map(Into::into),
        limit: 10,
        ..Default::default()
    })?;
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].id, wanted.id);
    assert!(second.next_cursor.is_none());
    Ok(())
}
