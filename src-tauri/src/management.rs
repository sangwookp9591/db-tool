//! Management commands share driver edits, cancellation, preview receipts and history.
use super::*;
use serde::{Deserialize, Serialize};
use tauri_plugin_dialog::DialogExt;
mod import;
pub(crate) mod administration;
pub use administration::*;
pub(crate) mod migration;
pub use migration::*;

#[tauri::command]
pub fn import_plan(
    filename: String,
    bytes: Vec<u8>,
    node_id: String,
    mapping: Option<Vec<ImportMapping>>,
    create_table: Option<bool>,
) -> CommandResult<ImportPlan> {
    let mut plan = import::parse(&filename, &bytes, node_id, mapping)?;
    if create_table.unwrap_or(false) {
        plan.request.create_table = Some(
            plan.columns
                .iter()
                .map(|m| Column {
                    name: m.target.clone(),
                    type_name: m.type_name.clone(),
                })
                .collect(),
        );
    }
    Ok(plan)
}
#[tauri::command]
pub async fn import_target(
    host: tauri::State<'_, Host>,
    session_id: String,
    namespace: Option<String>,
    name: String,
) -> CommandResult<String> {
    if name.trim().is_empty() || name.len() > 255 || name.contains('\0') {
        return Err(DbError::query("New table name requires 1..255 bytes"));
    }
    let snapshot = schema_snapshot(host, session_id, namespace.clone()).await?;
    match snapshot.dialect.as_str() {
        "postgres" => serde_json::to_string(&[
            "table",
            namespace.as_deref().unwrap_or("public"),
            "tables",
            &name,
        ])
        .map_err(|_| DbError::query("Invalid target")),
        "sqlite" => Ok(format!("table:{name}")),
        _ => Err(DbError::unsupported(
            "New import tables require a SQL driver",
        )),
    }
}
#[derive(Serialize)]
pub struct ImportFile {
    filename: String,
    bytes: Vec<u8>,
}
#[tauri::command]
pub async fn import_pick_file(window: tauri::WebviewWindow) -> CommandResult<Option<ImportFile>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .add_filter("Import", &["csv", "tsv", "json", "ndjson", "jsonl", "xlsx"])
        .pick_file(move |file| {
            let _ = tx.send(file);
        });
    let file = rx
        .await
        .map_err(|_| DbError::internal("File dialog closed"))?;
    let Some(file) = file else { return Ok(None) };
    let path = file
        .into_path()
        .map_err(|_| DbError::query("Local file required"))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DbError::query("Invalid file name"))?
        .to_owned();
    let bytes = read_bounded(&path, IMPORT_MAX_BYTES)?;
    Ok(Some(ImportFile { filename, bytes }))
}
pub(super) fn read_bounded(path: &std::path::Path, limit: u64) -> CommandResult<Vec<u8>> {
    use std::io::Read;
    let meta = path
        .symlink_metadata()
        .map_err(|_| DbError::query("Cannot read selected file"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
        return Err(DbError::query("File is not regular or exceeds size limit"));
    }
    let mut data = vec![];
    std::fs::File::open(path)
        .map_err(|_| DbError::query("Cannot open selected file"))?
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|_| DbError::query("Cannot read selected file"))?;
    if data.len() as u64 > limit {
        return Err(DbError::query("File exceeds size limit"));
    }
    Ok(data)
}
#[derive(Clone, Serialize)]
pub struct ImportProgress {
    pub run_id: String,
    pub phase: String,
    pub completed: u64,
    pub total: u64,
}
#[tauri::command]
pub async fn import_preview(
    app: tauri::AppHandle,
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: EditRequest,
) -> CommandResult<ChangePreview> {
    match import_run(&host, session_id, run_id, req, true, Some(&app)).await? {
        ImportOutput::Preview(p) => Ok(p),
        _ => Err(DbError::internal("Invalid import output")),
    }
}
#[tauri::command]
pub async fn import_apply(
    app: tauri::AppHandle,
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: EditRequest,
) -> CommandResult<EditResult> {
    match import_run(&host, session_id, run_id, req, false, Some(&app)).await? {
        ImportOutput::Applied(p) => Ok(p),
        _ => Err(DbError::internal("Invalid import output")),
    }
}
async fn import_run(
    host: &Host,
    session_id: String,
    run_id: String,
    mut req: EditRequest,
    preview: bool,
    app: Option<&tauri::AppHandle>,
) -> CommandResult<ImportOutput> {
    let record = host.session(&session_id)?;
    if host.access_mode(&record)? == permissions::AccessMode::ReadOnly {
        return Err(DbError::new(ErrorKind::Auth, "Read-only connection"));
    }
    if !host
        .driver(&record.profile.driver_id)?
        .manifest()
        .capabilities
        .import_rows
    {
        return Err(DbError::unsupported(
            "Driver does not support atomic import",
        ));
    }
    if req.edits.is_empty()
        || req.edits.len() > IMPORT_MAX_ROWS
        || req
            .edits
            .iter()
            .any(|e| !matches!(e, RowEdit::Insert { .. }))
    {
        return Err(DbError::query("Import requires 1..10000 inserts"));
    }
    if serde_json::to_vec(&req)
        .map_err(|_| DbError::query("Invalid import request"))?
        .len()
        > 12 * 1024 * 1024
    {
        return Err(DbError::query("Import request exceeds transport limit"));
    }
    req.preview_sample_limit = 100;
    let fingerprint = permissions::fingerprint(&format!("import:{session_id}"), &req)?;
    receipt(host, &fingerprint, preview)?;
    let run = host.run(&session_id, &run_id)?;
    let notify = |phase: &str, completed: u64| {
        if let Some(app) = app {
            use tauri::Emitter;
            let _ = app.emit(
                "import-progress",
                ImportProgress {
                    run_id: run_id.clone(),
                    phase: phase.into(),
                    completed,
                    total: req.edits.len() as u64,
                },
            );
        }
    };
    notify(if preview { "preview" } else { "applying" }, 0);
    let started = Instant::now();
    let progress=ImportEvents {app,run_id:&run_id,preview};
    let future = record.session.import_rows_progress(&req, preview, run.token.clone(), &progress);
    tokio::pin!(future);
    let result = match tokio::time::timeout(
        Duration::from_millis(req.timeout_ms.clamp(1, 3600000)),
        &mut future,
    )
    .await
    {
        Ok(r) => r,
        Err(_) => {
            run.token.cancel();
            let _ = future.await;
            Err(DbError::new(ErrorKind::Timeout, "Import timed out"))
        }
    };
    let mut result = host.observe(&record, result);
    if let Ok(ImportOutput::Preview(p)) = &result
        && (p.committed || p.statements.is_empty())
    {
        return Err(DbError::query("Driver did not produce a rollback preview"));
    }
    if let Ok(ImportOutput::Preview(p)) = &mut result {
        let mut remaining = 100usize;
        for s in &mut p.statements {
            for t in [&mut s.before, &mut s.after].into_iter().flatten() {
                t.rows.truncate(remaining);
                remaining = remaining.saturating_sub(t.rows.len());
            }
        }
    }
    if result.is_ok() {
        if preview {
            remember(host, fingerprint)?;
        }
        notify(
            if preview {
                "preview_complete"
            } else {
                "committed"
            },
            req.edits.len() as u64,
        );
    } else {
        notify("failed", 0);
    }
    let _ = host
        .history(HistoryEntry {
            kind: if preview {
                "import_preview"
            } else {
                "import_apply"
            }
            .into(),
            source: HistorySource::default(),
            timestamp: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            profile_id: Some(record.profile.id.clone()),
            profile_name: storage::redact(&record.profile.name, record.secret.as_deref()),
            text: format!("Import {} rows (values omitted)", req.edits.len()),
            elapsed_ms: started.elapsed().as_millis() as u64,
            row_count: if result.is_ok() {
                req.edits.len() as u64
            } else {
                0
            },
            error: result
                .as_ref()
                .err()
                .map(|_| "Import failed; verify outcome if connection was lost".into()),
        })
        .await;
    result
}
fn receipt(host: &Host, fingerprint: &str, preview: bool) -> CommandResult<()> {
    let previous = host
        .previews
        .lock()
        .map_err(|_| DbError::internal("Preview lock unavailable"))?
        .remove(fingerprint);
    if !preview
        && !previous.is_some_and(|p| {
            p.created.elapsed() < Duration::from_secs(60) && p.fingerprint == fingerprint
        })
    {
        return Err(DbError::new(
            ErrorKind::Auth,
            "A matching preview within 60 seconds is required",
        ));
    }
    Ok(())
}
fn remember(host: &Host, fingerprint: String) -> CommandResult<()> {
    let mut receipts = host
        .previews
        .lock()
        .map_err(|_| DbError::internal("Preview lock unavailable"))?;
    receipts.retain(|_, p| p.created.elapsed() < Duration::from_secs(60));
    if receipts.len() >= 64 {
        return Err(DbError::query("Too many pending previews"));
    }
    receipts.insert(
        fingerprint.clone(),
        permissions::Approval {
            fingerprint,
            created: Instant::now(),
        },
    );
    Ok(())
}
#[tauri::command]
pub async fn schema_snapshot(
    host: tauri::State<'_, Host>,
    session_id: String,
    scope: Option<String>,
) -> CommandResult<SchemaSnapshot> {
    let r = host.session(&session_id)?;
    if !host
        .driver(&r.profile.driver_id)?
        .manifest()
        .capabilities
        .schema_snapshot
    {
        return Err(DbError::unsupported("Schema snapshot unsupported"));
    }
    host.observe(&r, r.session.schema_snapshot(scope.as_deref()).await)
}
#[tauri::command]
pub fn schema_diff(before: SchemaSnapshot, after: SchemaSnapshot) -> CommandResult<SnapshotDiff> {
    snapshot_diff(&before, &after)
}
#[tauri::command]
pub async fn ddl_render(
    host: tauri::State<'_, Host>,
    session_id: String,
    diff: SnapshotDiff,
) -> CommandResult<Vec<String>> {
    let r = host.session(&session_id)?;
    if !host
        .driver(&r.profile.driver_id)?
        .manifest()
        .capabilities
        .ddl_render
    {
        return Err(DbError::unsupported("DDL rendering unsupported"));
    }
    r.session.ddl_render(&diff).await
}
#[tauri::command]
pub fn snapshot_encode(mut snapshot: SchemaSnapshot) -> CommandResult<String> {
    snapshot.normalize()?;
    serde_json::to_string_pretty(&snapshot).map_err(|_| DbError::query("Invalid snapshot"))
}
#[tauri::command]
pub fn snapshot_decode(bytes: Vec<u8>) -> CommandResult<SchemaSnapshot> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(DbError::query("Snapshot exceeds 8 MiB"));
    }
    let mut s: SchemaSnapshot =
        serde_json::from_slice(&bytes).map_err(|_| DbError::query("Invalid snapshot"))?;
    s.normalize()?;
    Ok(s)
}

struct ImportEvents<'a>{app:Option<&'a tauri::AppHandle>,run_id:&'a str,preview:bool}
#[async_trait::async_trait]
impl ProgressSink for ImportEvents<'_>{
 async fn progress(&self,completed:u64,total:u64)->Result<(),DbError>{if let Some(app)=self.app{use tauri::Emitter;let _=app.emit("import-progress",ImportProgress{run_id:self.run_id.into(),phase:if self.preview{"preview"}else{"applying"}.into(),completed,total});}Ok(())}
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (Host, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::new(Paths {
            config: dir.path().into(),
            data: dir.path().into(),
            log: dir.path().into(),
        });
        host.registry
            .register(Arc::new(driver_sqlite::SqliteDriver::new()));
        let session = driver_sqlite::SqliteDriver::new()
            .connect(&ConnectConfig {
                fields: std::collections::BTreeMap::from([(
                    "path".into(),
                    dir.path().join("test.db").to_string_lossy().into_owned(),
                )]),
                secret: None,
                connect_timeout_ms: 1000,
            })
            .await
            .unwrap();
        let exec:ExecRequest=serde_json::from_value(serde_json::json!({"text":"CREATE TABLE imported (id INTEGER PRIMARY KEY, value TEXT)"})).unwrap();
        session.execute(&exec, CancelToken::new()).await.unwrap();
        host.sessions.lock().unwrap().insert(
            "s".into(),
            Arc::new(SessionRecord {
                session: Arc::from(session),
                profile: serde_json::from_value(
                    serde_json::json!({"id":"w3","name":"test","driver_id":"sqlite","fields":{}}),
                )
                .unwrap(),
                secret: None,
                _tunnel: None,
                dead: AtomicBool::new(false),
            }),
        );
        (host, dir)
    }
    #[tokio::test]
    async fn import_approval_is_bound_single_use_and_history_has_no_values() {
        let (host, _dir) = fixture().await;
        let req = EditRequest {originals: vec![],
            create_table: None,
            node_id: "table:imported".into(),
            edits: vec![RowEdit::Insert {
                values: vec![("value".into(), Value::Text("private-import-content".into()))],
            }],
            timeout_ms: 1000,
            preview_sample_limit: 100,
        };
        assert!(matches!(
            import_run(
                &host,
                "s".into(),
                "unapproved".into(),
                req.clone(),
                false,
                None
            )
            .await,
            Err(DbError {
                kind: ErrorKind::Auth,
                ..
            })
        ));
        import_run(&host, "s".into(), "preview".into(), req.clone(), true, None)
            .await
            .unwrap();
        let mut changed = req.clone();
        changed.edits.push(RowEdit::Insert { values: vec![] });
        assert!(matches!(
            import_run(&host, "s".into(), "changed".into(), changed, false, None).await,
            Err(DbError {
                kind: ErrorKind::Auth,
                ..
            })
        ));
        import_run(
            &host,
            "s".into(),
            "approved".into(),
            req.clone(),
            false,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(
            import_run(&host, "s".into(), "replay".into(), req, false, None).await,
            Err(DbError {
                kind: ErrorKind::Auth,
                ..
            })
        ));
        let history = host.library.run(|store| store.history_list(200)).await.unwrap();
        assert!(history.iter().all(|entry| !entry.text.contains("private-import-content")));
        host.session("s").unwrap().session.close().await;
    }
    #[tokio::test]
    async fn migration_preview_approval_history_and_destructive_confirmation() {
        let (host, dir) = fixture().await;
        let app = tauri::test::mock_builder()
            .manage(host)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let folder = dir.path().join("migrations");
        std::fs::create_dir(&folder).unwrap();
        let sql = "CREATE TABLE new_migration (id INTEGER PRIMARY KEY)";
        std::fs::write(folder.join("001_create.sql"), sql).unwrap();
        let path = folder.to_string_lossy().into_owned();
        assert!(
            !migration_scan(app.state(), "s".into(), path.clone())
                .await
                .unwrap()
                .migrations[0]
                .applied
        );
        assert!(migration_apply(
            app.state(),
            "s".into(),
            "unapproved".into(),
            sql.into(),
            None,
            Some("001".into())
        )
        .await
        .is_err());
        migration_preview(app.state(), "s".into(), "preview".into(), sql.into())
            .await
            .unwrap();
        migration_apply(
            app.state(),
            "s".into(),
            "apply".into(),
            sql.into(),
            None,
            Some("001".into()),
        )
        .await
        .unwrap();
        assert!(
            migration_scan(app.state(), "s".into(), path)
                .await
                .unwrap()
                .migrations[0]
                .applied
        );
        let drop_sql = "DROP TABLE new_migration";
        migration_preview(
            app.state(),
            "s".into(),
            "drop-preview".into(),
            drop_sql.into(),
        )
        .await
        .unwrap();
        assert!(migration_apply(
            app.state(),
            "s".into(),
            "drop-wrong".into(),
            drop_sql.into(),
            Some("wrong".into()),
            None
        )
        .await
        .is_err());
        migration_apply(
            app.state(),
            "s".into(),
            "drop-apply".into(),
            drop_sql.into(),
            Some("test".into()),
            None,
        )
        .await
        .unwrap();
        app.state::<Host>()
            .session("s")
            .unwrap()
            .session
            .close()
            .await;
    }
    #[test]
    fn snapshot_file_roundtrip_and_version_rejection() {
        let s = SchemaSnapshot {
            version: 1,
            dialect: "sqlite".into(),
            objects: vec![],
        };
        assert_eq!(
            snapshot_decode(snapshot_encode(s.clone()).unwrap().into_bytes()).unwrap(),
            s
        );
        assert!(
            snapshot_decode(br#"{"version":2,"dialect":"sqlite","objects":[]}"#.to_vec()).is_err()
        );
    }
}
