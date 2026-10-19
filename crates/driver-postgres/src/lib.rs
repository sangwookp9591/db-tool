//! PostgreSQL text-protocol driver; no dependency on the desktop host.
mod preview;
mod edits;
mod schema;
mod split;
use async_trait::async_trait;
use dbcore::*;
use futures_util::{pin_mut, StreamExt};
use postgres_native_tls::MakeTlsConnector;
use preview::RunOutput;
pub use split::split_statements;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tokio_postgres::{
    config::SslMode, error::ErrorPosition, types::Type, Client, SimpleQueryMessage,
};

pub struct PostgresDriver {
    manifest: DriverManifest,
}
impl Default for PostgresDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl PostgresDriver {
    pub fn new() -> Self {
        let fields = [
            ("host", "Host", FieldKind::Text, Some("localhost"), false),
            ("port", "Port", FieldKind::Number, Some("5432"), false),
            (
                "database",
                "데이터베이스",
                FieldKind::Text,
                Some("postgres"),
                false,
            ),
            ("user", "사용자", FieldKind::Text, Some("postgres"), false),
            ("password", "비밀번호", FieldKind::Password, None, true),
            (
                "sslmode",
                "TLS 모드",
                FieldKind::Select {
                    options: ["disable", "prefer", "require", "verify-ca", "verify-full"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                },
                Some("prefer"),
                false,
            ),
            (
                "sslrootcert",
                "TLS CA 파일",
                FieldKind::FilePath,
                None,
                false,
            ),
        ];
        Self {
            manifest: DriverManifest {
                id: "postgres".into(),
                display_name: "PostgreSQL".into(),
                paradigm: Paradigm::Relational,
                query_language: QueryLanguage {
                    id: "sql".into(),
                    display: "SQL".into(),
                    editor_mode: "sql".into(),
                    supports_params: false,
                },
                connection_fields: fields
                    .into_iter()
                    .map(|(key, label, kind, default, secret)| FieldSpec {
                        key: key.into(),
                        label: label.into(),
                        kind,
                        required: !secret && key != "sslrootcert",
                        default: default.map(str::to_string),
                        secret,
                        placeholder: String::new(),
                    })
                    .collect(),
                capabilities: Capabilities {
                    backup: true, roles: true,
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
                    params: false,
                    transactions: true,
                    paged_browse: true,
                    graph_expand: false,
                    multi_statement: true,
                    edit_rows: true,
                },
                default_query: "SELECT current_database(), version();".into(),
            },
        }
    }
}
#[async_trait]
impl Driver for PostgresDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        let field = |key: &str, default: &str| {
            cfg.fields
                .get(key)
                .cloned()
                .unwrap_or_else(|| default.into())
        };
        let mut pg = tokio_postgres::Config::new();
        pg.host(field("tls_server_name", &field("host", "localhost")))
            .port(
                field("port", "5432")
                    .parse::<u16>()
                    .map_err(|_| DbError::new(ErrorKind::Connection, "Invalid port"))?,
            )
            .dbname(field("database", "postgres"))
            .user(field("user", "postgres"))
            .connect_timeout(Duration::from_millis(cfg.connect_timeout_ms.max(1)));
        if cfg.fields.contains_key("tls_server_name") {
            // Keep certificate verification bound to the destination behind the local relay.
            let address = field("host", "localhost").parse::<std::net::IpAddr>()
                .map_err(|_| DbError::new(ErrorKind::Connection, "Invalid tunnel address"))?;
            if !address.is_loopback() { return Err(DbError::new(ErrorKind::Connection, "Tunnel must use loopback")); }
            pg.hostaddr(address);
        }
        if let Some(secret) = &cfg.secret {
            pg.password(secret);
        }
        let mode = field("sslmode", "prefer");
        pg.ssl_mode(match mode.as_str() {
            "disable" => SslMode::Disable,
            "prefer" => SslMode::Prefer,
            "require" | "verify-ca" | "verify-full" => SslMode::Require,
            _ => return Err(DbError::new(ErrorKind::Connection, "Invalid TLS mode")),
        });
        let root = cfg.fields.get("sslrootcert").filter(|s| !s.is_empty());
        let verify = matches!(mode.as_str(), "verify-ca" | "verify-full")
            || (mode == "require" && root.is_some());
        let mut connector = native_tls::TlsConnector::builder();
        // libpq require encrypts without verification unless an explicit CA is supplied.
        connector.danger_accept_invalid_certs(!verify);
        connector.danger_accept_invalid_hostnames(mode != "verify-full");
        if mode != "disable" {
            if let Some(root) = root {
                let pem = tokio::fs::read(root)
                    .await
                    .map_err(|_| DbError::new(ErrorKind::Connection, "Cannot read TLS CA file"))?;
                let certificates = native_tls::Certificate::stack_from_pem(&pem)
                    .map_err(|_| DbError::new(ErrorKind::Connection, "Invalid TLS CA PEM file"))?;
                if certificates.is_empty() {
                    return Err(DbError::new(
                        ErrorKind::Connection,
                        "TLS CA file contains no certificates",
                    ));
                }
                connector.disable_built_in_roots(true);
                for certificate in certificates {
                    connector.add_root_certificate(certificate);
                }
            }
        }
        let tls = MakeTlsConnector::new(
            connector
                .build()
                .map_err(|_| DbError::new(ErrorKind::Connection, "TLS initialization failed"))?,
        );
        let (client, connection) = tokio::time::timeout(
            Duration::from_millis(cfg.connect_timeout_ms.max(1)),
            pg.connect(tls.clone()),
        )
        .await
        .map_err(|_| DbError::new(ErrorKind::Connection, "Connection timed out"))?
        .map_err(|e| {
            let mut error = pg_error(e, cfg.secret.as_deref());
            if error.kind == ErrorKind::Query {
                error.kind = ErrorKind::Connection;
            }
            error
        })?;
        let task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let session = PgSession {
            client,
            tls,
            cancel_address: (field("host", "localhost"), pg.get_ports()[0]),
            tls_server_name: field("tls_server_name", &field("host", "localhost")),
            state: Mutex::new(TxState::default()),
            closed: AtomicBool::new(false),
            connection: task.abort_handle(),
            secret: cfg.secret.clone(),
        };
        // Stable text rendering avoids bytea escape mode and session date format surprises.
        session.client.batch_execute("SET bytea_output = 'hex'; SET datestyle = 'ISO, YMD'; SET standard_conforming_strings = on").await.map_err(|e| session.error(e))?;
        Ok(Box::new(session))
    }
}
#[derive(Default)]
struct TxState {
    explicit: bool,
}
pub struct PgSession {
    client: Client,
    tls: MakeTlsConnector,
    cancel_address: (String, u16),
    tls_server_name: String,
    state: Mutex<TxState>,
    closed: AtomicBool,
    connection: tokio::task::AbortHandle,
    secret: Option<String>,
}
impl Drop for PgSession {
    fn drop(&mut self) {
        self.connection.abort();
    }
}
impl PgSession {
    fn error(&self, e: tokio_postgres::Error) -> DbError {
        pg_error(e, self.secret.as_deref())
    }
    async fn cancel_and_wait(&self) -> Result<(), DbError> {
        use tokio::io::AsyncReadExt;
        use tokio_postgres::tls::MakeTlsConnect;
        let (host, port) = &self.cancel_address;
        let socket = tokio::net::TcpStream::connect((host.as_str(), *port))
            .await
            .map_err(|_| DbError::internal("Cannot connect cancellation socket"))?;
        let raw = socket
            .into_std()
            .map_err(|_| DbError::internal("Cannot retain cancellation socket"))?;
        let mut acknowledgement = tokio::net::TcpStream::from_std(
            raw.try_clone()
                .map_err(|_| DbError::internal("Cannot retain cancellation socket"))?,
        )
        .map_err(|_| DbError::internal("Cannot retain cancellation socket"))?;
        let socket = tokio::net::TcpStream::from_std(raw)
            .map_err(|_| DbError::internal("Cannot restore cancellation socket"))?;
        let tls = <MakeTlsConnector as MakeTlsConnect<tokio::net::TcpStream>>::make_tls_connect(
            &mut self.tls.clone(),
            &self.tls_server_name,
        )
        .map_err(|_| DbError::internal("Cannot initialize cancellation TLS"))?;
        self.client
            .cancel_token()
            .cancel_query_raw(socket, tls)
            .await
            .map_err(|e| self.error(e))?;
        // PostgreSQL closes this socket after processing CancelRequest; flush alone is not an acknowledgement.
        let mut byte = [0];
        while acknowledgement
            .read(&mut byte)
            .await
            .map_err(|_| DbError::internal("Cannot await cancellation"))?
            != 0
        {}
        Ok(())
    }
    async fn run(
        &self,
        req: &ExecRequest,
        state: &mut TxState,
        cancel: &CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        if !req.params.is_empty() {
            return Err(DbError::unsupported(
                "PostgreSQL 파라미터는 아직 지원하지 않습니다",
            ));
        }
        let mut results = Vec::new();
        for sql in split_statements(&req.text)? {
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
            }
            let start = Instant::now();
            let words = split::keywords(&sql);
            let first = words.first().map(String::as_str).unwrap_or("");
            // COPY requires a separate streaming API; rejecting prevents a session becoming stuck in COPY mode.
            if first == "COPY" {
                return Err(DbError::unsupported("COPY streaming is not supported"));
            }
            let statement = self.client.prepare(&sql).await.map_err(|e| self.error(e))?;
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
            }
            let mut result = if statement.columns().is_empty() {
                let messages = self
                    .client
                    .simple_query(&sql)
                    .await
                    .map_err(|e| self.error(e))?;
                let affected = messages
                    .iter()
                    .find_map(|m| match m {
                        SimpleQueryMessage::CommandComplete(n) => Some(*n),
                        _ => None,
                    })
                    .unwrap_or(0);
                match first {
                    "BEGIN" | "START" => state.explicit = true,
                    "COMMIT" | "END" | "ABORT" => {
                        state.explicit = words.windows(2).any(|w| w == ["AND", "CHAIN"])
                    }
                    "ROLLBACK" if !words.iter().any(|w| w == "TO") => {
                        state.explicit = words.windows(2).any(|w| w == ["AND", "CHAIN"])
                    }
                    _ => {}
                }
                ResultSet {
                    message: Some(format!("{first} {affected}")),
                    affected: Some(affected),
                    ..Default::default()
                }
            } else {
                let types = statement
                    .columns()
                    .iter()
                    .map(|c| c.type_().clone())
                    .collect::<Vec<_>>();
                let columns = statement
                    .columns()
                    .iter()
                    .map(|c| Column {
                        name: c.name().into(),
                        type_name: c.type_().name().into(),
                    })
                    .collect();
                let limit = clamp_row_limit(req.row_limit) as usize;
                let cursorable = matches!(first, "SELECT" | "WITH" | "VALUES" | "TABLE");
                let rows = if cursorable {
                    self.cursor_rows(&sql, limit, state.explicit, cancel)
                        .await?
                } else {
                    self.stream_rows(&sql, limit).await?
                };
                let truncated = rows.len() > limit;
                let rows = rows
                    .into_iter()
                    .take(limit)
                    .map(|row| {
                        (0..types.len())
                            .map(|i| map_value(&types[i], row.get(i)))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                ResultSet {
                    row_count: rows.len() as u64,
                    table: Some(Table { columns, rows }),
                    truncated,
                    ..Default::default()
                }
            };
            result.title = result_title(sql.lines().next().unwrap_or(""));
            result.elapsed_ms = start.elapsed().as_millis() as u64;
            results.push(result);
        }
        Ok(results)
    }
    async fn cursor_rows(
        &self,
        sql: &str,
        limit: usize,
        explicit: bool,
        cancel: &CancelToken,
    ) -> Result<Vec<tokio_postgres::SimpleQueryRow>, DbError> {
        if !explicit {
            self.client
                .batch_execute("BEGIN")
                .await
                .map_err(|e| self.error(e))?;
        }
        if cancel.is_cancelled() {
            if !explicit {
                let _ = self.client.batch_execute("ROLLBACK").await;
            }
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let cursor = format!("axdb_{}", uuid_free_id());
        let query = format!(
            "DECLARE {cursor} NO SCROLL CURSOR FOR {sql}\n; FETCH {} FROM {cursor}; CLOSE {cursor};",
            limit + 1
        );
        let result = self.client.simple_query(&query).await;
        match result {
            Ok(messages) => {
                if !explicit {
                    self.client
                        .batch_execute("COMMIT")
                        .await
                        .map_err(|e| self.error(e))?;
                }
                Ok(messages
                    .into_iter()
                    .filter_map(|m| match m {
                        SimpleQueryMessage::Row(r) => Some(r),
                        _ => None,
                    })
                    .collect())
            }
            Err(e) => {
                if !explicit {
                    let _ = self.client.batch_execute("ROLLBACK").await;
                }
                let mut error = self.error(e);
                if let Some(pos) = error.position {
                    error.position = Some(
                        pos.saturating_sub(
                            format!("DECLARE {cursor} NO SCROLL CURSOR FOR ")
                                .chars()
                                .count() as u32,
                        )
                        .max(1),
                    );
                }
                Err(error)
            }
        }
    }
    async fn stream_rows(
        &self,
        sql: &str,
        limit: usize,
    ) -> Result<Vec<tokio_postgres::SimpleQueryRow>, DbError> {
        // shortcut: PG forbids cursors for DML RETURNING/SHOW/EXPLAIN; stream and drain, revisit for protocol-level row limits.
        let stream = self
            .client
            .simple_query_raw(sql)
            .await
            .map_err(|e| self.error(e))?;
        pin_mut!(stream);
        let mut rows = Vec::new();
        while let Some(message) = stream.next().await {
            if let SimpleQueryMessage::Row(row) = message.map_err(|e| self.error(e))? {
                if rows.len() <= limit {
                    rows.push(row);
                }
            }
        }
        Ok(rows)
    }
    async fn run_guarded(
        &self,
        req: &ExecRequest,
        state: &mut TxState,
        cancel: &CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        if !req.read_only {
            return self.run(req, state, cancel).await;
        }
        if state.explicit {
            return Err(DbError::unsupported("Read-only agent execution is unavailable inside a user transaction; finish it first"));
        }
        // Prevent SQL from ending or reconfiguring the host-owned read-only transaction.
        for sql in split_statements(&req.text)? {
            let words = split::keywords(&sql);
            if matches!(
                words.first().map(String::as_str),
                Some(
                    "BEGIN"
                        | "START"
                        | "COMMIT"
                        | "END"
                        | "ROLLBACK"
                        | "ABORT"
                        | "SAVEPOINT"
                        | "RELEASE"
                        | "SET"
                        | "RESET"
                        | "DISCARD"
                        | "PREPARE"
                        | "EXECUTE"
                )
            ) {
                return Err(DbError::unsupported(
                    "Transaction and session control is unavailable in read-only execution",
                ));
            }
        }
        // Acquire a snapshot before untrusted SQL so even DO/set_config cannot switch to READ WRITE.
        state.explicit = true;
        let begin = self
            .client
            .batch_execute("BEGIN READ ONLY; SELECT 1")
            .await
            .map_err(|e| self.error(e));
        let result = match begin {
            Ok(()) => self.run(req, state, cancel).await,
            Err(error) => Err(error),
        };
        let cleanup = self
            .client
            .batch_execute("ROLLBACK")
            .await
            .map_err(|e| self.error(e));
        state.explicit = false;
        if cleanup.is_err() {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
        }
        cleanup?;
        result
    }
    async fn execute_cancelled(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
        preview: bool,
        edits: Option<(&EditRequest, bool)>,
    ) -> Result<RunOutput, DbError> {
        let deadline = tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)));
        tokio::pin!(deadline);
        let mut state = tokio::select! { biased; _=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"실행이 취소되었습니다")), _=&mut deadline=>return Err(DbError::new(ErrorKind::Timeout,"Execution timed out while waiting for session")), state=self.state.lock()=>state };
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let mut sent_cancel = false;
        let kind = {
            let future = async {
                if let Some((edit, import)) = edits {
                    if import && state.explicit { return Err(DbError::unsupported("Finish the existing transaction before importing")); }
                    self.edit_run(edit, &mut state, &cancel, preview).await
                } else if preview {
                    self.preview_run(req, &mut state, &cancel)
                        .await
                        .map(RunOutput::Preview)
                } else {
                    self.run_guarded(req, &mut state, &cancel)
                        .await
                        .map(RunOutput::Execute)
                }
            };
            tokio::pin!(future);
            let kind = tokio::select! {
                biased;
                _=cancel.cancelled()=>ErrorKind::Cancelled,
                _=&mut deadline=>ErrorKind::Timeout,
                result=&mut future=>return result,
            };
            cancel.cancel();
            // Retry across protocol boundaries: a cancel that arrives while the backend is idle is ignored.
            let mut retry = tokio::time::interval(Duration::from_millis(50));
            loop {
                tokio::select! {
                    biased;
                    result = &mut future => { let _ = result; break; },
                    _ = retry.tick() => {
                        sent_cancel = true;
                        if !matches!(tokio::time::timeout(Duration::from_secs(3), self.cancel_and_wait()).await, Ok(Ok(()))) {
                            self.connection.abort();
                            self.closed.store(true, Ordering::Release);
                        }
                    }
                }
            }
            kind
        };
        if sent_cancel && !self.closed.load(Ordering::Acquire) {
            // Drain late cancel responses while this execution still owns the session lock.
            for attempt in 0..3 {
                match self.client.simple_query("SELECT 1").await {
                    Ok(_) => break,
                    Err(e) if e.code().is_some_and(|c| c.code() == "25P02") => {
                        // A late cancel can abort the cursor transaction this driver opened; only a
                        // user's explicit transaction should stay aborted for them to roll back.
                        if !state.explicit {
                            let _ = self.client.batch_execute("ROLLBACK").await;
                        }
                        break;
                    }
                    Err(e) if e.code().is_some_and(|c| c.code() == "57014") && attempt < 2 => {
                        continue
                    }
                    Err(_) => {
                        self.connection.abort();
                        self.closed.store(true, Ordering::Release);
                        break;
                    }
                }
            }
        }
        Err(DbError::new(kind, "Execution interrupted"))
    }
}
fn uuid_free_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
#[async_trait]
impl Session for PgSession {
    async fn preview_grants(&self,changes:&[GrantChange],cancel:CancelToken)->Result<GrantPlan,DbError>{let plan=self.native_grants(changes).await?;let req=ExecRequest{text:plan.statements.join(";\n"),read_only:false,params:Default::default(),row_limit:1,graph_node_limit:1,timeout_ms:30000,preview_sample_limit:1};self.migrate_transaction(&req,cancel,true).await?;Ok(plan)}

    async fn import_rows_progress(&self,req:&EditRequest,preview:bool,cancel:CancelToken,sink:&dyn ProgressSink)->Result<ImportOutput,DbError>{if preview {let output=self.import_rows(req,true,cancel).await?;sink.progress(req.edits.len() as u64,req.edits.len() as u64).await?;Ok(output)}else{self.copy_import(req,cancel,sink).await}}

    async fn apply_grants(&self,changes:&[GrantChange],cancel:CancelToken)->Result<(),DbError>{let plan=self.native_grants(changes).await?;let req=ExecRequest{text:plan.statements.join(";\n"),params:Default::default(),read_only:false,row_limit:1,graph_node_limit:1,timeout_ms:30000,preview_sample_limit:1};self.migrate(&req,cancel).await?;Ok(())}

    async fn backup(&self,cancel:CancelToken)->Result<BackupData,DbError>{self.native_backup(cancel).await}
    async fn restore(&self,backup:&BackupData,overwrite:bool,cancel:CancelToken)->Result<(),DbError>{self.native_restore(backup,overwrite,cancel).await}
    async fn roles(&self)->Result<RolesSnapshot,DbError>{self.native_roles().await}
    async fn plan_grants(&self,changes:&[GrantChange])->Result<GrantPlan,DbError>{self.native_grants(changes).await}

    async fn import_rows(&self, req: &EditRequest, preview: bool, cancel: CancelToken) -> Result<ImportOutput, DbError> {
        if req.edits.is_empty() || req.edits.len() > IMPORT_MAX_ROWS || req.edits.iter().any(|e| !matches!(e, RowEdit::Insert { .. })) {
            return Err(DbError::query("Import requires 1..10000 inserts"));
        }
        let exec = ExecRequest { text: String::new(), read_only: false, params: Default::default(), row_limit: 100, graph_node_limit: 1, timeout_ms: req.timeout_ms, preview_sample_limit: 100 };
        match self.execute_cancelled(&exec, cancel, preview, Some((req, true))).await? {
            RunOutput::Preview(result) => Ok(ImportOutput::Preview(result)),
            RunOutput::Edit(result) => Ok(ImportOutput::Applied(result)),
            _ => Err(DbError::internal("Invalid import result")),
        }
    }
    async fn apply_migration(&self,req:&ExecRequest,cancel:CancelToken)->Result<Vec<ResultSet>,DbError>{self.migrate(req,cancel).await}
    async fn schema_snapshot(&self, scope:Option<&str>)->Result<SchemaSnapshot,DbError>{ self.load_snapshot(scope).await }
    async fn ddl_render(&self,diff:&SnapshotDiff)->Result<Vec<String>,DbError>{dbcore::workbench::render_sql(diff,"postgres")}
    async fn explain(&self, query: &str, analyze: bool, cancel: CancelToken) -> Result<PlanTree, DbError> {
        let statements = split_statements(query)?;
        if statements.len() != 1 { return Err(DbError::query("Explain requires one statement")); }
        let tokens = dbcore::preview::tokens(&statements[0])?;
        if let Some(reason) = dbcore::preview::rejection(&tokens) { return Err(DbError::query(reason)); }
        if !tokens.first().is_some_and(|t| ["SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "MERGE", "VALUES"].iter().any(|w| t.is(w))) {
            return Err(DbError::unsupported("Explain supports queries and transactional DML only"));
        }
        let req = ExecRequest { text: format!("EXPLAIN (FORMAT JSON, VERBOSE TRUE, ANALYZE {}) {}", if analyze { "TRUE" } else { "FALSE" }, statements[0]), params: Default::default(), read_only: false, row_limit: 1, graph_node_limit: 1, timeout_ms: 30000, preview_sample_limit: 1 };
        // The existing preview path owns the session lock, cancellation and rollback cleanup.
        let preview = self.preview(&req, cancel.clone()).await?;
        let raw = preview.statements.first().and_then(|s| s.after.as_ref()).and_then(|t| t.rows.first()).and_then(|r| r.first());
        let Some(Value::Json(raw)) = raw else { return Err(DbError::query("Missing JSON explain plan")); };
        let raw: serde_json::Value = serde_json::from_str(raw).map_err(|_| DbError::query("Invalid JSON explain plan"))?;
        let mut plan = PlanTree::postgres(&raw[0]["Plan"]);
        fn scans(raw: &serde_json::Value, out: &mut Vec<(String,String)>) {
            if raw["Node Type"] == "Seq Scan" { if let (Some(schema),Some(table))=(raw["Schema"].as_str(),raw["Relation Name"].as_str()) { out.push((schema.into(),table.into())); } }
            if let Some(children)=raw["Plans"].as_array() { for c in children { scans(c,out); } }
        }
        let mut tables=vec![]; scans(&raw[0]["Plan"],&mut tables);
        for (schema,table) in tables {
            let _guard=tokio::select! { biased; _=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Explain cancelled")), guard=self.state.lock()=>guard };
            let params: [&(dyn tokio_postgres::types::ToSql + Sync); 2] = [&schema, &table];
            let rows=tokio::select! { biased;
                _=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Explain cancelled")),
                rows=self.client.query("SELECT c.reltuples::float8 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE (n.nspname=$1 OR ($1='pg_temp' AND n.oid=pg_my_temp_schema())) AND c.relname=$2", &params) => rows.map_err(|e| self.error(e))?,
            };
            if rows.first().is_some_and(|r| r.get::<_,f64>(0)>=10000.0) {
                plan.warnings.push(format!("Large table full scan: {schema}.{table} (~{} rows in catalog statistics)",rows[0].get::<_,f64>(0)));
            }
        }
        if analyze { plan.warnings.extend(preview.notes); }
        Ok(plan)
    }

    async fn preview_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<ChangePreview,DbError> {
        let mut exec = ExecRequest {text:String::new(),read_only:false,params:Default::default(),row_limit:1000,graph_node_limit:1,timeout_ms:req.timeout_ms,preview_sample_limit:req.preview_sample_limit};
        exec.timeout_ms=req.timeout_ms;
        match self.execute_cancelled(&exec,cancel,true,Some((req, false))).await? {RunOutput::Preview(r)=>Ok(r),_=>unreachable!()}
    }
    async fn apply_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<EditResult,DbError> {
        let mut exec = ExecRequest {text:String::new(),read_only:false,params:Default::default(),row_limit:1000,graph_node_limit:1,timeout_ms:req.timeout_ms,preview_sample_limit:req.preview_sample_limit};
        exec.timeout_ms=req.timeout_ms;
        match self.execute_cancelled(&exec,cancel,false,Some((req, false))).await? {RunOutput::Edit(r)=>Ok(r),_=>unreachable!()}
    }

    async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> { self.load_schema_graph(scope).await }
    async fn preview(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        match self.execute_cancelled(req, cancel, true, None).await? {
            RunOutput::Preview(r) => Ok(r),
            _ => unreachable!(),
        }
    }
    async fn ping(&self) -> Result<(), DbError> {
        let _guard = self.state.lock().await;
        self.client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| self.error(e))?;
        Ok(())
    }
    async fn server_info(&self) -> Result<String, DbError> {
        let _guard = self.state.lock().await;
        Ok(self
            .client
            .query_one("SELECT version()", &[])
            .await
            .map_err(|e| self.error(e))?
            .get(0))
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        match self.execute_cancelled(req, cancel, false, None).await? {
            RunOutput::Execute(r) => Ok(r),
            _ => unreachable!(),
        }
    }
    async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> {
        Err(DbError::unsupported("Graph expansion is not supported"))
    }
    async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.connection.abort();
    }
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        let path = parse_path(node_id)?;
        if path.len() != 4 || path[0] != "table" {
            return Err(DbError::query("Not a table node"));
        }
        let (schema, table) = (&path[1], &path[3]);
        let keys = {
            let _guard = self.state.lock().await;
            self.client.query("SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid=i.indrelid AND a.attnum IN (SELECT k.attnum FROM unnest(i.indkey) WITH ORDINALITY AS k(attnum,ord) WHERE k.ord<=i.indnkeyatts) WHERE i.indrelid=to_regclass($1) AND i.indisprimary ORDER BY array_position(i.indkey,a.attnum)",&[&format!("{}.{}",quote_ident(schema),quote_ident(table))]).await.map_err(|e|self.error(e))?
        };
        let order = if keys.is_empty() {
            if path[2] == "views" {
                "1".into()
            } else {
                "ctid".into()
            }
        } else {
            keys.iter()
                .map(|r| quote_ident(r.get(0)))
                .collect::<Vec<_>>()
                .join(",")
        };
        let limit = clamp_row_limit(page.limit);
        let predicates = page.filter.iter().map(|(field,value)| {
            let literal = dbcore::filter::sql_literal(value)?;
            let literal = if literal.starts_with('\'') { format!("E{}",literal.replace('\\',"\\\\")) } else {literal};
            Ok(format!("{} {} {}",quote_ident(field),if matches!(value,Value::Null) {"IS"}else{"="},literal))
        }).collect::<Result<Vec<_>,DbError>>()?;
        let condition = if predicates.is_empty() {String::new()}else{format!(" WHERE {}",predicates.join(" AND "))};
        let req = ExecRequest {
            read_only: true,
            text: format!(
                "SELECT * FROM {}.{}{condition} ORDER BY {order} LIMIT {} OFFSET {}",
                quote_ident(schema),
                quote_ident(table),
                limit + 1,
                page.offset
            ),
            params: Default::default(),
            row_limit: limit,
            graph_node_limit: 300,
            timeout_ms: 30_000,
            preview_sample_limit: default_preview_sample_limit(),
        };
        let mut result = self.execute(&req, CancelToken::new()).await?.remove(0);
        if !keys.is_empty() && path[2] == "tables" { result.edit_target = Some(EditTarget {node_id:node_id.into(),key_columns:keys.iter().map(|r|r.get(0)).collect()}); }
        result.page = Some(PageInfo {
            offset: page.offset,
            limit,
            has_more: result.truncated,
        });
        Ok(result)
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        let _guard = self.state.lock().await;
        let mut nodes = Vec::new();
        let Some(parent) = parent else {
            for row in self.client.query("SELECT nspname FROM pg_namespace WHERE nspname NOT IN ('pg_catalog','information_schema') AND nspname NOT LIKE 'pg_toast%' AND nspname NOT LIKE 'pg_temp%' ORDER BY nspname",&[]).await.map_err(|e|self.error(e))? {
                let name:String=row.get(0); let mut n=node(path_id(&["schema",&name]),&name,"schema",Icon::Database,true,None); n.actions.push(NodeAction {id:"diagram".into(),label:"다이어그램 보기".into(),kind:NodeActionKind::OpenSchemaGraph {scope:Some(name)}}); nodes.push(n);
            }
            return Ok(nodes);
        };
        let path = parse_path(parent)?;
        match path.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["schema",schema]=>{
                for (kind,label) in [("tables","테이블"),("views","뷰")] { nodes.push(node(path_id(&["folder",schema,kind]),label,"folder",Icon::Folder,true,None)); }
            }
            ["folder",schema,kind] if matches!(*kind,"tables"|"views")=>{
                let relkinds=if *kind=="tables" { vec!["r","p","f"] } else { vec!["v","m"] };
                for row in self.client.query("SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind::text=ANY($2) ORDER BY c.relname",&[schema,&relkinds]).await.map_err(|e|self.error(e))? {
                    let name:String=row.get(0);
                    let mut table=node(path_id(&["table",schema,kind,&name]),&name,if *kind=="views"{"view"}else{"table"},if *kind=="views"{Icon::View}else{Icon::Table},true,None);
                    table.actions=vec![NodeAction { id:"browse".into(),label:"데이터 보기".into(),kind:NodeActionKind::Browse },NodeAction { id:"query".into(),label:"쿼리 열기".into(),kind:NodeActionKind::OpenQuery { text:format!("SELECT * FROM {}.{} LIMIT 100",quote_ident(schema),quote_ident(&name)) } }];
                    nodes.push(table);
                }
            }
            ["table",schema,kind,table]=>{
                for (folder,label,icon) in [("columns","컬럼",Icon::Column),("indexes","인덱스",Icon::Index)] { nodes.push(node(path_id(&[folder,schema,kind,table]),label,"folder",icon,true,None)); }
            }
            ["columns",schema,_,table]=>{
                for row in self.client.query("SELECT a.attname, format_type(a.atttypid,a.atttypmod), NOT a.attnotnull, EXISTS(SELECT 1 FROM pg_index i WHERE i.indrelid=c.oid AND i.indisprimary AND a.attnum=ANY(i.indkey)) FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2 AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum",&[schema,table]).await.map_err(|e|self.error(e))? {
                    let name:String=row.get(0);let typ:String=row.get(1);let nullable:bool=row.get(2);let pk:bool=row.get(3);
                    nodes.push(node(path_id(&["column",schema,table,&name]),&name,"column",if pk {Icon::Key}else{Icon::Column},false,Some(format!("{typ} · {}{}",if nullable {"NULL"}else{"NOT NULL"},if pk {" · PK"}else{""}))));
                }
            }
            ["indexes",schema,_,table]=>{
                for row in self.client.query("SELECT indexname,indexdef FROM pg_indexes WHERE schemaname=$1 AND tablename=$2 ORDER BY indexname",&[schema,table]).await.map_err(|e|self.error(e))? {
                    let name:String=row.get(0);nodes.push(node(path_id(&["index",schema,table,&name]),&name,"index",Icon::Index,false,Some(row.get(1))));
                }
            }
            _=>return Err(DbError::query("Invalid explorer node")),
        }
        Ok(nodes)
    }
}
fn node(
    id: String,
    label: &str,
    kind: &str,
    icon: Icon,
    has_children: bool,
    detail: Option<String>,
) -> ExplorerNode {
    ExplorerNode {
        id,
        label: label.into(),
        detail,
        kind: kind.into(),
        icon,
        has_children,
        actions: Vec::new(),
    }
}
fn path_id(parts: &[&str]) -> String {
    serde_json::to_string(parts).expect("string array serialization")
}
fn parse_path(id: &str) -> Result<Vec<String>, DbError> {
    serde_json::from_str(id).map_err(|_| DbError::query("Invalid explorer node id"))
}
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn pg_error(e: tokio_postgres::Error, secret: Option<&str>) -> DbError {
    let redact = |text: &str| match secret.filter(|s| !s.is_empty()) {
        Some(s) => text.replace(s, "***"),
        None => text.into(),
    };
    if let Some(pg) = e.as_db_error() {
        let kind = match pg.code().code() {
            "28P01" | "28000" => ErrorKind::Auth,
            "57014" => ErrorKind::Cancelled,
            code if code.starts_with("08") || matches!(code, "57P01" | "57P02" | "57P03") => {
                ErrorKind::Connection
            }
            _ => ErrorKind::Query,
        };
        DbError {
            kind,
            message: redact(pg.message()),
            detail: pg.detail().map(redact),
            position: pg.position().map(|p| match p {
                ErrorPosition::Original(p) | ErrorPosition::Internal { position: p, .. } => *p,
            }),
        }
    } else {
        DbError::new(
            ErrorKind::Connection,
            if e.to_string().to_ascii_lowercase().contains("tls") {
                "PostgreSQL TLS handshake failed"
            } else {
                "PostgreSQL connection failed or was closed"
            },
        )
    }
}
fn map_value(typ: &Type, text: Option<&str>) -> Result<Value, DbError> {
    let Some(text) = text else {
        return Ok(Value::Null);
    };
    let owned = || text.to_string();
    Ok(match typ.oid() {
        16 => Value::Bool(text == "t" || text == "true"),
        20 | 21 | 23 => Value::Int(owned()),
        1700 => Value::Decimal(owned()),
        700 | 701 => Value::Float(match text.parse::<f64>() {
            Ok(v) if v.is_finite() => FloatValue::Number(v),
            _ => FloatValue::Special(
                match text {
                    "-Infinity" => "-Infinity",
                    "Infinity" => "Infinity",
                    _ => "NaN",
                }
                .into(),
            ),
        }),
        18 | 19 | 25 | 1042 | 1043 => Value::Text(owned()),
        1082 => Value::Date(owned()),
        1083 | 1266 => Value::Time(owned()),
        1114 => Value::DateTime(owned()),
        1184 => Value::DateTimeTz(owned()),
        1186 => Value::Duration(owned()),
        2950 => Value::Uuid(owned()),
        114 | 3802 => Value::Json(owned()),
        17 => {
            let hex = text
                .strip_prefix("\\x")
                .ok_or_else(|| DbError::query("Unexpected bytea text format"))?;
            if hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(DbError::query("Invalid bytea hex"));
            }
            Value::Bytes {
                len: (hex.len() / 2) as u64,
                hex: hex.chars().take(128).collect(),
            }
        }
        _ => Value::Other {
            type_name: match typ.kind() {
                tokio_postgres::types::Kind::Array(inner) => format!("{}[]", inner.name()),
                _ => typ.name().into(),
            },
            text: owned(),
        },
    })
}

/// Result title: the first 80 chars, with an ellipsis when the text was cut.
fn result_title(text: &str) -> String {
    let mut chars = text.chars();
    let mut title: String = chars.by_ref().take(80).collect();
    if chars.next().is_some() {
        title.push('…');
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn result_title_caps_at_80_chars_with_ellipsis() {
        assert_eq!(result_title("select 1"), "select 1");
        let exact = "a".repeat(80);
        assert_eq!(result_title(&exact), exact);
        let long = "한".repeat(81);
        assert_eq!(result_title(&long), format!("{}…", "한".repeat(80)));
    }
    #[test]
    fn quotes_identifiers_and_types() {
        assert_eq!(quote_ident("x\"; DROP TABLE t"), "\"x\"\"; DROP TABLE t\"");
        assert_eq!(
            map_value(&Type::INT8, Some("9223372036854775807")).unwrap(),
            Value::Int("9223372036854775807".into())
        );
        assert_eq!(
            map_value(&Type::BYTEA, Some("\\x00ff")).unwrap(),
            Value::Bytes {
                len: 2,
                hex: "00ff".into()
            }
        );
        assert_eq!(
            map_value(&Type::FLOAT8, Some("Infinity")).unwrap(),
            Value::Float(FloatValue::Special("Infinity".into()))
        );
        assert_eq!(
            map_value(&Type::INT4_ARRAY, Some("{1,2}")).unwrap(),
            Value::Other {
                type_name: "int4[]".into(),
                text: "{1,2}".into()
            }
        );
    }
}

mod management;
mod administration;
