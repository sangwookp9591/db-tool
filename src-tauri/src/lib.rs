mod recycle;
pub mod net;
mod management;
pub mod backup_file;
pub mod driver_install;
mod agent_bridge;
mod onboarding;
mod updater;
mod context;
mod assistant;
mod agent_dispatch;
mod permissions;
mod edits;
mod editor;
mod export;
pub mod drivers;
pub mod plugin_host;
pub mod storage;
mod star;
pub mod stream;
mod viewer_bench;
use dbcore::*;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use storage::{HistoryEntry, HistorySource, Paths, Profile};
use tauri::Manager;
use tracing_subscriber::prelude::*;

struct SessionRecord {
    session: Arc<dyn Session>,
    profile: Profile,
    secret: Option<String>,
    _tunnel: Option<net::Tunnel>,
    dead: AtomicBool,
}
struct RunRecord {
    token: CancelToken,
    session_id: String,
}
pub struct Host {
    agent_results: agent_bridge::results::Store,
    backup_statuses: Mutex<HashMap<String,management::BackupStatus>>,
    restore_files: Mutex<HashMap<String,management::administration::files::Selected>>,
    network: Arc<net::State>,
    registry: std::sync::LazyLock<Registry, Box<dyn FnOnce() -> Registry + Send + Sync>>,
    paths: Paths,
    sessions: Mutex<HashMap<String, Arc<SessionRecord>>>,
    runs: Mutex<HashMap<String, RunRecord>>,
    context_runs: Mutex<std::collections::VecDeque<context::RunContext>>,
    storage: tokio::sync::Mutex<()>,
    agent_storage: Mutex<()>,
    approvals: Mutex<HashMap<String, permissions::Approval>>,
    previews: Mutex<HashMap<String, permissions::Approval>>,
}
impl Host {
    pub fn new(paths: Paths) -> Self {
        let driver_root = paths.data.join("drivers");
        Self {
            agent_results: Default::default(),
            network: Arc::new(net::State::default()),
            backup_statuses: Mutex::default(),
            restore_files: Mutex::default(),
            registry: std::sync::LazyLock::new(Box::new(move || drivers::registry(&driver_root))),
            paths,
            sessions: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
            context_runs: Mutex::new(std::collections::VecDeque::new()),
            storage: tokio::sync::Mutex::new(()),
            agent_storage: Mutex::new(()),
            approvals: Mutex::new(HashMap::new()),
            previews: Mutex::new(HashMap::new()),
        }
    }
    fn session(&self, id: &str) -> Result<Arc<SessionRecord>, DbError> {
        let record = self
            .sessions
            .lock()
            .map_err(|_| DbError::internal("Session lock unavailable"))?
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "세션을 찾을 수 없습니다"))?;
        if record.dead.load(Ordering::Acquire) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "연결이 끊겼습니다. 다시 연결하세요",
            ));
        }
        Ok(record)
    }
    fn driver(&self, id: &str) -> Result<Arc<dyn Driver>, DbError> {
        self.registry
            .get(id)
            .ok_or_else(|| DbError::unsupported("Unknown driver"))
    }
    fn run(&self, session_id: &str, run_id: &str) -> Result<RunGuard<'_>, DbError> {
        if run_id.is_empty() {
            return Err(DbError::query("run_id is required"));
        }
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| DbError::internal("Run lock unavailable"))?;
        if runs.contains_key(run_id) {
            return Err(DbError::query("run_id is already active"));
        }
        let token = CancelToken::new();
        runs.insert(
            run_id.into(),
            RunRecord {
                token: token.clone(),
                session_id: session_id.into(),
            },
        );
        Ok(RunGuard {
            host: self,
            id: run_id.into(),
            token,
        })
    }
    async fn connect(
        &self,
        profile: &Profile,
        secret: Option<String>,
    ) -> Result<(Box<dyn Session>, Option<String>, Option<net::Tunnel>), DbError> {
        self.connect_progress(profile, secret, net::Progress::default()).await
    }
    async fn connect_progress(
        &self, profile: &Profile, secret: Option<String>, progress: net::Progress,
    ) -> Result<(Box<dyn Session>, Option<String>, Option<net::Tunnel>), DbError> {
        let driver = self.driver(&profile.driver_id)?;
        let mut safe = profile.clone();
        let uri_secret = net::extract_uri_secret(&mut safe)?;
        let inline = storage::strip_secrets(&mut safe, driver.manifest()).or(uri_secret);
        let secret = match secret.or(inline) {
            Some(s) => Some(s),
            None if safe.save_secret => { let _bench = viewer_bench::span("keychain_get"); storage::keychain_get(&profile.id)? },
            None => None,
        };
        safe.fields.retain(|key, _| {
            driver
                .manifest()
                .connection_fields
                .iter()
                .any(|f| !f.secret && f.key == *key)
        });
        tracing::info!(driver=%safe.driver_id,field_names=?safe.fields.keys().collect::<Vec<_>>(),"Connecting");
        let prepared = net::prepare(&safe, safe.fields.clone(), secret, driver.manifest(), &self.paths.data, self.network.clone(), progress.clone()).await?;
        let tls = !prepared.tls_offloaded && net::tls_required(&prepared.fields);
        let start = Instant::now();
        if !prepared.tls_offloaded { progress.stage("tls", if tls { "running" } else { "skipped" }, start, None); }
        progress.stage("db_auth", "running", start, None);
        let result = driver
            .connect(&ConnectConfig {
                fields: prepared.fields,
                secret: prepared.secret.clone(),
                connect_timeout_ms: 5000,
            })
            .await;
        match &result {
            Ok(_) => {
                if tls { progress.stage("tls", "passed", start, None); }
                progress.stage("db_auth", "passed", start, None);
            }
            Err(e) => {
                let (stage, safe) = net::database_error(e, !matches!(profile.auth, net::model::Auth::Password));
                if tls && stage != "tls" {
                    progress.stage("tls", if e.kind == ErrorKind::Auth { "passed" } else { "skipped" }, start, None);
                }
                if stage == "tls" { progress.stage("db_auth", "skipped", start, None); }
                progress.stage(stage, "failed", start, Some(&safe));
            }
        }
        Ok((result?, prepared.secret, prepared.tunnel))
    }
    fn access_mode(&self, record: &SessionRecord) -> Result<permissions::AccessMode, DbError> {
        Ok(storage::read_profiles(&self.paths.config.join("profiles.json"))?.into_iter().find(|p|p.id==record.profile.id).map(|p|p.access_mode).unwrap_or(record.profile.access_mode))
    }
    fn observe<T>(&self, record: &SessionRecord, result: Result<T, DbError>) -> Result<T, DbError> {
        if let Err(error) = &result {
            if error.kind == ErrorKind::Connection {
                record.dead.store(true, Ordering::Release);
            }
        }
        result
    }
    async fn history(&self, entry: HistoryEntry) -> Result<(), DbError> {
        let _guard = self.storage.lock().await;
        if entry.kind == "execute" && entry.error.is_none() { star::successful_query(&self.paths.config)?; }
        let path = self.paths.data.join("history.jsonl");
        let mut entries = storage::read_history(&path)?;
        entries.push(entry);
        let start = entries.len().saturating_sub(1000);
        let mut data = Vec::new();
        for entry in &entries[start..] {
            serde_json::to_writer(&mut data, entry)
                .map_err(|_| DbError::internal("Cannot serialize history"))?;
            data.push(b'\n');
        }
        storage::write_private(&path, &data)
    }
}
struct RunGuard<'a> {
    host: &'a Host,
    id: String,
    token: CancelToken,
}
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.token.cancel();
        if let Ok(mut runs) = self.host.runs.lock() {
            runs.remove(&self.id);
        }
    }
}

type CommandResult<T> = Result<T, DbError>;
#[tauri::command]
async fn drivers_list(host: tauri::State<'_, Host>) -> CommandResult<Vec<DriverManifest>> {
    let _bench = viewer_bench::span("drivers_list");
    Ok(host.registry.manifests())
}
#[tauri::command]
async fn profiles_list(host: tauri::State<'_, Host>) -> CommandResult<Vec<Profile>> {
    let _guard = host.storage.lock().await;
    let mut profiles = storage::read_profiles(&host.paths.config.join("profiles.json"))?;
    for profile in &mut profiles {
        if let Some(driver) = host.registry.get(&profile.driver_id) {
            storage::strip_secrets(profile, driver.manifest());
        }
    }
    Ok(profiles)
}
#[tauri::command]
async fn profile_save(
    host: tauri::State<'_, Host>,
    mut profile: Profile,
    secret: Option<String>,
) -> CommandResult<Profile> {
    let _bench = viewer_bench::span("profile_save");
    let driver = host.driver(&profile.driver_id)?;
    if profile.access_mode == permissions::AccessMode::ReadOnly && !driver.manifest().capabilities.read_only_exec { return Err(DbError::unsupported("이 드라이버는 읽기 전용 실행을 강제할 수 없습니다")); }
    let uri_secret = net::extract_uri_secret(&mut profile)?;
    let secret = secret.or(storage::strip_secrets(&mut profile, driver.manifest())).or(uri_secret);
    if let Some(network) = &profile.network { network.validate()?; }
    host.network.invalidate(&profile.id);
    if profile.id.is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    if profile.name.trim().is_empty() {
        return Err(DbError::query("연결 이름을 입력하세요"));
    }
    let _guard = host.storage.lock().await;
    let path = host.paths.config.join("profiles.json");
    let mut profiles = storage::read_profiles(&path)?;
    if profile.save_secret {
        if let Some(s) = secret.as_deref() {
            let _keychain = viewer_bench::span("keychain_save");
            storage::keychain_set(&profile.id, Some(s))?;
        }
    } else {
        let _keychain = viewer_bench::span("keychain_delete");
        storage::keychain_set(&profile.id, None)?;
    }
    profiles.retain(|p| p.id != profile.id);
    profiles.push(profile.clone());
    storage::write_private(
        &path,
        &serde_json::to_vec_pretty(&profiles)
            .map_err(|_| DbError::internal("Cannot serialize profiles"))?,
    )?;
    Ok(profile)
}
#[tauri::command]
async fn profile_delete(host: tauri::State<'_, Host>, profile_id: String) -> CommandResult<()> {
    let _bench = viewer_bench::span("profile_delete");
    let _guard = host.storage.lock().await;
    let path = host.paths.config.join("profiles.json");
    let mut profiles = storage::read_profiles(&path)?;
    { let _keychain = viewer_bench::span("keychain_delete"); storage::keychain_set(&profile_id, None)?; }
    net::delete_credentials(&profile_id)?;
    host.network.invalidate(&profile_id);
    profiles.retain(|p| p.id != profile_id);
    storage::write_private(
        &path,
        &serde_json::to_vec_pretty(&profiles)
            .map_err(|_| DbError::internal("Cannot serialize profiles"))?,
    )
}
#[tauri::command]
async fn connection_test(
    host: tauri::State<'_, Host>,
    profile: Profile,
    secret: Option<String>,
) -> CommandResult<String> {
    let _bench = viewer_bench::span("connection_test");
    let (session, _, _tunnel) = host.connect(&profile, secret).await?;
    let result = async {
        session.ping().await?;
        session.server_info().await
    }
    .await;
    session.close().await;
    result
}
#[tauri::command]
async fn session_info(host: tauri::State<'_, Host>, session_id: String) -> CommandResult<String> {
    host.session(&session_id)?.session.server_info().await
}

#[tauri::command]
async fn session_open(
    host: tauri::State<'_, Host>,
    profile_id: String,
    secret: Option<String>,
) -> CommandResult<String> {
    let _bench = viewer_bench::span("session_open");
    let profile = {
        let _guard = host.storage.lock().await;
        storage::read_profiles(&host.paths.config.join("profiles.json"))?
            .into_iter()
            .find(|p| p.id == profile_id)
            .ok_or_else(|| DbError::query("연결 프로필을 찾을 수 없습니다"))?
    };
    let (session, secret, tunnel) = host.connect(&profile, secret).await?;
    let id = uuid::Uuid::new_v4().to_string();
    host.sessions
        .lock()
        .map_err(|_| DbError::internal("Session lock unavailable"))?
        .insert(
            id.clone(),
            Arc::new(SessionRecord {
                session: Arc::from(session),
                profile,
                secret,
                _tunnel: tunnel,
                dead: AtomicBool::new(false),
            }),
        );
    Ok(id)
}
#[tauri::command]
async fn session_close(host: tauri::State<'_, Host>, session_id: String) -> CommandResult<()> {
    host.agent_results.clear_session(&session_id);
    let record = host
        .sessions
        .lock()
        .map_err(|_| DbError::internal("Session lock unavailable"))?
        .remove(&session_id);
    if let Some(record) = record {
        record.dead.store(true, Ordering::Release);
        if let Some(tunnel) = &record._tunnel { tunnel.close(); }
        host.network.emit(net::Event::Network { profile_id:record.profile.id.clone(), state:"disconnected".into(), attempt:0, code:None });
        {
            let runs = host
                .runs
                .lock()
                .map_err(|_| DbError::internal("Run lock unavailable"))?;
            for run in runs.values().filter(|r| r.session_id == session_id) {
                run.token.cancel();
            }
        }
        record.session.close().await;
    }
    if let Ok(mut contexts) = host.context_runs.lock() { contexts.retain(|r| r.session_id != session_id); }
    Ok(())
}
#[tauri::command]
async fn explorer_children(
    host: tauri::State<'_, Host>,
    session_id: String,
    parent: Option<String>,
) -> CommandResult<Vec<ExplorerNode>> {
    let record = host.session(&session_id)?;
    host.observe(
        &record,
        record.session.list_children(parent.as_deref()).await,
    )
}
#[tauri::command]
async fn schema_graph(host: tauri::State<'_, Host>, session_id: String, scope: Option<String>) -> CommandResult<SchemaGraph> {
    let record=host.session(&session_id)?;
    host.observe(&record,record.session.schema_graph(scope.as_deref()).await)
}
#[tauri::command]
async fn browse(
    host: tauri::State<'_, Host>,
    session_id: String,
    node_id: String,
    offset: u64,
    limit: u32,
    run_id: String,
    filter: Option<Vec<(String, Value)>>,
) -> CommandResult<ResultSet> {
    let record = host.session(&session_id)?;
    if filter.as_ref().is_some_and(|f|!f.is_empty()) && !host.driver(&record.profile.driver_id)?.manifest().capabilities.filtered_browse { return Err(DbError::unsupported("Filtered browse is not supported")); }
    let run = host.run(&session_id, &run_id)?;
    let future = record.session.browse(
        &node_id,
        PageRequest {
            filter: filter.unwrap_or_default(),
            offset,
            limit: clamp_row_limit(limit),
        },
    );
    tokio::pin!(future);
    let result = tokio::select! {
        result=&mut future=>result,
        _=run.token.cancelled()=>{
            // Session::browse has no cancel argument; closing the transport stops server work.
            record.dead.store(true,Ordering::Release);record.session.close().await;
            let _=future.await;Err(DbError::new(ErrorKind::Cancelled,"데이터 보기가 취소되었습니다. 다시 연결하세요"))
        }
    };
    let result = host.observe(&record, result);
    context::remember(&host, &session_id, &run_id, "", result.as_ref().map(std::slice::from_ref));
    result
}
#[tauri::command]
async fn execute(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: ExecRequest,
) -> CommandResult<Vec<ResultSet>> {
    execute_impl(&host, session_id, run_id, req, None, HistorySource::Ui).await
}
#[tauri::command]
async fn execute_stream(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: ExecRequest,
    on_event: tauri::ipc::Channel<tauri::ipc::Response>,
) -> CommandResult<Vec<ResultSet>> {
    execute_impl(
        &host,
        session_id,
        run_id,
        req,
        Some(on_event),
        HistorySource::Ui,
    )
    .await
}
#[tauri::command]
async fn execute_agent(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: ExecRequest,
) -> CommandResult<Vec<ResultSet>> {
    if !req.read_only {
        let approval=host.approvals.lock().map_err(|_|DbError::internal("Approval lock unavailable"))?.remove(&run_id);
        if !approval.is_some_and(|a|a.created.elapsed().as_secs()<60 && permissions::fingerprint(&session_id,&req).is_ok_and(|f|f==a.fingerprint)) { return Err(DbError::new(ErrorKind::Auth,"앱에서 승인해야 합니다")); }
    }
    execute_impl(&host, session_id, run_id, req, None, HistorySource::Agent).await
}
async fn execute_impl(
    host: &Host,
    session_id: String,
    run_id: String,
    mut req: ExecRequest,
    channel: Option<tauri::ipc::Channel<tauri::ipc::Response>>,
    source: HistorySource,
) -> CommandResult<Vec<ResultSet>> {
    let record = host.session(&session_id)?;
    let run = host.run(&session_id, &run_id)?;
    if host.access_mode(&record)? == permissions::AccessMode::ReadOnly {
        if !host.driver(&record.profile.driver_id)?.manifest().capabilities.read_only_exec { return Err(DbError::unsupported("이 드라이버는 읽기 전용 실행을 강제할 수 없습니다")); }
        req.read_only = true;
    }
    if req.read_only && !host.driver(&record.profile.driver_id)?.manifest().capabilities.read_only_exec { return Err(DbError::unsupported("이 드라이버는 읽기 전용 실행을 강제할 수 없습니다")); }
    req.row_limit = clamp_row_limit(req.row_limit);
    let started = Instant::now();
    let live = channel.as_ref().map(|channel| stream::LiveResults::new(channel.clone(), run.token.clone(), req.row_limit));
    let future = async {
        if let Some(sink) = live.as_ref() {
            record.session.execute_stream(&req, run.token.clone(), sink).await
        } else {
            record.session.execute(&req, run.token.clone()).await
        }
    };
    tokio::pin!(future);
    let result =
        match tokio::time::timeout(Duration::from_millis(req.timeout_ms.max(1)), &mut future).await
        {
            Ok(result) => result,
            Err(_) => {
                run.token.cancel();
                let _ = future.await;
                Err(DbError::new(ErrorKind::Timeout, "실행 시간이 초과되었습니다"))
            }
        };
    let result = host.observe(&record, result);
    context::remember(host, &session_id, &run_id, &req.text, result.as_deref());
    let entry = HistoryEntry {
        kind: "execute".into(),
        source,
        timestamp: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        profile_id: Some(record.profile.id.clone()),
        profile_name: storage::redact(&record.profile.name, record.secret.as_deref()),
        text: storage::redact(&req.text, record.secret.as_deref()),
        elapsed_ms: started.elapsed().as_millis() as u64,
        row_count: result
            .as_ref()
            .map(|r| r.iter().map(|r| r.row_count).sum())
            .unwrap_or(0),
        error: result
            .as_ref()
            .err()
            .map(|e| storage::redact(&e.message, record.secret.as_deref())),
    };
    if host.history(entry).await.is_err() {
        tracing::error!("Could not save query history");
    }
    if let Some(channel) = channel {
        let results = result?;
        if let Some(sink) = live.as_ref().filter(|sink|sink.started() || results.is_empty()) {
            sink.finish()?;
            return Ok(vec![]);
        }
        // WKWebView Channel delivery regressed graph/small results; preserve direct invoke for those measured cases.
        if results.iter().any(|r| r.graph.is_some()) || results.iter().all(|r| r.table.as_ref().is_none_or(|t| t.rows.len() <= 2000)) {
            return Ok(results);
        }
        stream::send_results(results, &channel, &run.token).await?;
        Ok(vec![])
    } else {
        result
    }
}
#[tauri::command]
async fn preview(host: tauri::State<'_, Host>, session_id: String, run_id: String, mut req: ExecRequest, source: Option<HistorySource>) -> CommandResult<ChangePreview> {
    let record = host.session(&session_id)?;
    if host.access_mode(&record)? == permissions::AccessMode::ReadOnly { return Err(DbError::new(ErrorKind::Auth,"읽기 전용 연결에서는 미리보기를 할 수 없습니다")); }
    let run = host.run(&session_id, &run_id)?;
    req.preview_sample_limit = req.preview_sample_limit.min(1000);
    let started = Instant::now();
    let future = record.session.preview(&req, run.token.clone());
    tokio::pin!(future);
    let result = match tokio::time::timeout(Duration::from_millis(req.timeout_ms.max(1)), &mut future).await {
        Ok(r) => r,
        Err(_) => { run.token.cancel(); let _ = future.await; Err(DbError::new(ErrorKind::Timeout, "미리보기 시간이 초과되었습니다")) }
    };
    let result = host.observe(&record, result);
    let entry = HistoryEntry {
        kind: "preview".into(), source: source.unwrap_or_default(),
        timestamp: time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
        profile_id: Some(record.profile.id.clone()),
        profile_name: storage::redact(&record.profile.name, record.secret.as_deref()),
        text: storage::redact(&req.text, record.secret.as_deref()),
        elapsed_ms: started.elapsed().as_millis() as u64,
        row_count: result.as_ref().map(|r| r.statements.iter().map(|s| s.affected.unwrap_or(0)).sum()).unwrap_or(0),
        error: result.as_ref().err().map(|e| storage::redact(&e.message, record.secret.as_deref())),
    };
    if host.history(entry).await.is_err() { tracing::error!("Could not save preview history"); }
    if result.is_ok() {
        let fingerprint=permissions::fingerprint(&session_id,&req)?;
        let mut previews=host.previews.lock().map_err(|_|DbError::internal("Preview lock unavailable"))?;
        previews.retain(|_,p|p.created.elapsed().as_secs()<60);
        if previews.len()<64 {previews.insert(fingerprint.clone(),permissions::Approval{fingerprint,created:Instant::now()});}
    }
    result
}
#[tauri::command]
async fn cancel(host: tauri::State<'_, Host>, run_id: String) -> CommandResult<()> {
    if let Some(run) = host
        .runs
        .lock()
        .map_err(|_| DbError::internal("Run lock unavailable"))?
        .get(&run_id)
    {
        run.token.cancel();
    }
    Ok(())
}
#[tauri::command]
async fn expand(
    host: tauri::State<'_, Host>,
    session_id: String,
    element_id: String,
    limit: u32,
) -> CommandResult<ResultSet> {
    let record = host.session(&session_id)?;
    host.observe(
        &record,
        record
            .session
            .expand(&element_id, limit.clamp(1, 100_000))
            .await,
    )
}
#[tauri::command]
async fn history_list(
    host: tauri::State<'_, Host>,
    limit: usize,
) -> CommandResult<Vec<HistoryEntry>> {
    let _guard = host.storage.lock().await;
    let mut entries = storage::read_history(&host.paths.data.join("history.jsonl"))?;
    entries.reverse();
    entries.truncate(limit.min(1000));
    Ok(entries)
}

#[tauri::command]
async fn history_delete(host: tauri::State<'_, Host>, timestamp: String, profile_name: String, text: String) -> CommandResult<()> {
    let _guard = host.storage.lock().await;
    storage::delete_history(&host.paths.data.join("history.jsonl"), &timestamp, &profile_name, &text)
}

fn file_logger(log: std::fs::File) -> impl tracing::Subscriber + Send + Sync {
    tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(log))
            // Driver libraries can log server NOTICE messages containing user SQL or secrets.
            .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.target() == "axdb" || metadata.target().starts_with("axdb::")
            })),
    )
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::Builder::new().build())
        .manage(recycle::Recycler::default())
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::Focused(focused) = event { recycle::focus(window.app_handle(), *focused); }
            }
        })
        .manage(export::Exports::default())
        .manage(driver_install::Installer::default())
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                if let (Some(bridge), Some(host)) = (webview.try_state::<agent_bridge::Bridge>(), webview.try_state::<Host>()) {
                    bridge.loading(&host.paths.config);
                }
            }
        })
        .setup(|app| {
            let paths = Paths::create(
                app.path().app_config_dir()?,
                app.path().app_data_dir()?,
                app.path().app_log_dir()?,
            )?;
            let log = storage::open_private_log(&paths.log.join("axdb.log"))?;
            file_logger(log)
                .try_init()
                .map_err(|_| "Cannot initialize file logging")?;
            let config = paths.config.clone();
            let enabled = agent_bridge::Bridge::configured(&config)?;
            let host = Host::new(paths);
            let network_handle = app.handle().clone();
            host.network.set_sink(Arc::new(move |event| {
                use tauri::Emitter;
                let _ = network_handle.emit("connection-network", event);
            }));
            app.manage(host);
            app.manage(agent_bridge::Bridge::default());
            if enabled {
                let handle = app.handle().clone();
                tauri::async_runtime::block_on(
                    app.state::<agent_bridge::Bridge>().start(handle, &config),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            recycle::recycle_settings,
            recycle::recycle_large,
            recycle::recycle_restore,
            recycle::recycle_ready,
            recycle::recycle_commit,
            net::connection_parse,
            net::connection_test_steps,
            net::connection_host_key_confirm,
            net::connection_credentials_save,
            net::iam_profiles,
            export::export_save_dialog,
            export::export_write,
            export::export_reveal,
            updater::updater_settings,
            updater::updater_configure,
            onboarding::onboarding_status,
            onboarding::onboarding_complete,
            onboarding::onboarding_mcp_config,
            context::context_get,
            context::context_error,
            assistant::ai_settings,
            assistant::ai_configure,
            assistant::ai_request,
            assistant::ai_login_start,
            assistant::ai_login_submit,
            assistant::ai_login_cancel,
            assistant::ai_login_poll,
            assistant::ai_login_status,
            assistant::ai_logout,
            assistant::provider::ai_provider_login_start,
            assistant::provider::ai_provider_login_poll,
            assistant::provider::ai_provider_login_cancel,
            assistant::provider::ai_provider_logout,
            assistant::provider::ai_models,
            assistant::oauth::ai_account_select,
            assistant::operation::ai_operation,
            assistant::operation::ai_diagnose_connection,
            drivers_list,
            driver_install::drivers_catalog,
            driver_install::driver_install,
            driver_install::driver_update,
            driver_install::driver_remove,
            driver_install::driver_cancel,
            driver_install::driver_pick_file,
            profiles_list,
            profile_save,
            profile_delete,
            connection_test,
            session_open,
            session_info,
            session_close,
            star::star_prompt,
            star::star_count,
            explorer_children,
            schema_graph,
            browse,
            execute,
            execute_agent,
            preview,
            agent_bridge::agent_bridge_set,
            agent_bridge::agent_bridge_status,
            agent_bridge::agent_bridge_reply,
            agent_bridge::agent_bridge_ready,
            execute_stream,
            viewer_bench::viewer_bench_enabled,
            viewer_bench::viewer_bench_target,
            viewer_bench::viewer_bench_load,
            viewer_bench::viewer_bench_config,
            viewer_bench::viewer_bench_write,
            viewer_bench::viewer_bench_checkpoint,
            editor::explain_query,
            editor::lint_query,
            editor::refactor_query,
            management::administration::files::backup_create_file, management::administration::files::restore_options, management::administration::files::restore_pick_file, management::administration::files::restore_file_preview, management::administration::files::restore_file_apply, management::administration::files::restore_file_discard,
            management::administration::backup_status, management::administration::backup_create, management::administration::restore_preview, management::administration::restore_apply,
            management::administration::roles, management::administration::plan_grants, management::administration::grants_preview, management::administration::grants_apply,
            management::import_plan,
            management::import_target,
            management::import_pick_file,
            management::import_preview,
            management::import_apply,
            management::schema_snapshot,
            management::schema_diff,
            management::ddl_render,
            management::snapshot_encode,
            management::snapshot_decode,
            management::migration::migration_scan,
            management::migration::migration_preview,
            management::migration::migration_apply,
            edits::edit_preview,
            edits::edit_apply,
            cancel,
            expand,
            history_list,
            history_delete,
            permissions::agent_clients_list,
            permissions::agent_client_save,
            permissions::agent_client_rotate,
            permissions::agent_client_revoke,
            permissions::agent_audit_list,
            permissions::agent_approve_write
        ])
        .build(tauri::generate_context!())
        .expect("Failed to build axdb")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                agent_bridge::cleanup(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    struct WaitingSession {
        stopped: AtomicBool,
    }
    #[async_trait]
    impl Session for WaitingSession {
        async fn preview(&self, _: &ExecRequest, cancel: CancelToken) -> CommandResult<ChangePreview> {
            cancel.cancelled().await; self.stopped.store(true, Ordering::Release);
            Err(DbError::new(ErrorKind::Cancelled, "cancelled"))
        }
        async fn ping(&self) -> CommandResult<()> {
            Ok(())
        }
        async fn server_info(&self) -> CommandResult<String> {
            Ok("test".into())
        }
        async fn list_children(&self, _: Option<&str>) -> CommandResult<Vec<ExplorerNode>> {
            Ok(Vec::new())
        }
        async fn browse(&self, _: &str, _: PageRequest) -> CommandResult<ResultSet> {
            Err(DbError::unsupported("test"))
        }
        async fn execute(
            &self,
            _: &ExecRequest,
            cancel: CancelToken,
        ) -> CommandResult<Vec<ResultSet>> {
            cancel.cancelled().await;
            self.stopped.store(true, Ordering::Release);
            Err(DbError::new(ErrorKind::Cancelled, "cancelled"))
        }
        async fn expand(&self, _: &str, _: u32) -> CommandResult<ResultSet> {
            Err(DbError::unsupported("test"))
        }
        async fn close(&self) {}
    }
    fn setup() -> (tauri::App<tauri::test::MockRuntime>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("axdb-host-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Construct paths directly so a user's AXDB_CONFIG_DIR cannot redirect test writes.
        let host = Host::new(Paths {
            config: dir.clone(),
            data: dir.clone(),
            log: dir.clone(),
        });
        let app = tauri::test::mock_builder()
            .manage(host)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        (app, dir)
    }
    fn request(text: &str, timeout_ms: u64) -> ExecRequest {
        ExecRequest {
            read_only: false,
            text: text.into(),
            params: Default::default(),
            row_limit: 1000,
            graph_node_limit: 300,
            timeout_ms,
            preview_sample_limit: default_preview_sample_limit(),
        }
    }
    #[test]
    fn file_logging_excludes_database_notices() {
        let path = std::env::temp_dir().join(format!("axdb-log-{}", uuid::Uuid::new_v4()));
        let log = std::fs::File::create(&path).unwrap();
        tracing::subscriber::with_default(file_logger(log), || {
            tracing::info!(target:"tokio_postgres::connection", "server notice containing a private-password");
            tracing::info!(target:"axdb", "safe connection field names");
        });
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("safe connection"));
        assert!(!text.contains("private-password"));
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn timeout_waits_for_cancel_and_redacts_history() {
        let (app, dir) = setup();
        let session = Arc::new(WaitingSession {
            stopped: AtomicBool::new(false),
        });
        let profile = Profile {
            id: "p".into(),
            name: "test".into(),
            driver_id: "postgres".into(),
            fields: Default::default(),
            save_secret: false, network: None, auth: Default::default(),
            access_mode: Default::default(),
        };
        app.state::<Host>().sessions.lock().unwrap().insert(
            "s".into(),
            Arc::new(SessionRecord {
                session: session.clone(),
                profile,
                secret: Some("history-secret".into()),
                _tunnel: None,
                dead: AtomicBool::new(false),
            }),
        );
        let mut req = request("SELECT 'history-secret'", 20);
        req.params.insert(
            "sensitive-param".into(),
            serde_json::json!("parameter-value"),
        );
        let error = execute(app.state(), "s".into(), "run".into(), req)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        assert!(session.stopped.load(Ordering::Acquire));
        assert!(app.state::<Host>().runs.lock().unwrap().is_empty());
        let history = history_list(app.state(), 100).await.unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].text.contains("***"));
        let text = std::fs::read_to_string(dir.join("history.jsonl")).unwrap();
        assert!(!text.contains("history-secret"));
        assert!(!text.contains("parameter-value"));
        session.stopped.store(false, Ordering::Release);
        let error = preview(app.state(), "s".into(), "preview".into(), request("SELECT 'history-secret'",20), Some(HistorySource::Agent)).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout); assert!(session.stopped.load(Ordering::Acquire));
        assert!(app.state::<Host>().runs.lock().unwrap().is_empty());
        let previews=history_list(app.state(),100).await.unwrap();
        assert!(previews.iter().any(|h| h.kind=="preview" && matches!(h.source,HistorySource::Agent) && h.text.contains("***")));
        // The history file is capped even after a long-running session.
        let entry = history[0].clone();
        let mut data = Vec::new();
        for _ in 0..1001 {
            serde_json::to_writer(&mut data, &entry).unwrap();
            data.push(b'\n');
        }
        storage::write_private(&dir.join("history.jsonl"), &data).unwrap();
        app.state::<Host>().history(entry).await.unwrap();
        assert_eq!(
            storage::read_history(&dir.join("history.jsonl"))
                .unwrap()
                .len(),
            1000
        );
        let close_host=app.state::<Host>();
        let close_run=close_host.run("s", "close-test").unwrap();
        session_close(app.state(), "s".into()).await.unwrap();
        assert!(close_run.token.is_cancelled());
        assert!(app.state::<Host>().session("s").is_err());
        session_close(app.state(), "s".into()).await.unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn real_host_commands() {
        if std::env::var("AXDB_TEST_PG").is_err() {
            eprintln!("AXDB_TEST_PG unset: live host commands not run");
            return;
        }
        let (app, dir) = setup();
        let profile = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "live-test".into(),
            driver_id: "postgres".into(),
            fields: [
                ("host".into(), "127.0.0.1".into()),
                ("port".into(), "55432".into()),
                ("database".into(), "axdb_test".into()),
                ("user".into(), "postgres".into()),
            ]
            .into(),
            save_secret: false, network: None, auth: Default::default(),
            access_mode: Default::default(),
        };
        // OS keychain interaction is deliberately separate from live DB command verification.
        storage::write_private(
            &dir.join("profiles.json"),
            &serde_json::to_vec(&vec![profile.clone()]).unwrap(),
        )
        .unwrap();
        assert!(drivers_list(app.state())
            .await
            .unwrap()
            .iter()
            .any(|d| d.id == "postgres"));
        assert_eq!(profiles_list(app.state()).await.unwrap().len(), 1);
        assert!(
            connection_test(app.state(), profile.clone(), Some("testpw".into()))
                .await
                .unwrap()
                .contains("PostgreSQL")
        );
        let id = session_open(app.state(), profile.id, Some("testpw".into()))
            .await
            .unwrap();
        let result = execute(
            app.state(),
            id.clone(),
            "select".into(),
            request("SELECT 9223372036854775807::int8", 1000),
        )
        .await
        .unwrap();
        assert_eq!(
            result[0].table.as_ref().unwrap().rows[0][0],
            Value::Int("9223372036854775807".into())
        );
        let changes=preview(app.state(),id.clone(),"preview-ddl".into(),request("CREATE TEMP TABLE axdb_host_preview(id integer)",1000),None).await.unwrap();
        assert!(!changes.committed);
        assert!(changes.statements[0].schema_diff.as_ref().unwrap().added.iter().any(|name| name.contains("axdb_host_preview")));
        let absent=execute(app.state(),id.clone(),"preview-check".into(),request("SELECT to_regclass('pg_temp.axdb_host_preview')",1000)).await.unwrap();
        assert_eq!(absent[0].table.as_ref().unwrap().rows[0][0],Value::Null);
        let error = execute(
            app.state(),
            id.clone(),
            "timeout".into(),
            request("SELECT pg_sleep(10)", 100),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        execute(
            app.state(),
            id.clone(),
            "after-timeout".into(),
            request("SELECT 1", 1000),
        )
        .await
        .unwrap();
        let run = execute(
            app.state(),
            id.clone(),
            "cancel".into(),
            request("SELECT pg_sleep(10)", 5000),
        );
        let stop = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel(app.state(), "cancel".into()).await.unwrap();
        };
        let (result, _) = tokio::join!(run, stop);
        assert_eq!(result.unwrap_err().kind, ErrorKind::Cancelled);
        execute(
            app.state(),
            id.clone(),
            "after-cancel".into(),
            request("SELECT 1", 1000),
        )
        .await
        .unwrap();
        assert!(!explorer_children(app.state(), id.clone(), None)
            .await
            .unwrap()
            .is_empty());
        session_close(app.state(), id.clone()).await.unwrap();
        assert_eq!(
            execute(app.state(), id, "closed".into(), request("SELECT 1", 100))
                .await
                .unwrap_err()
                .kind,
            ErrorKind::Connection
        );
        let history = std::fs::read_to_string(dir.join("history.jsonl")).unwrap();
        assert!(!history.contains("testpw"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn read_only_mode_updates_open_sessions_and_approvals_are_bound_and_single_use() {
        let (app,dir)=setup();
        let profile=Profile{id:"readonly".into(),name:"test".into(),driver_id:"sqlite".into(),fields:std::collections::BTreeMap::from([("path".into(),dir.join("test.sqlite").to_string_lossy().to_string())]),save_secret:false,network:None,auth:Default::default(),access_mode:Default::default()};
        storage::write_private(&dir.join("profiles.json"),&serde_json::to_vec(&vec![profile.clone()]).unwrap()).unwrap();
        let session=session_open(app.state(),profile.id.clone(),None).await.unwrap();
        execute(app.state(),session.clone(),"setup".into(),request("CREATE TABLE items(id integer); INSERT INTO items VALUES(1)",1000)).await.unwrap();
        let write=request("DELETE FROM items",1000);
        assert!(execute_agent(app.state(),session.clone(),"unapproved".into(),write.clone()).await.is_err());
        assert!(permissions::agent_approve_write(app.state(),session.clone(),"missing-preview".into(),write.clone()).is_err());
        preview(app.state(),session.clone(),"preview".into(),write.clone(),Some(HistorySource::Agent)).await.unwrap();
        let mut changed=write.clone();changed.params.insert("modified".into(),serde_json::json!(true));
        assert!(permissions::agent_approve_write(app.state(),session.clone(),"changed".into(),changed).is_err());
        permissions::agent_approve_write(app.state(),session.clone(),"approved".into(),write.clone()).unwrap();
        execute_agent(app.state(),session.clone(),"approved".into(),write.clone()).await.unwrap();
        assert!(execute_agent(app.state(),session.clone(),"approved".into(),write.clone()).await.is_err());
        let mut profile=profile;profile.access_mode=permissions::AccessMode::ReadOnly;
        storage::write_private(&dir.join("profiles.json"),&serde_json::to_vec(&vec![profile]).unwrap()).unwrap();
        let err=execute(app.state(),session.clone(),"ui-write".into(),request("INSERT INTO items VALUES(2)",1000)).await.unwrap_err();assert_eq!(err.kind,ErrorKind::Query);
        assert!(preview(app.state(),session.clone(),"no-preview".into(),write,None).await.is_err());
        let result=execute(app.state(),session.clone(),"count".into(),request("SELECT count(*) FROM items",1000)).await.unwrap();assert_eq!(result[0].table.as_ref().unwrap().rows[0][0],Value::Int("0".into()));
        session_close(app.state(),session).await.unwrap();std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn client_crud_rotation_and_private_audit_commands() {
        let (app,dir)=setup();
        app.manage(agent_bridge::Bridge::default());
        let input=permissions::ClientInput{id:None,name:"scoped".into(),connections:Default::default(),allowed_tools:vec!["list_connections".into()],max_rows:3,max_result_rows:200,max_result_bytes:262144,expires_at:None};
        let created=permissions::agent_client_save(app.state(),input).unwrap();let raw=created["token"].as_str().unwrap();let id=created["client"]["id"].as_str().unwrap().to_owned();
        assert!(permissions::authenticate(&dir,raw).unwrap().is_some());
        let listed=serde_json::to_string(&permissions::agent_clients_list(app.state()).unwrap()).unwrap();assert!(!listed.contains(raw)&&!listed.contains("token_hash")&&!listed.contains("token_salt"));
        let rotated=permissions::agent_client_rotate(app.state(),app.state(),id.clone()).await.unwrap();assert!(permissions::authenticate(&dir,raw).unwrap().is_none());
        let client=permissions::authenticate(&dir,rotated["token"].as_str().unwrap()).unwrap().unwrap();
        for _ in 0..55 {permissions::audit(&app.state::<Host>(),&client,"run_query",None,"denied",Some("SELECT 'private-statement'")).unwrap();}
        permissions::audit(&app.state::<Host>(),&client,"private-tool-secret",Some("private-profile-secret"),"denied",None).unwrap();
        let events=permissions::agent_audit_list(app.state()).unwrap();assert_eq!(events.len(),50);let log=std::fs::read_to_string(dir.join("agent-audit.jsonl")).unwrap();assert!(!log.contains("private-statement")&&!log.contains(raw)&&!log.contains("private-tool-secret")&&!log.contains("private-profile-secret"));
        permissions::agent_client_revoke(app.state(),id).unwrap();assert!(permissions::authenticate(&dir,rotated["token"].as_str().unwrap()).unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

}
