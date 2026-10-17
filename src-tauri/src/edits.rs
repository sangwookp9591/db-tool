use super::*;
#[tauri::command]
pub async fn edit_preview(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: EditRequest,
) -> CommandResult<ChangePreview> {
    match run(&host, session_id, run_id, req, true).await? {
        Output::Preview(r) => Ok(r),
        _ => unreachable!(),
    }
}
#[tauri::command]
pub async fn edit_apply(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: EditRequest,
) -> CommandResult<EditResult> {
    match run(&host, session_id, run_id, req, false).await? {
        Output::Apply(r) => Ok(r),
        _ => unreachable!(),
    }
}
#[derive(Debug)]
enum Output {
    Preview(ChangePreview),
    Apply(EditResult),
}
async fn run(
    host: &Host,
    session_id: String,
    run_id: String,
    req: EditRequest,
    preview: bool,
) -> CommandResult<Output> {
    let record = host.session(&session_id)?;
    if host.access_mode(&record)? == permissions::AccessMode::ReadOnly {
        return Err(DbError::new(
            ErrorKind::Auth,
            "Row editing forbidden on a read-only connection",
        ));
    }
    if !host
        .driver(&record.profile.driver_id)?
        .manifest()
        .capabilities
        .edit_rows
    {
        return Err(DbError::unsupported("Row editing is not supported"));
    }
    let fingerprint = permissions::fingerprint(&format!("edit:{session_id}"), &req)?;
    let run = host.run(&session_id, &run_id)?;
    {
        let mut previews = host
            .previews
            .lock()
            .map_err(|_| DbError::internal("Preview lock unavailable"))?;
        // A fresh preview invalidates a previous approval even when the new preview fails.
        let approval = previews.remove(&fingerprint);
        if !preview
            && !approval.is_some_and(|p| {
                p.created.elapsed() < Duration::from_secs(60) && p.fingerprint == fingerprint
            })
        {
            return Err(DbError::new(ErrorKind::Auth, "Preview required"));
        }
    }
    let started = Instant::now();
    let future = async {
        if preview {
            record
                .session
                .preview_edits(&req, run.token.clone())
                .await
                .map(Output::Preview)
        } else {
            record
                .session
                .apply_edits(&req, run.token.clone())
                .await
                .map(Output::Apply)
        }
    };
    tokio::pin!(future);
    let result =
        match tokio::time::timeout(Duration::from_millis(req.timeout_ms.max(1)), &mut future).await
        {
            Ok(r) => r,
            Err(_) => {
                run.token.cancel();
                let _ = future.await;
                Err(DbError::new(ErrorKind::Timeout, "Editing timed out"))
            }
        };
    let result = host.observe(&record, result);
    let (text, row_count) = match &result {
        Ok(Output::Preview(p)) => (
            p.statements
                .iter()
                .map(|s| s.text.clone())
                .collect::<Vec<_>>()
                .join(";\n"),
            p.statements.iter().filter_map(|s| s.affected).sum(),
        ),
        Ok(Output::Apply(r)) => (r.statements.join(";\n"), r.affected.iter().sum()),
        Err(e) => (
            e.detail.clone().unwrap_or_else(|| {
                format!("Row edits for {} ({} edits)", req.node_id, req.edits.len())
            }),
            0,
        ),
    };
    let entry = HistoryEntry {
        kind: if preview {
            "edit_preview"
        } else {
            "edit_apply"
        }
        .into(),
        source: HistorySource::default(),
        timestamp: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        profile_id: Some(record.profile.id.clone()),
        profile_name: storage::redact(&record.profile.name, record.secret.as_deref()),
        text: storage::redact(&text, record.secret.as_deref()),
        elapsed_ms: started.elapsed().as_millis() as u64,
        row_count,
        error: result
            .as_ref()
            .err()
            .map(|e| storage::redact(&e.message, record.secret.as_deref())),
    };
    if host.history(entry).await.is_err() {
        tracing::error!("Could not save edit history");
    }
    if preview && result.is_ok() {
        let mut previews = host
            .previews
            .lock()
            .map_err(|_| DbError::internal("Preview lock unavailable"))?;
        previews.retain(|_, p| p.created.elapsed() < Duration::from_secs(60));
        if previews.len() < 64 {
            previews.insert(
                fingerprint.clone(),
                permissions::Approval {
                    fingerprint,
                    created: Instant::now(),
                },
            );
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (Host, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("axdb-c2-host-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let host = Host::new(Paths {
            config: dir.clone(),
            data: dir.clone(),
            log: dir.clone(),
        });
        let session = driver_sqlite::SqliteDriver::new()
            .connect(&ConnectConfig {
                fields: std::collections::BTreeMap::from([(
                    "path".into(),
                    dir.join("test.db").to_str().unwrap().into(),
                )]),
                secret: None,
                connect_timeout_ms: 1000,
            })
            .await
            .unwrap();
        let exec:ExecRequest=serde_json::from_value(serde_json::json!({"text":"CREATE TABLE c2(id INTEGER PRIMARY KEY,t TEXT); INSERT INTO c2 VALUES(1,'old')"})).unwrap();
        session.execute(&exec, CancelToken::new()).await.unwrap();
        host.sessions.lock().unwrap().insert(
            "s".into(),
            Arc::new(SessionRecord {
                session: Arc::from(session),
                profile: Profile {
                    id: "p".into(),
                    name: "test".into(),
                    driver_id: "sqlite".into(),
                    fields: Default::default(),
                    save_secret: false,
                    access_mode: permissions::AccessMode::ReadWrite,
                },
                secret: Some("private-secret".into()),
                dead: AtomicBool::new(false),
            }),
        );
        (host, dir)
    }
    fn request() -> EditRequest {
        EditRequest {
            node_id: "table:c2".into(),
            edits: vec![RowEdit::Update {
                key: vec![("id".into(), Value::Int("1".into()))],
                set: vec![("t".into(), Value::Text("private-secret".into()))],
            }],
            timeout_ms: 5000,
            preview_sample_limit: 50,
        }
    }
    #[tokio::test]
    async fn host_requires_matching_fresh_single_use_edit_preview() {
        let (host, dir) = fixture().await;
        let req = request();
        assert!(
            matches!(run(&host,"s".into(),"no-preview".into(),req.clone(),false).await,Err(DbError{kind:ErrorKind::Auth,message,..}) if message=="Preview required")
        );
        assert!(matches!(
            run(&host, "s".into(), "preview".into(), req.clone(), true).await,
            Ok(Output::Preview(_))
        ));
        let mut changed = req.clone();
        changed.timeout_ms += 1;
        assert!(matches!(
            run(&host, "s".into(), "changed".into(), changed, false).await,
            Err(DbError {
                kind: ErrorKind::Auth,
                ..
            })
        ));
        assert!(matches!(
            run(&host, "s".into(), "apply".into(), req.clone(), false).await,
            Ok(Output::Apply(_))
        ));
        assert!(matches!(
            run(&host, "s".into(), "replay".into(), req.clone(), false).await,
            Err(DbError {
                kind: ErrorKind::Auth,
                ..
            })
        ));
        run(
            &host,
            "s".into(),
            "expired-preview".into(),
            req.clone(),
            true,
        )
        .await
        .unwrap();
        for approval in host.previews.lock().unwrap().values_mut() {
            approval.created = Instant::now() - Duration::from_secs(61);
        }
        assert!(matches!(
            run(&host, "s".into(), "expired".into(), req.clone(), false).await,
            Err(DbError {
                kind: ErrorKind::Auth,
                ..
            })
        ));
        let history = storage::read_history(&host.paths.data.join("history.jsonl")).unwrap();
        assert!(history
            .iter()
            .any(|h| h.kind == "edit_preview" && h.text.contains("?1")));
        assert!(history
            .iter()
            .any(|h| h.kind == "edit_apply" && h.text.contains("?1")));
        assert!(history.iter().all(|h| !h.text.contains("private-secret")));
        host.session("s").unwrap().session.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn host_rejects_read_only_and_unsupported_edits() {
        let (host, dir) = fixture().await;
        let record = host.sessions.lock().unwrap().remove("s").unwrap();
        let mut profile = record.profile.clone();
        profile.access_mode = permissions::AccessMode::ReadOnly;
        host.sessions.lock().unwrap().insert(
            "s".into(),
            Arc::new(SessionRecord {
                session: record.session.clone(),
                profile: profile.clone(),
                secret: None,
                dead: AtomicBool::new(false),
            }),
        );
        for preview in [false, true] {
            assert!(matches!(
                run(
                    &host,
                    "s".into(),
                    format!("ro-{preview}"),
                    request(),
                    preview
                )
                .await,
                Err(DbError {
                    kind: ErrorKind::Auth,
                    ..
                })
            ));
        }
        profile.access_mode = permissions::AccessMode::ReadWrite;
        profile.driver_id = "neo4j".into();
        host.sessions.lock().unwrap().insert(
            "s".into(),
            Arc::new(SessionRecord {
                session: record.session.clone(),
                profile,
                secret: None,
                dead: AtomicBool::new(false),
            }),
        );
        for preview in [false, true] {
            assert!(matches!(
                run(
                    &host,
                    "s".into(),
                    format!("unsupported-{preview}"),
                    request(),
                    preview
                )
                .await,
                Err(DbError {
                    kind: ErrorKind::Unsupported,
                    ..
                })
            ));
        }
        record.session.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}
