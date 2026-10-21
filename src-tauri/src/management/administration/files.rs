use super::*;
use crate::backup_file::{self, Manifest, Staged};
use std::io::{Read, Seek, SeekFrom};
pub(crate) struct Selected {
    pub staged: Staged,
    pub session: String,
    pub created: Instant,
}
#[derive(Serialize)]
pub struct Picked {
    pub token: String,
    pub name: String,
    pub manifest: Manifest,
}
#[derive(Serialize)]
pub struct FilePreview {
    pub manifest: Manifest,
    pub risk: OperationRisk,
    pub needs_approval: bool,
    pub warnings: Vec<String>,
}
struct Progress<'a, R: tauri::Runtime> {
    host: &'a Host,
    app: &'a tauri::AppHandle<R>,
    session: &'a str,
    run: &'a str,
    phase: &'a str,
    last: Mutex<(Instant, u64)>,
}
impl<R: tauri::Runtime> Progress<'_, R> {
    fn emit(&self, done: u64, total: u64) {
        if let Ok(mut last) = self.last.lock() {
            if last.0.elapsed() < Duration::from_millis(100) && (total == 0 || done < total) {
                return;
            }
            *last = (Instant::now(), done);
        }
        status(
            self.host,
            self.app,
            self.session,
            BackupStatus {
                run_id: self.run.into(),
                phase: self.phase.into(),
                completed: done,
                total,
                manifest: None,
            },
        );
    }
}
#[async_trait::async_trait]
impl<R: tauri::Runtime> ProgressSink for Progress<'_, R> {
    async fn progress(&self, done: u64, total: u64) -> Result<(), DbError> {
        self.emit(done, total);
        Ok(())
    }
}
#[tauri::command]
pub async fn restore_options(
    host: tauri::State<'_, Host>,
    session_id: String,
) -> CommandResult<RestoreOptions> {
    capability(&host, &session_id, true)?
        .session
        .restore_options()
        .await
}
#[tauri::command]
pub async fn backup_create_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    host: tauri::State<'_, Host>,
    exports: tauri::State<'_, crate::export::Exports>,
    session_id: String,
    run_id: String,
    path: std::path::PathBuf,
    token: String,
) -> CommandResult<Manifest> {
    let record = capability(&host, &session_id, true)?;
    let path = exports.take_path(&token, &path).map_err(DbError::query)?;
    let run = host.run(&session_id, &run_id)?;
    let started = Instant::now();
    let progress = Progress {
        host: &host,
        app: &app,
        session: &session_id,
        run: &run_id,
        phase: "running",
        last: Mutex::new((Instant::now() - Duration::from_secs(1), 0)),
    };
    status(
        &host,
        &app,
        &session_id,
        BackupStatus {
            run_id: run_id.clone(),
            phase: "running".into(),
            ..Default::default()
        },
    );
    let result = host.observe(
        &record,
        backup_file::create(&*record.session, &path, run.token.clone(), &progress).await,
    );
    status(
        &host,
        &app,
        &session_id,
        BackupStatus {
            run_id: run_id.clone(),
            phase: if result.is_ok() {
                "complete"
            } else if run.token.is_cancelled() {
                "cancelled"
            } else {
                "failed"
            }
            .into(),
            completed: result.as_ref().map(|m| m.bytes).unwrap_or(0),
            total: result.as_ref().map(|m| m.bytes).unwrap_or(0),
            manifest: result.as_ref().ok().map(|m| BackupManifest {
                version: m.version,
                format: m.format.clone(),
                sha256: m.sha256.clone(),
                bytes: m.bytes,
                counts: m.counts.clone(),
            }),
        },
    );
    audit(
        &host,
        &record,
        "backup",
        started,
        result
            .as_ref()
            .map(|m| {
                m.counts
                    .iter()
                    .fold(0u64, |sum, (_, n)| sum.saturating_add(*n))
            })
            .unwrap_or(0),
        result.is_err(),
    )
    .await;
    result
}
#[tauri::command]
pub async fn restore_pick_file(
    window: tauri::WebviewWindow,
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
) -> CommandResult<Option<Picked>> {
    if window.label() != "main" {
        return Err(DbError::new(
            ErrorKind::Auth,
            "Use the main window to select a backup",
        ));
    }
    let record = capability(&host, &session_id, true)?;
    writable(&host, &record)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("AxDB backup", &["axbackup"])
        .pick_file(move |file| {
            let _ = tx.send(file);
        });
    let Some(file) = rx.await.map_err(|_| DbError::query("File dialog closed"))? else {
        return Ok(None);
    };
    let path = file
        .into_path()
        .map_err(|_| DbError::query("Select a local backup file"))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DbError::query("Invalid backup filename"))?
        .to_owned();
    let run = host.run(&session_id, &run_id)?;
    let cancel = run.token.clone();
    let staged = tokio::task::spawn_blocking(move || {
        backup_file::stage(&mut backup_file::open_selected(&path)?, &cancel)
    })
    .await
    .map_err(|_| DbError::internal("Backup verification worker failed"))??;
    let manifest = staged.manifest.clone();
    let token = uuid::Uuid::new_v4().to_string();
    let mut files = host
        .restore_files
        .lock()
        .map_err(|_| DbError::internal("Restore selection lock unavailable"))?;
    files.retain(|_, v| v.created.elapsed() < Duration::from_secs(300));
    if files.len() >= 8 {
        return Err(DbError::query(
            "Too many pending backup selections; finish or cancel an existing restore",
        ));
    }
    files.insert(
        token.clone(),
        Selected {
            staged,
            session: session_id,
            created: Instant::now(),
        },
    );
    Ok(Some(Picked {
        token,
        name,
        manifest,
    }))
}
fn fingerprint(
    session: &str,
    token: &str,
    manifest: &Manifest,
    overwrite: bool,
    new_database: &Option<String>,
) -> CommandResult<String> {
    permissions::fingerprint(
        &format!("restore-file:{session}"),
        &(
            token,
            &manifest.sha256,
            &manifest.format,
            manifest.bytes,
            overwrite,
            new_database,
        ),
    )
}
#[tauri::command]
pub async fn restore_file_preview(
    host: tauri::State<'_, Host>,
    session_id: String,
    token: String,
    overwrite: bool,
    new_database: Option<String>,
) -> CommandResult<FilePreview> {
    let record = capability(&host, &session_id, true)?;
    writable(&host, &record)?;
    let options = record.session.restore_options().await?;
    if overwrite && !options.overwrite {
        return Err(DbError::unsupported(&options.guidance));
    }
    if let Some(name) = &new_database {
        validate_restore_database(name)?;
        if overwrite || !options.new_database {
            return Err(DbError::unsupported(&options.guidance));
        }
    }
    let files = host
        .restore_files
        .lock()
        .map_err(|_| DbError::internal("Restore selection lock unavailable"))?;
    let file = files
        .get(&token)
        .filter(|f| f.session == session_id && f.created.elapsed() < Duration::from_secs(300))
        .ok_or_else(|| DbError::query("Select and verify the backup file again"))?;
    let manifest = file.staged.manifest.clone();
    let fp = fingerprint(&session_id, &token, &manifest, overwrite, &new_database)?;
    receipt(&host, &fp, true)?;
    remember(&host, fp)?;
    let mut warnings = manifest.warnings.clone();
    warnings.push("신뢰하는 백업만 복원하세요. DB 정의가 이 연결의 권한으로 실행됩니다.".into());
    warnings.push(options.guidance);
    if let Some(name) = new_database {
        warnings.push(format!(
            "새 DB {name}에 복원합니다. 검증 후 연결을 교체하세요."
        ));
    }
    Ok(FilePreview {
        manifest,
        risk: if overwrite {
            OperationRisk::Destructive
        } else {
            OperationRisk::Write
        },
        needs_approval: true,
        warnings,
    })
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn restore_file_apply<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    token: String,
    overwrite: bool,
    new_database: Option<String>,
    approved: bool,
    confirmation: Option<String>,
) -> CommandResult<()> {
    let record = capability(&host, &session_id, true)?;
    writable(&host, &record)?;
    if !approved || overwrite && confirmation.as_deref() != Some(record.profile.name.as_str()) {
        return Err(DbError::new(
            ErrorKind::Auth,
            "Human approval and exact connection name for overwrite are required",
        ));
    }
    if let Some(name) = &new_database {
        validate_restore_database(name)?;
        if overwrite {
            return Err(DbError::query("New database cannot overwrite"));
        }
    }
    let mut selected = {
        let mut files = host
            .restore_files
            .lock()
            .map_err(|_| DbError::internal("Restore selection lock unavailable"))?;
        let file = files
            .get(&token)
            .filter(|f| f.session == session_id && f.created.elapsed() < Duration::from_secs(300))
            .ok_or_else(|| DbError::query("Select and verify the backup file again"))?;
        receipt(
            &host,
            &fingerprint(
                &session_id,
                &token,
                &file.staged.manifest,
                overwrite,
                &new_database,
            )?,
            false,
        )?;
        files.remove(&token).ok_or_else(|| DbError::query("Select and verify the backup file again"))?
    };
    let run = host.run(&session_id, &run_id)?;
    let started = Instant::now();
    let total = selected.staged.manifest.bytes;
    let progress = Progress {
        host: &host,
        app: &app,
        session: &session_id,
        run: &run_id,
        phase: "restoring",
        last: Mutex::new((Instant::now() - Duration::from_secs(1), 0)),
    };
    struct Reader<'a, 'b, R: tauri::Runtime> {
        file: &'a mut std::fs::File,
        progress: &'a Progress<'b, R>,
        done: u64,
        total: u64,
        cancel: CancelToken,
    }
    impl<R: tauri::Runtime> Read for Reader<'_, '_, R> {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if self.cancel.is_cancelled() {
                return Err(std::io::Error::other("Restore cancelled"));
            }
            let len = self.file.read(bytes)?;
            self.done += len as u64;
            self.progress.emit(self.done, self.total);
            Ok(len)
        }
    }
    selected
        .staged
        .payload
        .seek(SeekFrom::Start(0))
        .map_err(|_| DbError::query("Cannot rewind restore"))?;
    let mut source = Reader {
        file: &mut selected.staged.payload,
        progress: &progress,
        done: 0,
        total,
        cancel: run.token.clone(),
    };
    let summary = selected.staged.manifest.summary();
    let result = if let Some(name) = new_database {
        record
            .session
            .restore_database(&summary, &mut source, &name, run.token.clone())
            .await
    } else {
        record
            .session
            .restore_stream(&summary, &mut source, overwrite, run.token.clone())
            .await
    };
    let result = host.observe(&record, result);
    status(
        &host,
        &app,
        &session_id,
        BackupStatus {
            run_id: run_id.clone(),
            phase: if result.is_ok() {
                "restored"
            } else if run.token.is_cancelled() {
                "cancelled"
            } else {
                "failed"
            }
            .into(),
            completed: if result.is_ok() { total } else { source.done },
            total,
            manifest: None,
        },
    );
    audit(
        &host,
        &record,
        "restore",
        started,
        summary
            .counts
            .iter()
            .fold(0u64, |sum, (_, n)| sum.saturating_add(*n)),
        result.is_err(),
    )
    .await;
    result
}
#[tauri::command]
pub fn restore_file_discard(
    host: tauri::State<'_, Host>,
    session_id: String,
    token: String,
) -> CommandResult<()> {
    let mut files = host
        .restore_files
        .lock()
        .map_err(|_| DbError::internal("Restore selection lock unavailable"))?;
    if files.get(&token).is_some_and(|f| f.session == session_id) {
        files.remove(&token);
    }
    Ok(())
}
