//! ETL commands (ADR-0005). The engine in `axdb-etl` runs over the host's open sessions so the
//! existing read-only, import, preview-receipt, cancellation and history boundaries apply.
use super::*;
use axdb_etl::engine::{self, Batch, Extractor, LoadOutput, LoadRequest, Loader, Observer, PreviewReport, Progress, RunReport, RunStatus};
use axdb_etl::spec::{Pipeline, Source};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MAX_PIPELINES: usize = 200;
pub const MAX_RUN_RECORDS: usize = 500;
/// Same transport ceiling as file import; a batch is one `import_rows` request.
const MAX_BATCH_BYTES: usize = 12 * 1024 * 1024;
const PREVIEW_SAMPLE_ROWS: u32 = 50;

fn pipelines_path(host: &Host) -> PathBuf {
    host.paths.data.join("etl").join("pipelines.json")
}
fn runs_path(host: &Host) -> PathBuf {
    host.paths.data.join("etl").join("runs.jsonl")
}
fn ensure_dir(path: &std::path::Path) -> CommandResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| DbError::internal("Cannot create ETL folder"))?;
    }
    Ok(())
}
fn read_pipelines(path: &std::path::Path) -> CommandResult<Vec<Pipeline>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| DbError::internal("파이프라인 파일을 읽을 수 없습니다")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err(DbError::internal("파이프라인 파일을 읽을 수 없습니다")),
    }
}
fn write_json(path: &std::path::Path, value: &impl Serialize) -> CommandResult<()> {
    ensure_dir(path)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| DbError::internal("Cannot encode ETL file"))?;
    storage::write_private(path, &bytes)
}

#[tauri::command]
pub async fn etl_list(host: tauri::State<'_, Host>) -> CommandResult<Vec<Pipeline>> {
    let _guard = host.storage.lock().await;
    read_pipelines(&pipelines_path(&host))
}
pub(crate) async fn save(host: &Host, mut pipeline: Pipeline) -> CommandResult<Pipeline> {
    pipeline.name = pipeline.name.trim().to_owned();
    pipeline.validate()?;
    let _guard = host.storage.lock().await;
    let path = pipelines_path(host);
    let mut all = read_pipelines(&path)?;
    if pipeline.id.is_empty() {
        pipeline.id = uuid::Uuid::new_v4().to_string();
    }
    match all.iter().position(|p| p.id == pipeline.id) {
        Some(at) => all[at] = pipeline.clone(),
        None if all.len() >= MAX_PIPELINES => {
            return Err(DbError::query("파이프라인은 200개까지 저장할 수 있습니다"));
        }
        None => all.push(pipeline.clone()),
    }
    write_json(&path, &all)?;
    Ok(pipeline)
}
#[tauri::command]
pub async fn etl_save(host: tauri::State<'_, Host>, pipeline: Pipeline) -> CommandResult<Pipeline> {
    save(&host, pipeline).await
}
#[tauri::command]
pub async fn etl_delete(host: tauri::State<'_, Host>, id: String) -> CommandResult<()> {
    let _guard = host.storage.lock().await;
    let path = pipelines_path(&host);
    let mut all = read_pipelines(&path)?;
    let before = all.len();
    all.retain(|p| p.id != id);
    if all.len() == before {
        return Err(DbError::query("파이프라인을 찾을 수 없습니다"));
    }
    write_json(&path, &all)
}

/// Persisted run summary. Database error text can quote row values, so only the status is kept.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunEntry {
    pub pipeline_id: String,
    pub pipeline_name: String,
    pub run_id: String,
    pub finished_at: String,
    pub status: RunStatus,
    #[serde(flatten)]
    pub progress: Progress,
    pub source_truncated: bool,
    pub elapsed_ms: u64,
}
#[tauri::command]
pub async fn etl_runs(host: tauri::State<'_, Host>, limit: Option<u32>) -> CommandResult<Vec<RunEntry>> {
    let _guard = host.storage.lock().await;
    let mut runs = read_runs(&runs_path(&host))?;
    runs.reverse();
    runs.truncate(limit.unwrap_or(50).clamp(1, MAX_RUN_RECORDS as u32) as usize);
    Ok(runs)
}
fn read_runs(path: &std::path::Path) -> CommandResult<Vec<RunEntry>> {
    match std::fs::read_to_string(path) {
        // Skip damaged lines instead of losing the whole log.
        Ok(text) => Ok(text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err(DbError::internal("실행 기록을 읽을 수 없습니다")),
    }
}
async fn record_run(host: &Host, entry: RunEntry) -> CommandResult<()> {
    let _guard = host.storage.lock().await;
    let path = runs_path(host);
    let mut runs = read_runs(&path)?;
    runs.push(entry);
    let skip = runs.len().saturating_sub(MAX_RUN_RECORDS);
    let mut text = String::new();
    for run in &runs[skip..] {
        text.push_str(&serde_json::to_string(run).map_err(|_| DbError::internal("Cannot encode run"))?);
        text.push('\n');
    }
    ensure_dir(&path)?;
    storage::write_private(&path, text.as_bytes())
}

struct Prepared {
    source: Arc<SessionRecord>,
    target: Arc<SessionRecord>,
    fingerprint: String,
}
fn prepare(host: &Host, pipeline: &Pipeline, source_id: &str, target_id: &str) -> CommandResult<Prepared> {
    pipeline.validate()?;
    let source = host.session(source_id)?;
    let target = host.session(target_id)?;
    if source.profile.id != pipeline.source.profile_id() {
        return Err(DbError::query("소스 세션이 파이프라인의 소스 연결과 다릅니다"));
    }
    if target.profile.id != pipeline.target.profile_id {
        return Err(DbError::query("대상 세션이 파이프라인의 대상 연결과 다릅니다"));
    }
    let caps = host.driver(&source.profile.driver_id)?.manifest().capabilities.clone();
    match pipeline.source {
        Source::Query { .. } if !caps.read_only_exec => {
            return Err(DbError::unsupported("소스 드라이버가 읽기 전용 실행을 보장하지 않아 쿼리 소스를 쓸 수 없습니다"));
        }
        Source::Table { .. } if !caps.paged_browse => {
            return Err(DbError::unsupported("소스 드라이버가 페이지 조회를 지원하지 않습니다"));
        }
        _ => {}
    }
    check_target(host, &target)?;
    let fingerprint = permissions::fingerprint(&format!("etl:{source_id}:{target_id}"), pipeline)?;
    Ok(Prepared { source, target, fingerprint })
}
/// Checked before preview, before run and again before every batch: the profile may change mid-run.
fn check_target(host: &Host, target: &SessionRecord) -> CommandResult<()> {
    if host.access_mode(target)? == permissions::AccessMode::ReadOnly {
        return Err(DbError::new(ErrorKind::Auth, "대상 연결이 읽기 전용입니다"));
    }
    if !host.driver(&target.profile.driver_id)?.manifest().capabilities.import_rows {
        return Err(DbError::unsupported("대상 드라이버가 일괄 적재를 지원하지 않습니다"));
    }
    Ok(())
}

/// Await a driver call with a deadline. On timeout the call is cancelled and its cleanup awaited.
async fn bounded<T>(
    ms: u64,
    cancel: &CancelToken,
    future: impl std::future::Future<Output = Result<T, DbError>>,
) -> Result<T, DbError> {
    tokio::pin!(future);
    match tokio::time::timeout(Duration::from_millis(ms), &mut future).await {
        Ok(result) => result,
        Err(_) => {
            cancel.cancel();
            let _ = future.await;
            Err(DbError::new(ErrorKind::Timeout, "ETL 배치 제한 시간을 넘었습니다"))
        }
    }
}

enum Pending {
    Table { node_id: String, offset: u64, done: bool },
    Query { text: String, rows: Option<(Vec<Column>, std::vec::IntoIter<Vec<Value>>)> },
}
struct SessionExtractor<'a> {
    host: &'a Host,
    record: Arc<SessionRecord>,
    pending: Pending,
    ceiling: Option<u64>,
    timeout_ms: u64,
    truncated: bool,
}
impl<'a> SessionExtractor<'a> {
    fn new(host: &'a Host, record: Arc<SessionRecord>, pipeline: &Pipeline) -> Self {
        let pending = match &pipeline.source {
            Source::Table { node_id, .. } => Pending::Table { node_id: node_id.clone(), offset: 0, done: false },
            Source::Query { text, .. } => Pending::Query { text: text.clone(), rows: None },
        };
        Self { host, record, pending, ceiling: pipeline.read_limit(), timeout_ms: pipeline.options.batch_timeout_ms, truncated: false }
    }
}
#[async_trait::async_trait]
impl Extractor for SessionExtractor<'_> {
    async fn next(&mut self, limit: u32, cancel: &CancelToken) -> Result<Option<Batch>, DbError> {
        match &mut self.pending {
            Pending::Table { node_id, offset, done } => {
                if *done {
                    return Ok(None);
                }
                let page = PageRequest { offset: *offset, limit: clamp_row_limit(limit), filter: vec![] };
                let future = self.record.session.browse(node_id, page);
                tokio::pin!(future);
                // `browse` takes no cancel token; like the browse command, a timeout closes the session.
                let result = match tokio::time::timeout(Duration::from_millis(self.timeout_ms), &mut future).await {
                    Ok(result) => result,
                    Err(_) => {
                        self.record.dead.store(true, Ordering::Release);
                        self.record.session.close().await;
                        let _ = future.await;
                        Err(DbError::new(ErrorKind::Timeout, "소스 페이지 조회가 제한 시간을 넘어 연결을 닫았습니다"))
                    }
                };
                let result = self.host.observe(&self.record, result)?;
                let has_more = result.page.as_ref().map(|p| p.has_more);
                let Some(table) = result.table else {
                    return Err(DbError::query("소스가 표 결과를 반환하지 않았습니다"));
                };
                *offset += table.rows.len() as u64;
                *done = !has_more.unwrap_or(table.rows.len() as u32 >= limit) || table.rows.is_empty();
                Ok(Some(Batch { columns: table.columns, rows: table.rows }))
            }
            Pending::Query { text, rows } => {
                if rows.is_none() {
                    let row_limit = clamp_row_limit(self.ceiling.unwrap_or(axdb_etl::spec::MAX_QUERY_ROWS).min(u64::from(u32::MAX)) as u32);
                    let req = ExecRequest {
                        read_only: true,
                        text: std::mem::take(text),
                        params: Default::default(),
                        row_limit,
                        graph_node_limit: 300,
                        timeout_ms: self.timeout_ms,
                        preview_sample_limit: 50,
                    };
                    let child = cancel.child_token();
                    let result = bounded(self.timeout_ms, &child, self.record.session.execute(&req, child.clone())).await;
                    let results = self.host.observe(&self.record, result)?;
                    let Some((table, truncated)) = results.into_iter().find_map(|r| Some((r.table?, r.truncated))) else {
                        return Err(DbError::query("소스 쿼리가 표 결과를 반환하지 않았습니다"));
                    };
                    self.truncated = truncated;
                    *rows = Some((table.columns, table.rows.into_iter()));
                }
                let Some((columns, iter)) = rows.as_mut() else { return Ok(None) };
                let batch: Vec<_> = iter.by_ref().take(limit as usize).collect();
                Ok((!batch.is_empty()).then(|| Batch { columns: columns.clone(), rows: batch }))
            }
        }
    }
    fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Batch progress is reported by the engine; per-row driver progress is not forwarded.
struct NoProgress;
#[async_trait::async_trait]
impl ProgressSink for NoProgress {
    async fn progress(&self, _: u64, _: u64) -> Result<(), DbError> {
        Ok(())
    }
}
struct SessionLoader<'a> {
    host: &'a Host,
    record: Arc<SessionRecord>,
    node_id: String,
    timeout_ms: u64,
}
#[async_trait::async_trait]
impl Loader for SessionLoader<'_> {
    async fn load(&mut self, req: LoadRequest, cancel: &CancelToken) -> Result<LoadOutput, DbError> {
        check_target(self.host, &self.record)?;
        let count = req.rows.len() as u64;
        let edits = req
            .rows
            .into_iter()
            .map(|row| RowEdit::Insert { values: req.columns.iter().cloned().zip(row).collect() })
            .collect();
        let edit = EditRequest {
            originals: vec![],
            create_table: req.create_table,
            node_id: self.node_id.clone(),
            edits,
            timeout_ms: self.timeout_ms,
            preview_sample_limit: 100,
        };
        let size = serde_json::to_vec(&edit).map_err(|_| DbError::query("적재 요청을 만들 수 없습니다"))?.len();
        if size > MAX_BATCH_BYTES {
            return Err(DbError::query("배치가 전송 한도(12 MiB)를 넘습니다. 배치 크기를 줄이세요"));
        }
        let child = cancel.child_token();
        // The progress variant is the one drivers specialise (PostgreSQL applies with COPY).
        let result = bounded(
            self.timeout_ms,
            &child,
            self.record.session.import_rows_progress(&edit, req.preview, child.clone(), &NoProgress),
        )
        .await;
        match self.host.observe(&self.record, result)? {
            ImportOutput::Preview(mut p) => {
                let mut remaining = 100usize;
                for s in &mut p.statements {
                    for t in [&mut s.before, &mut s.after].into_iter().flatten() {
                        t.rows.truncate(remaining);
                        remaining = remaining.saturating_sub(t.rows.len());
                    }
                }
                Ok(LoadOutput::Preview(p))
            }
            ImportOutput::Applied(_) => Ok(LoadOutput::Applied(count)),
        }
    }
}

#[derive(Clone, Serialize)]
struct ProgressEvent<'a> {
    run_id: &'a str,
    #[serde(flatten)]
    progress: &'a Progress,
}
struct Events<'a> {
    app: Option<&'a tauri::AppHandle>,
    run_id: &'a str,
}
impl Observer for Events<'_> {
    fn progress(&self, progress: &Progress) {
        if let Some(app) = self.app {
            use tauri::Emitter;
            let _ = app.emit("etl-progress", ProgressEvent { run_id: self.run_id, progress });
        }
    }
}

fn now() -> String {
    time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}
async fn history(host: &Host, kind: &str, target: &SessionRecord, pipeline: &Pipeline, elapsed_ms: u64, rows: u64, error: Option<String>) {
    let name = storage::redact(&pipeline.name, target.secret.as_deref());
    let _ = host
        .history(HistoryEntry {
            kind: kind.into(),
            source: HistorySource::Ui,
            timestamp: now(),
            profile_id: Some(target.profile.id.clone()),
            profile_name: storage::redact(&target.profile.name, target.secret.as_deref()),
            text: format!("ETL '{name}' (values omitted)"),
            elapsed_ms,
            row_count: rows,
            error,
        })
        .await;
}

pub(crate) async fn preview_impl(
    host: &Host,
    pipeline: Pipeline,
    source_session_id: String,
    target_session_id: String,
    run_id: String,
    sample_rows: Option<u32>,
) -> CommandResult<PreviewReport> {
    let prepared = prepare(host, &pipeline, &source_session_id, &target_session_id)?;
    let run = host.run(&target_session_id, &run_id)?;
    let started = Instant::now();
    let mut extractor = SessionExtractor::new(host, prepared.source.clone(), &pipeline);
    let mut loader = SessionLoader { host, record: prepared.target.clone(), node_id: pipeline.target.node_id.clone(), timeout_ms: pipeline.options.batch_timeout_ms };
    let result = engine::preview(&pipeline, &mut extractor, &mut loader, sample_rows.unwrap_or(PREVIEW_SAMPLE_ROWS), &run.token).await;
    if result.is_ok() {
        management::remember(host, prepared.fingerprint)?;
    }
    let rows = result.as_ref().map(|r| r.sample.len() as u64).unwrap_or(0);
    let error = result.as_ref().err().map(|_| "ETL preview failed".to_owned());
    history(host, "etl_preview", &prepared.target, &pipeline, started.elapsed().as_millis() as u64, rows, error).await;
    result
}
#[tauri::command]
pub async fn etl_preview(
    host: tauri::State<'_, Host>,
    pipeline: Pipeline,
    source_session_id: String,
    target_session_id: String,
    run_id: String,
    sample_rows: Option<u32>,
) -> CommandResult<PreviewReport> {
    preview_impl(&host, pipeline, source_session_id, target_session_id, run_id, sample_rows).await
}

pub(crate) async fn run_impl(
    host: &Host,
    app: Option<&tauri::AppHandle>,
    pipeline: Pipeline,
    source_session_id: String,
    target_session_id: String,
    run_id: String,
) -> CommandResult<RunReport> {
    let prepared = prepare(host, &pipeline, &source_session_id, &target_session_id)?;
    // Single use: a failed or cancelled run needs a fresh preview.
    management::receipt(host, &prepared.fingerprint, false)?;
    let run = host.run(&target_session_id, &run_id)?;
    let mut extractor = SessionExtractor::new(host, prepared.source.clone(), &pipeline);
    let mut loader = SessionLoader { host, record: prepared.target.clone(), node_id: pipeline.target.node_id.clone(), timeout_ms: pipeline.options.batch_timeout_ms };
    let report = engine::run(&pipeline, &mut extractor, &mut loader, &Events { app, run_id: &run_id }, &run.token).await;
    drop(run);
    let error = match report.status {
        RunStatus::Completed => None,
        RunStatus::Cancelled => Some("ETL run cancelled".to_owned()),
        RunStatus::Failed => Some("ETL run failed; committed batches remain".to_owned()),
    };
    history(host, "etl_run", &prepared.target, &pipeline, report.elapsed_ms, report.progress.rows_written, error).await;
    record_run(
        host,
        RunEntry {
            pipeline_id: pipeline.id.clone(),
            pipeline_name: storage::redact(&pipeline.name, prepared.target.secret.as_deref()),
            run_id,
            finished_at: now(),
            status: report.status,
            progress: report.progress.clone(),
            source_truncated: report.source_truncated,
            elapsed_ms: report.elapsed_ms,
        },
    )
    .await?;
    Ok(report)
}
#[tauri::command]
pub async fn etl_run(
    app: tauri::AppHandle,
    host: tauri::State<'_, Host>,
    pipeline: Pipeline,
    source_session_id: String,
    target_session_id: String,
    run_id: String,
) -> CommandResult<RunReport> {
    run_impl(&host, Some(&app), pipeline, source_session_id, target_session_id, run_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axdb_etl::spec::{CastType, Cmp, Options, Step, Target, TextFunc};
    use dbcore::LockExt;

    const ROWS: i64 = 2_500;
    async fn connect(dir: &std::path::Path, file: &str, sql: &str) -> Box<dyn Session> {
        let session = driver_sqlite::SqliteDriver::new()
            .connect(&ConnectConfig {
                fields: std::collections::BTreeMap::from([("path".into(), dir.join(file).to_string_lossy().into_owned())]),
                secret: None,
                connect_timeout_ms: 1000,
            })
            .await
            .unwrap();
        exec(&*session, sql).await;
        session
    }
    async fn exec(session: &dyn Session, sql: &str) -> Vec<ResultSet> {
        let req: ExecRequest = serde_json::from_value(serde_json::json!({"text": sql, "row_limit": 100000})).unwrap();
        session.execute(&req, CancelToken::new()).await.unwrap()
    }
    fn insert(host: &Host, id: &str, profile: &str, session: Box<dyn Session>, read_only: bool) {
        let mut profile: Profile =
            serde_json::from_value(serde_json::json!({"id": profile, "name": "etl", "driver_id": "sqlite", "fields": {}})).unwrap();
        if read_only {
            profile.access_mode = permissions::AccessMode::ReadOnly;
        }
        host.sessions.lock_or_recover().insert(
            id.into(),
            Arc::new(SessionRecord { session: Arc::from(session), profile, secret: None, _tunnel: None, dead: AtomicBool::new(false) }),
        );
    }
    async fn fixture() -> (Host, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::new(Paths { config: dir.path().into(), data: dir.path().into(), log: dir.path().into() });
        host.registry.register(Arc::new(driver_sqlite::SqliteDriver::new()));
        let rows: Vec<String> = (1..=ROWS).map(|i| format!("({i},'secret-name-{i}','{}')", i % 90)).collect();
        let sql = format!("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, age TEXT); INSERT INTO users VALUES {}", rows.join(","));
        insert(&host, "src", "ps", connect(dir.path(), "src.db", &sql).await, false);
        let target = connect(dir.path(), "dst.db", "CREATE TABLE copy(id INTEGER PRIMARY KEY, name TEXT, age TEXT)").await;
        insert(&host, "dst", "pd", target, false);
        (host, dir)
    }
    fn table_copy() -> Pipeline {
        Pipeline {
            id: String::new(),
            name: "users copy".into(),
            description: String::new(),
            version: axdb_etl::spec::SPEC_VERSION,
            source: Source::Table { profile_id: "ps".into(), node_id: "table:users".into() },
            steps: vec![],
            target: Target { profile_id: "pd".into(), node_id: "table:copy".into(), create_table: false },
            options: Options::default(),
        }
    }
    async fn count(host: &Host, session: &str, from: &str) -> String {
        let record = host.session(session).unwrap();
        let sets = exec(&*record.session, &format!("SELECT count(*) FROM {from}")).await;
        match &sets[0].table.as_ref().unwrap().rows[0][0] {
            Value::Int(n) => n.clone(),
            other => panic!("{other:?}"),
        }
    }
    async fn preview(host: &Host, p: &Pipeline, run: &str) -> CommandResult<PreviewReport> {
        preview_impl(host, p.clone(), "src".into(), "dst".into(), run.into(), None).await
    }
    async fn run(host: &Host, p: &Pipeline, run: &str) -> CommandResult<RunReport> {
        run_impl(host, None, p.clone(), "src".into(), "dst".into(), run.into()).await
    }
    fn auth<T: std::fmt::Debug>(result: CommandResult<T>) -> bool {
        matches!(result, Err(DbError { kind: ErrorKind::Auth, .. }))
    }

    #[tokio::test]
    async fn table_copy_needs_a_fresh_single_use_preview() {
        let (host, _dir) = fixture().await;
        let p = table_copy();
        assert!(auth(run(&host, &p, "r0").await));
        let report = preview(&host, &p, "p1").await.unwrap();
        assert_eq!(report.sample.len(), 50);
        assert!(report.target.as_ref().is_some_and(|t| !t.committed));
        assert_eq!(count(&host, "dst", "copy").await, "0", "preview must roll back");
        // A changed spec does not match the receipt, and the mismatch does not consume it.
        let mut changed = p.clone();
        changed.options.batch_size = 999;
        assert!(auth(run(&host, &changed, "r1").await));
        let report = run(&host, &p, "r2").await.unwrap();
        assert_eq!(report.status, RunStatus::Completed, "{report:?}");
        let progress = &report.progress;
        assert_eq!((progress.rows_read, progress.rows_written, progress.batches_committed), (ROWS as u64, ROWS as u64, 3));
        assert_eq!(count(&host, "dst", "copy").await, ROWS.to_string());
        assert!(auth(run(&host, &p, "r3").await), "receipt is single use");
    }
    #[tokio::test]
    async fn query_source_transforms_into_a_created_table() {
        let (host, _dir) = fixture().await;
        let p = Pipeline {
            source: Source::Query { profile_id: "ps".into(), text: "SELECT id, name, age FROM users WHERE id <= 900".into() },
            steps: vec![
                Step::Cast { column: "age".into(), to: CastType::Int },
                Step::Filter { column: "age".into(), cmp: Cmp::Ge, value: Some(Value::Int("45".into())) },
                Step::Text { column: "name".into(), func: TextFunc::Upper },
                Step::Constant { column: "origin".into(), value: Value::Text("crm".into()) },
            ],
            target: Target { profile_id: "pd".into(), node_id: "table:adults".into(), create_table: true },
            ..table_copy()
        };
        let report = preview(&host, &p, "p").await.unwrap();
        let types: Vec<_> = report.output_columns.iter().map(|c| c.type_name.as_str()).collect();
        assert_eq!(types, ["integer", "text", "integer", "text"]);
        let report = run(&host, &p, "r").await.unwrap();
        assert_eq!(report.status, RunStatus::Completed, "{report:?}");
        // Ages are i % 90, so ids 1..=900 have exactly 45 of every 90 at or above 45.
        assert_eq!(report.progress.rows_written, 450);
        assert_eq!(count(&host, "dst", "adults WHERE origin = 'crm' AND name LIKE 'SECRET-NAME-%'").await, "450");
    }
    #[tokio::test]
    async fn boundaries_are_enforced_before_any_write() {
        let (host, dir) = fixture().await;
        let mut wrong = table_copy();
        wrong.source = Source::Table { profile_id: "pd".into(), node_id: "table:users".into() };
        assert!(preview(&host, &wrong, "a").await.unwrap_err().message.contains("소스 세션"));
        // A write disguised as a source query is stopped by read-only execution.
        let mut writes = table_copy();
        writes.source = Source::Query { profile_id: "ps".into(), text: "UPDATE users SET name = 'x' RETURNING id".into() };
        assert!(preview(&host, &writes, "b").await.is_err());
        assert_eq!(count(&host, "src", "users WHERE name = 'x'").await, "0");
        // Read-only target.
        insert(&host, "dst", "pd", connect(dir.path(), "ro.db", "CREATE TABLE copy(id INTEGER)").await, true);
        assert!(auth(preview(&host, &table_copy(), "c").await));
        // A missing target table fails inside the rollback preview and leaves no receipt.
        insert(&host, "dst", "pd", connect(dir.path(), "dst2.db", "SELECT 1").await, false);
        let mut missing = table_copy();
        missing.target.node_id = "table:nope".into();
        assert!(preview(&host, &missing, "d").await.is_err());
        assert!(auth(run(&host, &missing, "e").await));
    }
    #[tokio::test]
    async fn records_keep_counts_but_no_values() {
        let (host, dir) = fixture().await;
        let p = table_copy();
        preview(&host, &p, "p").await.unwrap();
        run(&host, &p, "r").await.unwrap();
        let runs = read_runs(&runs_path(&host)).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].progress.rows_written, ROWS as u64);
        let text = std::fs::read_to_string(runs_path(&host)).unwrap();
        assert!(!text.contains("secret-name"), "{text}");
        let library = std::fs::read(dir.path().join("library.sqlite3")).unwrap_or_default();
        assert!(!String::from_utf8_lossy(&library).contains("secret-name"));
    }
    #[tokio::test]
    async fn receipts_are_bound_to_the_session_pair_and_live_permissions() {
        let (host, dir) = fixture().await;
        let p = table_copy();
        // Same profile, different session: the receipt does not transfer.
        insert(&host, "dst2", "pd", connect(dir.path(), "dst.db", "SELECT 1").await, false);
        preview(&host, &p, "p1").await.unwrap();
        assert!(auth(run_impl(&host, None, p.clone(), "src".into(), "dst2".into(), "r1".into()).await));
        // The target is switched to read-only after the preview: the run is refused before writing.
        preview(&host, &p, "p2").await.unwrap();
        let mut profile = host.session("dst").unwrap().profile.clone();
        profile.access_mode = permissions::AccessMode::ReadOnly;
        std::fs::write(dir.path().join("profiles.json"), serde_json::to_vec(&[profile]).unwrap()).unwrap();
        assert!(auth(run(&host, &p, "r2").await));
        assert_eq!(count(&host, "dst", "copy").await, "0");
    }
    #[tokio::test]
    async fn run_ids_writes_and_oversized_batches_are_refused() {
        let (host, dir) = fixture().await;
        let busy = host.run("dst", "same").unwrap();
        assert!(preview(&host, &table_copy(), "same").await.unwrap_err().message.contains("already active"));
        drop(busy);
        let mut multi = table_copy();
        multi.source = Source::Query { profile_id: "ps".into(), text: "SELECT 1; UPDATE users SET name = 'x'".into() };
        assert!(preview(&host, &multi, "m").await.is_err());
        assert_eq!(count(&host, "src", "users WHERE name = 'x'").await, "0");
        // 20 rows of 1 MB each exceed the 12 MiB transport ceiling of one batch.
        let wide = connect(dir.path(), "wide.db", "CREATE TABLE w(v TEXT); WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 20) INSERT INTO w SELECT printf('%.1000000c', 'x') FROM n").await;
        insert(&host, "src", "ps", wide, false);
        let mut big = table_copy();
        big.source = Source::Table { profile_id: "ps".into(), node_id: "table:w".into() };
        big.target.node_id = "table:w".into();
        big.target.create_table = true;
        assert!(preview(&host, &big, "w").await.unwrap_err().message.contains("12 MiB"));
    }
    #[tokio::test]
    async fn saved_pipelines_round_trip_and_are_validated() {
        let (host, _dir) = fixture().await;
        let saved = save(&host, table_copy()).await.unwrap();
        assert!(!saved.id.is_empty());
        let mut renamed = saved.clone();
        renamed.name = "  renamed  ".into();
        save(&host, renamed).await.unwrap();
        let all = read_pipelines(&pipelines_path(&host)).unwrap();
        assert_eq!((all.len(), all[0].name.as_str()), (1, "renamed"));
        let mut bad = table_copy();
        bad.options.batch_size = 0;
        assert!(save(&host, bad).await.is_err());
        assert_eq!(read_pipelines(&pipelines_path(&host)).unwrap().len(), 1);
    }
}
