use super::*;
use std::path::Path;
#[derive(Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub name: String,
    pub format: String,
    pub text: String,
    pub checksum: String,
    pub applied: bool,
}
#[derive(Serialize)]
pub struct MigrationList {
    pub migrations: Vec<Migration>,
    pub history_tables: Vec<String>,
    pub warnings: Vec<String>,
}
fn candidate(path: &Path) -> Option<(String, String)> {
    let name = path.file_name()?.to_str()?;
    if name == "migration.sql" {
        return Some((
            path.parent()?.file_name()?.to_str()?.into(),
            "prisma".into(),
        ));
    }
    let (prefix, _) = name.split_once('_')?;
    if name.ends_with(".sql")
        && prefix.starts_with('V')
        && prefix.len() > 1
        && prefix[1..].split('.').all(|part| !part.is_empty())
        && name.contains("__")
        && prefix[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return Some((prefix[1..].into(), "flyway".into()));
    }
    if name.ends_with(".sql") && !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
        return Some((prefix.into(), "plain".into()));
    }
    None
}
fn scan(root: &Path) -> CommandResult<Vec<Migration>> {
    if root
        .symlink_metadata()
        .map_err(|_| DbError::query("Cannot read migration folder"))?
        .file_type()
        .is_symlink()
    {
        return Err(DbError::query("Migration folder symlinks are not allowed"));
    }
    let root = root
        .canonicalize()
        .map_err(|_| DbError::query("Cannot read migration folder"))?;
    let mut paths = vec![];
    let mut entries = 0;
    let mut total = 0;
    fn visit(
        dir: &Path,
        depth: usize,
        paths: &mut Vec<std::path::PathBuf>,
        entries: &mut usize,
    ) -> CommandResult<()> {
        if depth > 3 {
            return Ok(());
        }
        for item in
            std::fs::read_dir(dir).map_err(|_| DbError::query("Cannot list migration folder"))?
        {
            *entries += 1;
            if *entries > 5000 {
                return Err(DbError::query("Migration folder exceeds 5000 entries"));
            }
            let item = item.map_err(|_| DbError::query("Cannot read migration entry"))?;
            let ty = item
                .file_type()
                .map_err(|_| DbError::query("Cannot read migration type"))?;
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                visit(&item.path(), depth + 1, paths, entries)?;
            } else if ty.is_file() && candidate(&item.path()).is_some() {
                paths.push(item.path());
            }
        }
        Ok(())
    }
    visit(&root, 0, &mut paths, &mut entries)?;
    let mut migrations = vec![];
    for p in paths {
        let (id, format) = candidate(&p).unwrap();
        let bytes = read_bounded(&p, 1024 * 1024)?;
        total += bytes.len();
        if total > 8 * 1024 * 1024 {
            return Err(DbError::query("Migration folder exceeds 8 MiB SQL"));
        }
        let text =
            String::from_utf8(bytes).map_err(|_| DbError::query("Migration must be UTF-8"))?;
        migrations.push(Migration {
            id,
            name: p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            format,
            text: text.clone(),
            checksum: permissions::hash(&text),
            applied: false,
        });
    }
    let key = |id: &str| {
        id.split('.')
            .map(|s| {
                let n = s.trim_start_matches('0');
                (n.len(), n.to_owned())
            })
            .collect::<Vec<_>>()
    };
    migrations.sort_by(|a, b| key(&a.id).cmp(&key(&b.id)).then(a.name.cmp(&b.name)));
    if migrations.windows(2).any(|m| m[0].id == m[1].id) {
        return Err(DbError::query("Duplicate migration versions"));
    }
    Ok(migrations)
}
fn request(text: String) -> ExecRequest {
    ExecRequest {
        text,
        read_only: false,
        params: Default::default(),
        row_limit: 100,
        graph_node_limit: 1,
        timeout_ms: 30000,
        preview_sample_limit: 100,
    }
}
pub fn validate(text: &str) -> CommandResult<bool> {
    if text.trim().is_empty() || text.len() > 1024 * 1024 {
        return Err(DbError::query("Migration requires 1 byte..1 MiB SQL"));
    }
    let tokens = dbcore::preview::tokens(text)?;
    let mut start = 0;
    for end in tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| (t.text == ";" && t.depth == 0).then_some(i))
        .chain(std::iter::once(tokens.len()))
    {
        if let Some(reason) = dbcore::preview::rejection(&tokens[start..end]) {
            return Err(DbError::query(reason));
        }
        start = end.saturating_add(1);
    }
    // ALTER may narrow types, remove constraints or rewrite data; require destructive approval conservatively.
    Ok(tokens.iter().any(|t| {
        ["DROP", "TRUNCATE", "ALTER", "DELETE", "REPLACE"]
            .iter()
            .any(|w| t.is(w))
    }))
}
#[tauri::command]
pub async fn migration_scan(
    host: tauri::State<'_, Host>,
    session_id: String,
    path: String,
) -> CommandResult<MigrationList> {
    let r = host.session(&session_id)?;
    let caps = host
        .driver(&r.profile.driver_id)?
        .manifest()
        .capabilities
        .clone();
    if !caps.schema_snapshot || !caps.read_only_exec {
        return Err(DbError::unsupported(
            "Migration history requires schema snapshots and enforced reads",
        ));
    }
    let mut migrations = scan(Path::new(&path))?;
    let snap = r.session.schema_snapshot(None).await?;
    let mut history_tables = vec![];
    let mut warnings = vec![];
    for object in snap.objects.iter().filter(|o| o.kind == "table") {
        let (column, filter) = match object.name.as_str() {
            "flyway_schema_history" => ("version", " WHERE success = true"),
            "_prisma_migrations" => (
                "migration_name",
                " WHERE finished_at IS NOT NULL AND rolled_back_at IS NULL",
            ),
            "schema_migrations" => ("version", ""),
            "axdb_migration_history" => ("version", ""),
            _ => continue,
        };
        let target = format!(
            "{}{}",
            if object.namespace.is_empty() {
                String::new()
            } else {
                format!("{}.", quote_identifier(&object.namespace))
            },
            quote_identifier(&object.name)
        );
        let mut req = request(format!(
            "SELECT {} FROM {target}{filter}",
            quote_identifier(column)
        ));
        req.read_only = true;
        req.row_limit = 100_000;
        let results = r.session.execute(&req, CancelToken::new()).await?;
        for result in results {
            if result.truncated {
                return Err(DbError::query("Migration history exceeds row cap"));
            }
            if let Some(table) = result.table {
                for row in table.rows {
                    if let Some(v) = row.first() {
                        let id = dbcore::edits::text(v)?.unwrap_or_default();
                        for m in &mut migrations {
                            if m.id == id || m.checksum == id {
                                m.applied = true;
                            }
                        }
                    }
                }
            }
        }
        history_tables.push(object.name.clone());
    }
    if history_tables.is_empty() {
        warnings.push("No migration history table found; every local migration is pending".into());
    }
    Ok(MigrationList {
        migrations,
        history_tables,
        warnings,
    })
}
#[tauri::command]
pub async fn migration_preview(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    text: String,
) -> CommandResult<ChangePreview> {
    validate(&text)?;
    let fingerprint = permissions::fingerprint(&format!("migration:{session_id}"), &text)?;
    receipt(&host, &fingerprint, true)?;
    let p = crate::preview(host.clone(), session_id, run_id, request(text), None).await?;
    if p.statements.is_empty()
        || p.statements
            .iter()
            .any(|s| s.warnings.iter().any(|w| w.starts_with("미리보기를 거부")))
    {
        return Err(DbError::query(
            "Migration preview was rejected; inspect SQL and transaction state",
        ));
    }
    remember(&host, fingerprint)?;
    Ok(p)
}
#[tauri::command]
pub async fn migration_apply(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    text: String,
    confirmation: Option<String>,
    migration_id: Option<String>,
) -> CommandResult<Vec<ResultSet>> {
    let destructive = validate(&text)?;
    let r = host.session(&session_id)?;
    if host.access_mode(&r)? == permissions::AccessMode::ReadOnly {
        return Err(DbError::new(ErrorKind::Auth, "Read-only connection"));
    }
    if destructive && confirmation.as_deref() != Some(r.profile.name.as_str()) {
        return Err(DbError::new(
            ErrorKind::Auth,
            "Type the connection name to approve destructive migration",
        ));
    }
    let caps = host
        .driver(&r.profile.driver_id)?
        .manifest()
        .capabilities
        .clone();
    if !caps.ddl_render || !caps.transactions {
        return Err(DbError::unsupported("Atomic migrations unsupported"));
    }
    receipt(
        &host,
        &permissions::fingerprint(&format!("migration:{session_id}"), &text)?,
        false,
    )?;
    let run = host.run(&session_id, &run_id)?;
    let started = Instant::now();
    let checksum = permissions::hash(&text);
    let version = migration_id.unwrap_or(checksum);
    if version.is_empty() || version.len() > 255 || version.contains('\0') {
        return Err(DbError::query("Migration ID must contain 1..255 bytes"));
    }
    let version = version.replace('\'', "''");
    let text = format!("{text}\n; CREATE TABLE IF NOT EXISTS axdb_migration_history (version VARCHAR(255) PRIMARY KEY); INSERT INTO axdb_migration_history(version) VALUES ('{version}');");
    let req = request(text);
    let future = r.session.apply_migration(&req, run.token.clone());
    tokio::pin!(future);
    let result = match tokio::time::timeout(Duration::from_secs(30), &mut future).await {
        Ok(r) => r,
        Err(_) => {
            run.token.cancel();
            let _ = future.await;
            Err(DbError::new(ErrorKind::Timeout, "Migration timed out"))
        }
    };
    let result = host.observe(&r, result);
    let _ = host
        .history(HistoryEntry {
            kind: "migration_apply".into(),
            source: HistorySource::default(),
            timestamp: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            profile_id: Some(r.profile.id.clone()),
            profile_name: storage::redact(&r.profile.name, r.secret.as_deref()),
            text: "Approved migration (SQL omitted)".into(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            row_count: 0,
            error: result
                .as_ref()
                .err()
                .map(|_| "Migration failed; verify outcome if connection was lost".into()),
        })
        .await;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_scan_orders_versions_and_rejects_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["V10__last.sql", "V2__second.sql", "V1__first.sql"] {
            std::fs::write(dir.path().join(name), "CREATE TABLE example (id int)").unwrap();
        }
        std::fs::create_dir_all(dir.path().join("migrations/20261019_init")).unwrap();
        std::fs::write(
            dir.path().join("migrations/20261019_init/migration.sql"),
            "SELECT 1",
        )
        .unwrap();
        let list = scan(dir.path()).unwrap();
        assert_eq!(
            list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["1", "2", "10", "20261019_init"]
        );
        std::fs::write(dir.path().join("V2__duplicate.sql"), "SELECT 1").unwrap();
        assert!(scan(dir.path()).is_err());
        assert!(candidate(Path::new("V__empty.sql")).is_none());
        assert!(candidate(Path::new("V1..2__invalid.sql")).is_none());
    }
    #[test]
    fn formats_and_transaction_escape() {
        assert_eq!(candidate(Path::new("V1.2__init.sql")).unwrap().1, "flyway");
        assert_eq!(
            candidate(Path::new("20261019_init/migration.sql"))
                .unwrap()
                .1,
            "prisma"
        );
        assert_eq!(candidate(Path::new("001_create.sql")).unwrap().0, "001");
        assert!(validate("CREATE TABLE x(i int); COMMIT; DELETE FROM y").is_err());
        assert!(validate("ALTER TABLE x DROP COLUMN i").unwrap());
        assert!(!validate("CREATE TABLE x(i int)").unwrap());
    }
}
