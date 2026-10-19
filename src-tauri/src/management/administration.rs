use super::*;
pub(crate) mod files;
use sha2::{Digest, Sha256};
use std::io::Read;
const MAGIC: &[u8] = b"AXBACKUP\x01";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupManifest {
    pub version: u32,
    pub format: String,
    pub sha256: String,
    pub bytes: u64,
    pub counts: Vec<(String, u64)>,
}
#[derive(Serialize)]
pub struct BackupArchive {
    pub manifest: BackupManifest,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct BackupStatus {
    pub run_id: String,
    pub phase: String,
    pub completed: u64,
    pub total: u64,
    pub manifest: Option<BackupManifest>,
}
#[derive(Serialize)]
pub struct RestorePreview {
    pub manifest: BackupManifest,
    pub risk: OperationRisk,
    pub needs_approval: bool,
    pub warnings: Vec<String>,
}
fn checksum(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn encode(backup: &BackupData) -> CommandResult<BackupArchive> {
    backup.validate(&backup.format)?;
    let manifest = BackupManifest {
        version: 1,
        format: backup.format.clone(),
        sha256: checksum(&backup.data),
        bytes: backup.data.len() as u64,
        counts: backup.counts.clone(),
    };
    let json =
        serde_json::to_vec(&manifest).map_err(|_| DbError::query("Cannot encode manifest"))?;
    if json.len() > 512 * 1024 {
        return Err(DbError::query("Backup manifest exceeds 512 KiB"));
    }
    let compressed = zstd::stream::encode_all(backup.data.as_slice(), 3)
        .map_err(|_| DbError::query("Cannot compress backup"))?;
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&(json.len() as u32).to_be_bytes());
    bytes.extend(json);
    bytes.extend(compressed);
    Ok(BackupArchive { manifest, bytes })
}
fn decode(bytes: &[u8]) -> CommandResult<(BackupManifest, BackupData)> {
    if bytes.len() > BACKUP_MAX_BYTES + 1024 * 1024
        || !bytes.starts_with(MAGIC)
        || bytes.len() < MAGIC.len() + 4
    {
        return Err(DbError::query(
            "Invalid .axbackup file or size limit exceeded",
        ));
    }
    let offset = MAGIC.len();
    let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let offset = offset + 4;
    if len > 512 * 1024 || offset + len >= bytes.len() {
        return Err(DbError::query("Invalid backup manifest length"));
    }
    let manifest: BackupManifest = serde_json::from_slice(&bytes[offset..offset + len])
        .map_err(|_| DbError::query("Invalid backup manifest"))?;
    if manifest.version != 1 || manifest.bytes > BACKUP_MAX_BYTES as u64 {
        return Err(DbError::query("Unsupported backup version or size"));
    }
    let decoder = zstd::stream::read::Decoder::new(&bytes[offset + len..])
        .map_err(|_| DbError::query("Invalid zstd backup"))?;
    let mut data = vec![];
    decoder
        .take(BACKUP_MAX_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| DbError::query("Invalid zstd backup"))?;
    if data.len() as u64 != manifest.bytes || checksum(&data) != manifest.sha256 {
        return Err(DbError::query("Backup checksum or length mismatch"));
    }
    let backup = BackupData {
        format: manifest.format.clone(),
        data,
        counts: manifest.counts.clone(),
    };
    backup.validate(&manifest.format)?;
    Ok((manifest, backup))
}
fn capability(host: &Host, session_id: &str, backup: bool) -> CommandResult<Arc<SessionRecord>> {
    let record = host.session(session_id)?;
    let driver = host.driver(&record.profile.driver_id)?;
    let c = &driver.manifest().capabilities;
    if !(if backup { c.backup } else { c.roles }) {
        return Err(DbError::unsupported(
            "Driver does not support this administration operation",
        ));
    }
    Ok(record)
}
fn writable(host: &Host, record: &SessionRecord) -> CommandResult<()> {
    if host.access_mode(record)? == permissions::AccessMode::ReadOnly {
        Err(DbError::new(ErrorKind::Auth, "Read-only connection"))
    } else {
        Ok(())
    }
}
fn status<R: tauri::Runtime>(
    host: &Host,
    app: &tauri::AppHandle<R>,
    session: &str,
    value: BackupStatus,
) {
    if let Ok(mut statuses) = host.backup_statuses.lock() {
        if statuses.len() >= 64 {
            statuses.retain(|_, s| s.phase == "running");
        }
        statuses.insert(session.into(), value.clone());
    }
    use tauri::Emitter;
    let _ = app.emit("backup-progress", value);
}
async fn audit(
    host: &Host,
    record: &SessionRecord,
    kind: &str,
    started: Instant,
    count: u64,
    failed: bool,
) {
    let _ = host
        .history(HistoryEntry {
            kind: kind.into(),
            source: HistorySource::default(),
            timestamp: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            profile_id: Some(record.profile.id.clone()),
            profile_name: storage::redact(&record.profile.name, record.secret.as_deref()),
            text: format!("{kind}: {count} items (values omitted)"),
            elapsed_ms: started.elapsed().as_millis() as u64,
            row_count: if failed { 0 } else { count },
            error: failed
                .then(|| "Administration failed; verify outcome if connection was lost".into()),
        })
        .await;
}
#[tauri::command]
pub fn backup_status(
    host: tauri::State<'_, Host>,
    session_id: String,
) -> CommandResult<BackupStatus> {
    capability(&host, &session_id, true)?;
    Ok(host
        .backup_statuses
        .lock()
        .map_err(|_| DbError::internal("Status lock unavailable"))?
        .get(&session_id)
        .cloned()
        .unwrap_or_default())
}
#[tauri::command]
pub async fn backup_create(
    app: tauri::AppHandle,
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
) -> CommandResult<BackupArchive> {
    let record = capability(&host, &session_id, true)?;
    let started = Instant::now();
    let run = host.run(&session_id, &run_id)?;
    let initial = BackupStatus {
        run_id: run_id.clone(),
        phase: "running".into(),
        completed: 0,
        total: 0,
        manifest: None,
    };
    status(&host, &app, &session_id, initial.clone());
    let result = record
        .session
        .backup(run.token.clone())
        .await
        .and_then(|backup| {
            if run.token.is_cancelled() {
                Err(DbError::new(ErrorKind::Cancelled, "Backup cancelled"))
            } else {
                encode(&backup)
            }
        });
    let final_status = BackupStatus {
        phase: if result.is_ok() { "complete" } else { "failed" }.into(),
        completed: result.as_ref().map(|a| a.manifest.bytes).unwrap_or(0),
        total: result.as_ref().map(|a| a.manifest.bytes).unwrap_or(0),
        manifest: result.as_ref().ok().map(|a| a.manifest.clone()),
        ..initial
    };
    status(&host, &app, &session_id, final_status);
    audit(
        &host,
        &record,
        "backup",
        started,
        result
            .as_ref()
            .map(|a| {
                a.manifest
                    .counts
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
pub fn restore_preview(
    host: tauri::State<'_, Host>,
    session_id: String,
    bytes: Vec<u8>,
    overwrite: bool,
) -> CommandResult<RestorePreview> {
    let record = capability(&host, &session_id, true)?;
    writable(&host, &record)?;
    let (manifest, _) = decode(&bytes)?;
    let fingerprint = permissions::fingerprint(
        &format!("restore:{session_id}"),
        &(checksum(&bytes), overwrite),
    )?;
    receipt(&host, &fingerprint, true)?;
    remember(&host, fingerprint)?;
    Ok(RestorePreview{manifest,risk:if overwrite{OperationRisk::Destructive}else{OperationRisk::Write},needs_approval:true,warnings:vec!["Only restore archives from a trusted source: native database schema and data commands will execute with this connection's privileges".into(),"Restore targets the currently connected database. Select a new empty database before applying. MongoDB and Neo4j overwrite are currently disabled.".into()]})
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn restore_apply<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    bytes: Vec<u8>,
    overwrite: bool,
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
    let (_, backup) = decode(&bytes)?;
    let fingerprint = permissions::fingerprint(
        &format!("restore:{session_id}"),
        &(checksum(&bytes), overwrite),
    )?;
    receipt(&host, &fingerprint, false)?;
    let run = host.run(&session_id, &run_id)?;
    let started = Instant::now();
    let initial = BackupStatus {
        run_id,
        phase: "restoring".into(),
        completed: 0,
        total: backup.data.len() as u64,
        manifest: None,
    };
    status(&host, &app, &session_id, initial.clone());
    let result = host.observe(
        &record,
        record
            .session
            .restore(&backup, overwrite, run.token.clone())
            .await,
    );
    status(
        &host,
        &app,
        &session_id,
        BackupStatus {
            phase: if result.is_ok() { "restored" } else { "failed" }.into(),
            completed: if result.is_ok() { initial.total } else { 0 },
            ..initial
        },
    );
    audit(
        &host,
        &record,
        "restore",
        started,
        backup
            .counts
            .iter()
            .fold(0u64, |sum, (_, n)| sum.saturating_add(*n)),
        result.is_err(),
    )
    .await;
    result
}
#[tauri::command]
pub async fn roles(
    host: tauri::State<'_, Host>,
    session_id: String,
) -> CommandResult<RolesSnapshot> {
    let r = capability(&host, &session_id, false)?;
    host.observe(&r, r.session.roles().await)
}
#[tauri::command]
pub async fn plan_grants(
    host: tauri::State<'_, Host>,
    session_id: String,
    changes: Vec<GrantChange>,
) -> CommandResult<GrantPlan> {
    let r = capability(&host, &session_id, false)?;
    validate_grants(&changes)?;
    r.session.plan_grants(&changes).await
}
#[tauri::command]
pub async fn grants_preview(
    host: tauri::State<'_, Host>,
    session_id: String,
    changes: Vec<GrantChange>,
) -> CommandResult<GrantPlan> {
    let r = capability(&host, &session_id, false)?;
    writable(&host, &r)?;
    let plan = r
        .session
        .preview_grants(&changes, CancelToken::new())
        .await?;
    if !plan.apply_supported {
        return Err(DbError::unsupported(
            "This driver offers grant planning only",
        ));
    }
    let fingerprint = permissions::fingerprint(&format!("grants:{session_id}"), &changes)?;
    receipt(&host, &fingerprint, true)?;
    remember(&host, fingerprint)?;
    Ok(plan)
}
#[tauri::command]
pub async fn grants_apply(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    changes: Vec<GrantChange>,
    approved: bool,
) -> CommandResult<()> {
    let r = capability(&host, &session_id, false)?;
    writable(&host, &r)?;
    if !approved {
        return Err(DbError::new(ErrorKind::Auth, "Human approval required"));
    }
    let fingerprint = permissions::fingerprint(&format!("grants:{session_id}"), &changes)?;
    receipt(&host, &fingerprint, false)?;
    let run = host.run(&session_id, &run_id)?;
    let started = Instant::now();
    let result = host.observe(
        &r,
        r.session.apply_grants(&changes, run.token.clone()).await,
    );
    audit(
        &host,
        &r,
        "grants",
        started,
        changes.len() as u64,
        result.is_err(),
    )
    .await;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_roundtrip_and_tamper_rejection() {
        let writer=crate::backup_file::ArchiveWriter::new(tempfile::tempfile().unwrap()).unwrap();
        assert!(writer.finish(BackupSummary::default()).is_err());
        let writer=crate::backup_file::ArchiveWriter::new(tempfile::tempfile().unwrap()).unwrap();
        assert!(writer.finish(BackupSummary{format:"test".into(),counts:vec![(String::new(),0);50001],warnings:vec![]}).is_err());
        let b = BackupData {
            format: "test".into(),
            data: b"sensitive row data".to_vec(),
            counts: vec![("table".into(), 1)],
        };
        let a = encode(&b).unwrap();
        assert_eq!(decode(&a.bytes).unwrap().1, b);
        let mut bytes = a.bytes;
        let pos = MAGIC.len() + 4;
        bytes[pos] = b'!';
        assert!(decode(&bytes).is_err());
        assert!(decode(b"AXBACKUP").is_err());
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("legacy.axbackup");
        std::fs::write(&path,encode(&b).unwrap().bytes).unwrap();
        let staged=crate::backup_file::stage(&mut std::fs::File::open(&path).unwrap(),&CancelToken::new()).unwrap();
        assert_eq!(staged.manifest.version,1);assert_eq!(staged.manifest.bytes,b.data.len() as u64);
    }
    #[tokio::test]
    async fn restore_requires_exact_one_use_preview_human_approval_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::new(Paths {
            config: dir.path().into(),
            data: dir.path().into(),
            log: dir.path().into(),
        });
        let driver = driver_sqlite::SqliteDriver::new();
        host.registry
            .register(Arc::new(driver_sqlite::SqliteDriver::new()));
        let source = driver
            .connect(&ConnectConfig {
                fields: std::collections::BTreeMap::from([(
                    "path".into(),
                    dir.path().join("source.db").to_string_lossy().into_owned(),
                )]),
                secret: None,
                connect_timeout_ms: 1000,
            })
            .await
            .unwrap();
        let req:ExecRequest=serde_json::from_value(serde_json::json!({"text":"CREATE TABLE secret_items(id INTEGER); INSERT INTO secret_items VALUES(7)"})).unwrap();
        source.execute(&req, CancelToken::new()).await.unwrap();
        let archive = encode(&source.backup(CancelToken::new()).await.unwrap()).unwrap();
        let target = driver
            .connect(&ConnectConfig {
                fields: std::collections::BTreeMap::from([(
                    "path".into(),
                    dir.path().join("target.db").to_string_lossy().into_owned(),
                )]),
                secret: None,
                connect_timeout_ms: 1000,
            })
            .await
            .unwrap();
        host.sessions.lock().unwrap().insert("s".into(),Arc::new(SessionRecord{session:Arc::from(target),profile:serde_json::from_value(serde_json::json!({"id":"w4","name":"restore target","driver_id":"sqlite","fields":{}})).unwrap(),secret:None,_tunnel:None,dead:AtomicBool::new(false)}));
        let app = tauri::test::mock_builder()
            .manage(host)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        assert!(restore_apply(
            app.handle().clone(),
            app.state(),
            "s".into(),
            "no-preview".into(),
            archive.bytes.clone(),
            false,
            true,
            None
        )
        .await
        .is_err());
        restore_preview(app.state(), "s".into(), archive.bytes.clone(), false).unwrap();
        assert!(restore_apply(
            app.handle().clone(),
            app.state(),
            "s".into(),
            "no-human".into(),
            archive.bytes.clone(),
            false,
            false,
            None
        )
        .await
        .is_err());
        restore_apply(
            app.handle().clone(),
            app.state(),
            "s".into(),
            "approved".into(),
            archive.bytes.clone(),
            false,
            true,
            None,
        )
        .await
        .unwrap();
        assert!(restore_apply(
            app.handle().clone(),
            app.state(),
            "s".into(),
            "replay".into(),
            archive.bytes.clone(),
            false,
            true,
            None
        )
        .await
        .is_err());
        let preview =
            restore_preview(app.state(), "s".into(), archive.bytes.clone(), true).unwrap();
        assert_eq!(preview.risk, OperationRisk::Destructive);
        assert!(restore_apply(
            app.handle().clone(),
            app.state(),
            "s".into(),
            "wrong-name".into(),
            archive.bytes.clone(),
            true,
            true,
            Some("wrong".into())
        )
        .await
        .is_err());
        restore_apply(
            app.handle().clone(),
            app.state(),
            "s".into(),
            "overwrite".into(),
            archive.bytes,
            true,
            true,
            Some("restore target".into()),
        )
        .await
        .unwrap();
        struct Progress;
        #[async_trait::async_trait] impl ProgressSink for Progress {async fn progress(&self,_:u64,_:u64)->Result<(),DbError>{Ok(())}}
        let path=dir.path().join("stream.axbackup");
        crate::backup_file::create(&*source,&path,CancelToken::new(),&Progress).await.unwrap();
        let staged=crate::backup_file::stage(&mut std::fs::File::open(&path).unwrap(),&CancelToken::new()).unwrap();
        app.state::<Host>().restore_files.lock().unwrap().insert("selected-file".into(),files::Selected{staged,session:"s".into(),created:Instant::now()});
        assert!(files::restore_file_apply(app.handle().clone(),app.state(),"s".into(),"file-no-preview".into(),"selected-file".into(),true,None,true,Some("restore target".into())).await.is_err());
        assert!(files::restore_file_preview(app.state(),"s".into(),"selected-file".into(),false,Some("newdb".into())).await.is_err());
        files::restore_file_preview(app.state(),"s".into(),"selected-file".into(),true,None).await.unwrap();
        assert!(files::restore_file_apply(app.handle().clone(),app.state(),"s".into(),"file-no-human".into(),"selected-file".into(),true,None,false,Some("restore target".into())).await.is_err());
        assert!(files::restore_file_apply(app.handle().clone(),app.state(),"s".into(),"file-wrong-name".into(),"selected-file".into(),true,None,true,Some("wrong".into())).await.is_err());
        assert!(files::restore_file_apply(app.handle().clone(),app.state(),"s".into(),"file-changed-target".into(),"selected-file".into(),false,None,true,None).await.is_err());
        files::restore_file_apply(app.handle().clone(),app.state(),"s".into(),"file-approved".into(),"selected-file".into(),true,None,true,Some("restore target".into())).await.unwrap();
        assert!(files::restore_file_apply(app.handle().clone(),app.state(),"s".into(),"file-replay".into(),"selected-file".into(),true,None,true,Some("restore target".into())).await.is_err());
        source.close().await;
        app.state::<Host>()
            .session("s")
            .unwrap()
            .session
            .close()
            .await;
    }
}
