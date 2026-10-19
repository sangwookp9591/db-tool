//! Installable MySQL/MariaDB driver. The desktop host has no MySQL dependency.
mod administration;
mod catalog;
mod management;
mod value;
use async_trait::async_trait;
use dbcore::*;
use mysql_async::{prelude::*, Conn, Opts, OptsBuilder, Params, SslOpts};
use sqlparser::{ast::Statement, dialect::MySqlDialect, parser::Parser};
use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub struct MysqlDriver {
    manifest: DriverManifest,
}
impl Default for MysqlDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl MysqlDriver {
    pub fn new() -> Self {
        let fields = [
            ("host", "Host", FieldKind::Text, Some("localhost"), false),
            ("port", "Port", FieldKind::Number, Some("3306"), false),
            ("database", "Database", FieldKind::Text, None, false),
            ("user", "User", FieldKind::Text, Some("root"), false),
            ("password", "Password", FieldKind::Password, None, true),
            (
                "sslmode",
                "TLS",
                FieldKind::Select {
                    options: ["disable", "require", "verify-full"]
                        .map(str::to_owned)
                        .into(),
                },
                Some("verify-full"),
                false,
            ),
            (
                "sslrootcert",
                "TLS CA file",
                FieldKind::FilePath,
                None,
                false,
            ),
        ];
        Self {
            manifest: DriverManifest {
                id: "mysql".into(),
                icon_key: Some("mysql".into()),
                display_name: "MySQL / MariaDB".into(),
                paradigm: Paradigm::Relational,
                query_language: QueryLanguage {
                    id: "sql".into(),
                    display: "SQL (? or :name parameters)".into(),
                    editor_mode: "sql".into(),
                    supports_params: true,
                },
                connection_fields: fields
                    .into_iter()
                    .map(|(key, label, kind, default, secret)| FieldSpec {
                        key: key.into(),
                        label: label.into(),
                        kind,
                        required: matches!(key, "host" | "port" | "user" | "sslmode"),
                        default: default.map(str::to_owned),
                        secret,
                        placeholder: String::new(),
                    })
                    .collect(),
                capabilities: Capabilities {
                    backup: true,
                    roles: true,
                    import_rows: true,
                    schema_snapshot: true,
                    ddl_render: true,
                    explain: true,
                    lint: true,
                    schema_graph: true,
                    filtered_browse: true,
                    preview_changes: PreviewSupport::Transactional,
                    read_only_exec: true,
                    cancel: CancelSupport::Server,
                    timeout: true,
                    params: true,
                    transactions: true,
                    paged_browse: true,
                    graph_expand: false,
                    multi_statement: true,
                    edit_rows: true,
                },
                default_query: "SELECT VERSION(), DATABASE();".into(),
            },
        }
    }
}
fn options(cfg: &ConnectConfig) -> Result<Opts, DbError> {
    let get = |key: &str, default: &str| {
        cfg.fields
            .get(key)
            .map(String::as_str)
            .unwrap_or(default)
            .to_owned()
    };
    let host = get("host", "localhost");
    let user = get("user", "root");
    if host.is_empty() || user.is_empty() || host.contains('\0') || user.contains('\0') {
        return Err(DbError::query("Host and user are required"));
    }
    let port = get("port", "3306")
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| DbError::query("Invalid MySQL port"))?;
    let mode = get("sslmode", "verify-full");
    let ssl = match mode.as_str() {
        "disable" => None,
        "require" | "verify-full" => {
            let mut ssl = SslOpts::default()
                .with_danger_accept_invalid_certs(mode == "require")
                .with_danger_skip_domain_validation(mode != "verify-full");
            if let Some(name) = cfg.fields.get("tls_server_name").filter(|s| !s.is_empty()) {
                ssl = ssl.with_danger_tls_hostname_override(Some(name.clone()));
            }
            if let Some(path) = cfg.fields.get("sslrootcert").filter(|p| !p.is_empty()) {
                ssl = ssl.with_root_certs(vec![std::path::PathBuf::from(path).into()]);
            }
            Some(ssl)
        }
        _ => return Err(DbError::query("Invalid MySQL TLS mode")),
    };
    Ok(OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port)
        .user(Some(user))
        .pass(cfg.secret.clone())
        .db_name(
            cfg.fields
                .get("database")
                .filter(|s| !s.is_empty())
                .cloned(),
        )
        .prefer_socket(false)
        .ssl_opts(ssl)
        .init(vec![
            "SET SESSION time_zone = '+00:00'",
            "SET NAMES utf8mb4",
        ])
        .into())
}
#[async_trait]
impl Driver for MysqlDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        let opts = options(cfg)?;
        if opts.ssl_opts().is_some() {
            // Workspace feature unification can enable two providers; preserve an existing choice.
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let mut conn = tokio::time::timeout(
            Duration::from_millis(cfg.connect_timeout_ms),
            Conn::new(opts.clone()),
        )
        .await
        .map_err(|_| DbError::new(ErrorKind::Timeout, "MySQL connection timed out"))?
        .map_err(error)?;
        let version: String = conn
            .query_first("SELECT VERSION()")
            .await
            .map_err(error)?
            .ok_or_else(|| DbError::query("Missing MySQL version"))?;
        let maria = version.contains("MariaDB");
        Ok(Box::new(MysqlSession {
            conn: Mutex::new(Some(conn)),
            opts,
            maria,
            version,
            closed: AtomicBool::new(false),
        }))
    }
}
static PROBE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
struct MysqlSession {
    conn: Mutex<Option<Conn>>,
    opts: Opts,
    maria: bool,
    version: String,
    closed: AtomicBool,
}
fn error(e: mysql_async::Error) -> DbError {
    match e {
        mysql_async::Error::Server(e) => DbError::new(
            match e.code {
                1044 | 1045 | 1142 | 1227 => ErrorKind::Auth,
                1317 => ErrorKind::Cancelled,
                3024 | 1969 => ErrorKind::Timeout,
                _ => ErrorKind::Query,
            },
            format!("MySQL server error {} (SQLSTATE {})", e.code, e.state),
        ),
        _ => DbError::new(
            ErrorKind::Connection,
            "MySQL connection or TLS handshake failed",
        ),
    }
}
fn active(conn: &mut Option<Conn>) -> Result<&mut Conn, DbError> {
    conn.as_mut()
        .ok_or_else(|| DbError::new(ErrorKind::Connection, "MySQL session is closed"))
}

fn quote(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}
fn qualified(db: &str, table: &str) -> String {
    format!("{}.{}", quote(db), quote(table))
}
fn node_id(parts: &[&str]) -> String {
    serde_json::to_string(parts).expect("string array")
}
fn table_path(id: &str) -> Result<(String, String), DbError> {
    let parts: Vec<String> =
        serde_json::from_str(id).map_err(|_| DbError::query("Invalid MySQL node"))?;
    match parts.as_slice() {
        [tag, db, name] if tag == "table" => Ok((db.clone(), name.clone())),
        _ => Err(DbError::query("Expected a MySQL table node")),
    }
}
fn parsed(sql: &str) -> Result<Vec<Statement>, DbError> {
    // Server-executed comments and changing lexical SQL modes must not bypass preflight.
    if sql.contains("/*!") || sql.contains("/*M!") || sql.contains("/*m!") {
        return Err(DbError::unsupported(
            "Executable comments are not allowed in protected operations",
        ));
    }
    Parser::parse_sql(&MySqlDialect {}, sql)
        .map_err(|_| DbError::query("Protected operation requires valid MySQL SQL"))
}
fn named_params(req: &ExecRequest) -> Result<Params, DbError> {
    if req.params.is_empty() {
        return Ok(Params::Empty);
    }
    if req.params.keys().all(|k| k.parse::<usize>().is_ok()) {
        let values = (1..=req.params.len())
            .map(|i| {
                req.params
                    .get(&i.to_string())
                    .ok_or_else(|| DbError::query("Positional parameter keys must be 1..N"))
                    .and_then(value::parameter)
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Params::Positional(values));
    }
    let mut params = std::collections::HashMap::new();
    for (name, v) in &req.params {
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(DbError::query("Invalid parameter name"));
        }
        params.insert(name.as_bytes().to_vec(), value::parameter(v)?);
    }
    Ok(Params::Named(params))
}
impl MysqlSession {
    async fn fresh(&self) -> Result<Conn, DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "MySQL session is closed",
            ));
        }
        tokio::time::timeout(Duration::from_secs(5), Conn::new(self.opts.clone()))
            .await
            .map_err(|_| DbError::new(ErrorKind::Timeout, "MySQL operation connection timed out"))?
            .map_err(error)
    }
    async fn operation_connection(&self, state: &mut Option<Conn>) -> Result<Conn, DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "MySQL session is closed",
            ));
        }
        let main = active(state)?;
        let autocommit: Option<u8> = main
            .query_first("SELECT @@session.autocommit")
            .await
            .map_err(error)?;
        if autocommit != Some(1) {
            return Err(DbError::unsupported(
                "Finish the current transaction and enable autocommit before a protected operation",
            ));
        }
        // RELEASE detects an explicit empty transaction; @@autocommit alone cannot.
        let name = format!(
            "axdb_probe_{}_{}_{}",
            main.id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            PROBE_ID.fetch_add(1, Ordering::Relaxed)
        );
        main.query_drop(format!("SAVEPOINT {name}"))
            .await
            .map_err(error)?;
        match main.query_drop(format!("RELEASE SAVEPOINT {name}")).await {
            Ok(()) => {
                return Err(DbError::unsupported(
                    "Finish the current transaction before a protected operation",
                ))
            }
            Err(mysql_async::Error::Server(e)) if e.code == 1305 => {}
            Err(e) => return Err(error(e)),
        }
        let db = main
            .query_first::<Option<String>, _>("SELECT DATABASE()")
            .await
            .map_err(error)?
            .flatten();
        let opts = OptsBuilder::from_opts(self.opts.clone()).db_name(db);
        tokio::time::timeout(Duration::from_secs(5), Conn::new(opts))
            .await
            .map_err(|_| DbError::new(ErrorKind::Timeout, "MySQL protected connection timed out"))?
            .map_err(error)
    }
    async fn guarded<T>(
        &self,
        id: u32,
        cancel: CancelToken,
        timeout: u64,
        work: impl Future<Output = Result<T, DbError>>,
    ) -> Result<T, DbError> {
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "MySQL operation cancelled",
            ));
        }
        tokio::pin!(work);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout.max(1));
        let reason = tokio::select! { biased;
            _=cancel.cancelled()=>ErrorKind::Cancelled,
            _=tokio::time::sleep_until(deadline)=>ErrorKind::Timeout,
            r=&mut work=>return if cancel.is_cancelled(){Err(DbError::new(ErrorKind::Cancelled,"MySQL operation cancelled"))}else if tokio::time::Instant::now()>=deadline{Err(DbError::new(ErrorKind::Timeout,"MySQL operation timed out"))}else{r},
        };
        // Keep the query future alive until the separate connection confirms its cancellation.
        let stop = async {
            let mut killer = Conn::new(self.opts.clone()).await.map_err(error)?;
            let kill = killer
                .query_drop(format!("KILL QUERY {id}"))
                .await
                .map_err(error);
            let _ = killer.disconnect().await;
            kill?;
            let _ = (&mut work).await;
            Ok::<_, DbError>(())
        };
        if !matches!(
            tokio::time::timeout(Duration::from_secs(3), stop).await,
            Ok(Ok(()))
        ) {
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "MySQL cancellation could not be confirmed; session closed",
            ));
        }
        Err(DbError::new(
            reason,
            "MySQL operation cancelled or timed out",
        ))
    }
    async fn set_timeout(&self, conn: &mut Conn, ms: u64) -> Result<(), DbError> {
        let setting = if self.maria {
            format!("SET SESSION max_statement_time={}", ms as f64 / 1000.0)
        } else {
            format!("SET SESSION max_execution_time={}", ms)
        };
        match tokio::time::timeout(Duration::from_secs(3), conn.query_drop(setting)).await {
            Ok(result) => result.map_err(error),
            Err(_) => {
                self.closed.store(true, Ordering::Release);
                Err(DbError::new(
                    ErrorKind::Connection,
                    "MySQL session setup timed out; session closed",
                ))
            }
        }
    }
    async fn cleanup(&self, conn: &mut Conn, sql: &str) -> Result<(), DbError> {
        if !matches!(
            tokio::time::timeout(Duration::from_secs(3), conn.query_drop(sql)).await,
            Ok(Ok(()))
        ) {
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "MySQL cleanup failed; session closed",
            ));
        }
        Ok(())
    }
}
async fn collect<P: Protocol>(
    mut result: mysql_async::QueryResult<'_, '_, P>,
    limit: usize,
    title: &str,
    utc: bool,
) -> Result<Vec<ResultSet>, DbError> {
    let mut out = vec![];
    loop {
        let columns = result.columns_ref().to_vec();
        let affected = result.affected_rows();
        let mut rows = vec![];
        let mut truncated = false;
        while let Some(row) = result.next().await.map_err(error)? {
            if rows.len() < limit {
                rows.push(
                    columns
                        .iter()
                        .enumerate()
                        .map(|(i, c)| -> Result<Value, DbError> {
                            let value = value::cell(c, row.as_ref(i).expect("column width"))?;
                            Ok(match value {
                                Value::DateTimeTz(text) if !utc => {
                                    Value::DateTime(text.trim_end_matches("+00:00").to_owned())
                                }
                                v => v,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            } else {
                truncated = true;
            }
        }
        out.push(ResultSet {
            title: title.chars().take(80).collect(),
            row_count: rows.len() as u64,
            truncated,
            table: if columns.is_empty() {
                None
            } else {
                Some(Table {
                    columns: columns
                        .iter()
                        .map(|c| Column {
                            name: c.name_str().into_owned(),
                            type_name: if utc {
                                value::type_name(c)
                            } else {
                                value::type_name(c)
                                    .replace("TIMESTAMP (UTC)", "TIMESTAMP (session time zone)")
                            },
                        })
                        .collect(),
                    rows,
                })
            },
            affected: if columns.is_empty() {
                Some(affected)
            } else {
                None
            },
            ..Default::default()
        });
        if result.columns().is_none() {
            result.drop_result().await.map_err(error)?;
            break;
        }
    }
    Ok(out)
}
async fn query(conn: &mut Conn, req: &ExecRequest, utc: bool) -> Result<Vec<ResultSet>, DbError> {
    let limit = clamp_row_limit(req.row_limit) as usize;
    if req.params.is_empty() {
        collect(
            conn.query_iter(&req.text).await.map_err(error)?,
            limit,
            &req.text,
            utc,
        )
        .await
    } else {
        collect(
            conn.exec_iter(&req.text, named_params(req)?)
                .await
                .map_err(error)?,
            limit,
            &req.text,
            utc,
        )
        .await
    }
}
#[async_trait]
impl Session for MysqlSession {
    async fn ping(&self) -> Result<(), DbError> {
        let mut state = self.conn.lock().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "MySQL session is closed",
            ));
        }
        tokio::time::timeout(Duration::from_secs(5), active(&mut state)?.ping())
            .await
            .map_err(|_| DbError::new(ErrorKind::Timeout, "MySQL ping timed out"))?
            .map_err(error)
    }
    async fn server_info(&self) -> Result<String, DbError> {
        Ok(self.version.clone())
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "MySQL query cancelled"));
        }
        let mut state = tokio::select! {s=self.conn.lock()=>s,_=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"MySQL query cancelled"))};
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "MySQL session is closed",
            ));
        }
        let start = Instant::now();
        let mut results = if req.read_only {
            let ast = parsed(&req.text)?;
            if ast.is_empty() || ast.iter().any(|s| !matches!(s, Statement::Query(_))) {
                return Err(DbError::new(
                    ErrorKind::Auth,
                    "Read-only execution accepts SELECT without INTO only",
                ));
            }
            management::allowed(&ast)?;
            let mut conn = self.operation_connection(&mut state).await?;
            self.set_timeout(&mut conn, req.timeout_ms).await?;
            conn.query_drop("SET SESSION TRANSACTION READ ONLY")
                .await
                .map_err(error)?;
            conn.query_drop("START TRANSACTION READ ONLY")
                .await
                .map_err(error)?;
            let r = self
                .guarded(
                    conn.id(),
                    cancel,
                    req.timeout_ms,
                    query(&mut conn, req, true),
                )
                .await;
            self.cleanup(&mut conn, "ROLLBACK").await?;
            r?
        } else {
            let conn = active(&mut state)?;
            if req
                .params
                .values()
                .any(|p| p.get("t").and_then(serde_json::Value::as_str) == Some("DateTimeTz"))
            {
                let zone: Option<String> = conn
                    .query_first("SELECT @@session.time_zone")
                    .await
                    .map_err(error)?;
                if !matches!(zone.as_deref(), Some("+00:00" | "UTC")) {
                    return Err(DbError::query("Tagged timestamp parameters require a UTC session; reset time_zone before executing"));
                }
            }
            self.set_timeout(conn, req.timeout_ms).await?;
            let r = self
                .guarded(conn.id(), cancel, req.timeout_ms, query(conn, req, false))
                .await;
            if !self.closed.load(Ordering::Acquire) {
                self.set_timeout(conn, 0).await?;
            }
            r?
        };
        for result in &mut results {
            result.elapsed_ms = start.elapsed().as_millis() as u64;
        }
        Ok(results)
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        self.children(parent).await
    }
    async fn browse(&self, node: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        self.browse_table(node, page).await
    }
    async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> {
        self.graph(scope).await
    }
    async fn preview(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        self.preview_sql(req, cancel).await
    }
    async fn preview_edits(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        self.edit_preview(req, cancel).await
    }
    async fn apply_edits(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
    ) -> Result<EditResult, DbError> {
        self.edit_apply(req, cancel).await
    }
    async fn import_rows(
        &self,
        req: &EditRequest,
        preview: bool,
        cancel: CancelToken,
    ) -> Result<ImportOutput, DbError> {
        if req.edits.is_empty()
            || req.edits.len() > IMPORT_MAX_ROWS
            || req
                .edits
                .iter()
                .any(|e| !matches!(e, RowEdit::Insert { .. }))
        {
            return Err(DbError::query("Import requires 1..10000 inserts"));
        }
        if preview {
            Ok(ImportOutput::Preview(self.edit_preview(req, cancel).await?))
        } else {
            Ok(ImportOutput::Applied(self.edit_apply(req, cancel).await?))
        }
    }
    async fn schema_snapshot(&self, scope: Option<&str>) -> Result<SchemaSnapshot, DbError> {
        self.snapshot(scope).await
    }
    async fn ddl_render(&self, diff: &SnapshotDiff) -> Result<Vec<String>, DbError> {
        management::ddl(diff)
    }
    async fn explain(
        &self,
        sql: &str,
        analyze: bool,
        cancel: CancelToken,
    ) -> Result<PlanTree, DbError> {
        self.plan(sql, analyze, cancel).await
    }
    async fn roles(&self) -> Result<RolesSnapshot, DbError> {
        self.load_roles().await
    }
    async fn plan_grants(&self, changes: &[GrantChange]) -> Result<GrantPlan, DbError> {
        self.grants(changes).await
    }
    async fn restore_options(&self) -> Result<RestoreOptions, DbError> {
        Ok(RestoreOptions{overwrite:false,new_database:false,guidance:"MySQL logical backup export only; restore requires an explicit reviewed migration in this driver version".into()})
    }
    async fn backup_stream(
        &self,
        cancel: CancelToken,
        sink: &dyn BackupSink,
    ) -> Result<BackupSummary, DbError> {
        self.dump(cancel, sink).await
    }
    async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> {
        Err(DbError::unsupported("MySQL has no graph expansion"))
    }
    async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Some(conn) = self.conn.lock().await.take() {
            let _ = tokio::time::timeout(Duration::from_secs(3), conn.disconnect()).await;
        }
    }
}
