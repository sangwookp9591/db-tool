mod agent_bridge;
pub mod drivers;
pub mod storage;
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
    dead: AtomicBool,
}
struct RunRecord {
    token: CancelToken,
    session_id: String,
}
pub struct Host {
    registry: Registry,
    paths: Paths,
    sessions: Mutex<HashMap<String, Arc<SessionRecord>>>,
    runs: Mutex<HashMap<String, RunRecord>>,
    storage: tokio::sync::Mutex<()>,
}
impl Host {
    pub fn new(paths: Paths) -> Self {
        Self {
            registry: drivers::registry(),
            paths,
            sessions: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
            storage: tokio::sync::Mutex::new(()),
        }
    }
    fn session(&self, id: &str) -> Result<Arc<SessionRecord>, DbError> {
        let record = self
            .sessions
            .lock()
            .map_err(|_| DbError::internal("Session lock unavailable"))?
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "Session not found"))?;
        if record.dead.load(Ordering::Acquire) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "Session disconnected; reconnect required",
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
    ) -> Result<(Box<dyn Session>, Option<String>), DbError> {
        let driver = self.driver(&profile.driver_id)?;
        let mut safe = profile.clone();
        let inline = storage::strip_secrets(&mut safe, driver.manifest());
        let secret = match secret.or(inline) {
            Some(s) => Some(s),
            None if safe.save_secret => storage::keychain_get(&profile.id)?,
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
        let session = driver
            .connect(&ConnectConfig {
                fields: safe.fields,
                secret: secret.clone(),
                connect_timeout_ms: 5000,
            })
            .await?;
        Ok((session, secret))
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
    let driver = host.driver(&profile.driver_id)?;
    let secret = secret.or(storage::strip_secrets(&mut profile, driver.manifest()));
    if profile.id.is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    if profile.name.trim().is_empty() {
        return Err(DbError::query("Profile name is required"));
    }
    let _guard = host.storage.lock().await;
    let path = host.paths.config.join("profiles.json");
    let mut profiles = storage::read_profiles(&path)?;
    if profile.save_secret {
        if let Some(s) = secret.as_deref() {
            storage::keychain_set(&profile.id, Some(s))?;
        }
    } else {
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
    let _guard = host.storage.lock().await;
    let path = host.paths.config.join("profiles.json");
    let mut profiles = storage::read_profiles(&path)?;
    storage::keychain_set(&profile_id, None)?;
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
    let (session, _) = host.connect(&profile, secret).await?;
    let result = async {
        session.ping().await?;
        session.server_info().await
    }
    .await;
    session.close().await;
    result
}
#[tauri::command]
async fn session_open(
    host: tauri::State<'_, Host>,
    profile_id: String,
    secret: Option<String>,
) -> CommandResult<String> {
    let profile = {
        let _guard = host.storage.lock().await;
        storage::read_profiles(&host.paths.config.join("profiles.json"))?
            .into_iter()
            .find(|p| p.id == profile_id)
            .ok_or_else(|| DbError::query("Profile not found"))?
    };
    let (session, secret) = host.connect(&profile, secret).await?;
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
                dead: AtomicBool::new(false),
            }),
        );
    Ok(id)
}
#[tauri::command]
async fn session_close(host: tauri::State<'_, Host>, session_id: String) -> CommandResult<()> {
    let record = host
        .sessions
        .lock()
        .map_err(|_| DbError::internal("Session lock unavailable"))?
        .remove(&session_id);
    if let Some(record) = record {
        record.dead.store(true, Ordering::Release);
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
async fn browse(
    host: tauri::State<'_, Host>,
    session_id: String,
    node_id: String,
    offset: u64,
    limit: u32,
    run_id: String,
) -> CommandResult<ResultSet> {
    let record = host.session(&session_id)?;
    let run = host.run(&session_id, &run_id)?;
    let future = record.session.browse(
        &node_id,
        PageRequest {
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
            let _=future.await;Err(DbError::new(ErrorKind::Cancelled,"Browse cancelled; reconnect required"))
        }
    };
    host.observe(&record, result)
}
#[tauri::command]
async fn execute(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: ExecRequest,
) -> CommandResult<Vec<ResultSet>> {
    execute_impl(host, session_id, run_id, req, None, HistorySource::Ui).await
}
#[tauri::command]
async fn execute_stream(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: ExecRequest,
    on_event: tauri::ipc::Channel<stream::StreamEvent>,
) -> CommandResult<Vec<ResultSet>> {
    execute_impl(
        host,
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
    execute_impl(host, session_id, run_id, req, None, HistorySource::Agent).await
}
async fn execute_impl(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    mut req: ExecRequest,
    channel: Option<tauri::ipc::Channel<stream::StreamEvent>>,
    source: HistorySource,
) -> CommandResult<Vec<ResultSet>> {
    let record = host.session(&session_id)?;
    let run = host.run(&session_id, &run_id)?;
    req.row_limit = clamp_row_limit(req.row_limit);
    let started = Instant::now();
    let future = record.session.execute(&req, run.token.clone());
    tokio::pin!(future);
    let result =
        match tokio::time::timeout(Duration::from_millis(req.timeout_ms.max(1)), &mut future).await
        {
            Ok(result) => result,
            Err(_) => {
                run.token.cancel();
                let _ = future.await;
                Err(DbError::new(ErrorKind::Timeout, "Execution timed out"))
            }
        };
    let result = host.observe(&record, result);
    let entry = HistoryEntry {
        source,
        timestamp: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
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
    // shortcut: drivers collect before host chunking; add a driver sink when database latency dominates first paint.
    if let Some(channel) = channel {
        let results = result?;
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

fn file_logger(log: std::fs::File) -> impl tracing::Subscriber + Send + Sync {
    tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(log))
            // Driver libraries can log server NOTICE messages containing user SQL or secrets.
            .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.target() == "db_tool" || metadata.target().starts_with("db_tool::")
            })),
    )
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let paths = Paths::create(
                app.path().app_config_dir()?,
                app.path().app_data_dir()?,
                app.path().app_log_dir()?,
            )?;
            let log = storage::open_private_log(&paths.log.join("db-tool.log"))?;
            file_logger(log)
                .try_init()
                .map_err(|_| "Cannot initialize file logging")?;
            let config = paths.config.clone();
            let enabled = agent_bridge::Bridge::configured(&config)?;
            if viewer_bench::viewer_bench_enabled() {
                if let Some(window) = app.get_webview_window("main") { window.set_always_on_top(true)?; window.set_focus()?; }
            }
            app.manage(Host::new(paths));
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
            drivers_list,
            profiles_list,
            profile_save,
            profile_delete,
            connection_test,
            session_open,
            session_close,
            explorer_children,
            browse,
            execute,
            execute_agent,
            agent_bridge::agent_bridge_set,
            agent_bridge::agent_bridge_status,
            agent_bridge::agent_bridge_reply,
            execute_stream,
            viewer_bench::viewer_bench_enabled,
            viewer_bench::viewer_bench_target,
            viewer_bench::viewer_bench_write,
            cancel,
            expand,
            history_list
        ])
        .build(tauri::generate_context!())
        .expect("Failed to build db-tool")
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
        let dir = std::env::temp_dir().join(format!("dbtool-host-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Construct paths directly so a user's DBTOOL_CONFIG_DIR cannot redirect test writes.
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
        }
    }
    #[test]
    fn file_logging_excludes_database_notices() {
        let path = std::env::temp_dir().join(format!("dbtool-log-{}", uuid::Uuid::new_v4()));
        let log = std::fs::File::create(&path).unwrap();
        tracing::subscriber::with_default(file_logger(log), || {
            tracing::info!(target:"tokio_postgres::connection", "server notice containing a private-password");
            tracing::info!(target:"db_tool", "safe connection field names");
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
            save_secret: false,
        };
        app.state::<Host>().sessions.lock().unwrap().insert(
            "s".into(),
            Arc::new(SessionRecord {
                session: session.clone(),
                profile,
                secret: Some("history-secret".into()),
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
        session_close(app.state(), "s".into()).await.unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn real_host_commands() {
        if std::env::var("DBTOOL_TEST_PG").is_err() {
            eprintln!("DBTOOL_TEST_PG unset: live host commands not run");
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
                ("database".into(), "dbtool_test".into()),
                ("user".into(), "postgres".into()),
            ]
            .into(),
            save_secret: false,
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
}
