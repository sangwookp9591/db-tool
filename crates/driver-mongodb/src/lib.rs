//! MongoDB command documents using the host-independent document/table contracts.
mod schema;
mod edits;
use async_trait::async_trait;
use dbcore::*;
use mongodb::{
    bson::{doc, Bson, Document},
    error::ErrorKind as MongoErrorKind,
    options::{ClientOptions, Credential, Tls, TlsOptions},
    Client, Database,
};
use serde::de::{MapAccess, Visitor};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct MongoDriver {
    manifest: DriverManifest,
}
impl Default for MongoDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl MongoDriver {
    pub fn new() -> Self {
        let field = |key: &str, label: &str, required, default: Option<&str>, secret| FieldSpec {
            key: key.into(),
            label: label.into(),
            kind: if secret {
                FieldKind::Password
            } else {
                FieldKind::Text
            },
            required,
            default: default.map(str::to_owned),
            secret,
            placeholder: default.unwrap_or("").into(),
        };
        Self {
            manifest: DriverManifest {
                id: "mongodb".into(),
                icon_key: Some("mongodb".into()),
                display_name: "MongoDB".into(),
                paradigm: Paradigm::Document,
                query_language: QueryLanguage {
                    id: "mongo".into(),
                    display: "MongoDB command".into(),
                    editor_mode: "json".into(),
                    supports_params: false,
                },
                connection_fields: vec![
                    field("uri", "URI", true, Some("mongodb://localhost:27017"), false),
                    field("database", "데이터베이스", true, None, false),
                    field("user", "사용자", false, None, false),
                    field("password", "비밀번호", false, None, true),
                    field(
                        "authSource",
                        "인증 데이터베이스",
                        false,
                        Some("admin"),
                        false,
                    ),
                    FieldSpec {
                        key: "tls".into(),
                        label: "TLS 모드".into(),
                        kind: FieldKind::Select {
                            options: vec!["uri".into(), "true".into(), "false".into()],
                        },
                        required: false,
                        default: Some("uri".into()),
                        secret: false,
                        placeholder: String::new(),
                    },
                    FieldSpec {
                        key: "tlsCAFile".into(),
                        label: "TLS CA 파일".into(),
                        kind: FieldKind::FilePath,
                        required: false,
                        default: None,
                        secret: false,
                        placeholder: String::new(),
                    },
                ],
                capabilities: Capabilities {
                    execute_stream: false,
                    backup: true, roles: true,
                    import_rows: true, schema_snapshot: true, ddl_render: true,
                    explain: true,
                    lint: false,
                    schema_graph: true,
                    read_only_exec: true,
                    cancel: CancelSupport::Abort,
                    timeout: true,
                    params: false,
                    transactions: false,
                    paged_browse: true,
                    graph_expand: false,
                    multi_statement: false,
                    edit_rows: true,
                    ..Default::default()
                },
                default_query: r#"{"find":"users","filter":{},"limit":50}"#.into(),
            },
        }
    }
}

#[async_trait]
impl Driver for MongoDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        let timeout = Duration::from_millis(cfg.connect_timeout_ms.max(1));
        tokio::time::timeout(timeout, async {
            let mut options = ClientOptions::parse(
                cfg.fields
                    .get("uri")
                    .map(String::as_str)
                    .unwrap_or("mongodb://localhost:27017"),
            )
            .await
            .map_err(|_| {
                DbError::new(
                    ErrorKind::Connection,
                    "Invalid MongoDB URI; use mongodb://host:port and separate credential fields",
                )
            })?;
            // Credentials in a persisted URI bypass the host's secret-field storage/redaction boundary.
            if options.credential.is_some() {
                return Err(DbError::new(
                    ErrorKind::Auth,
                    "Use the separate User and Password fields, not credentials in the URI",
                ));
            }
            let database = cfg
                .fields
                .get("database")
                .filter(|s| !s.is_empty())
                .ok_or_else(|| DbError::query("Database is required"))?
                .clone();
            if let Some(user) = cfg.fields.get("user").filter(|s| !s.is_empty()) {
                options.credential = Some(
                    Credential::builder()
                        .username(user.clone())
                        .password(cfg.secret.clone())
                        .source(
                            cfg.fields
                                .get("authSource")
                                .filter(|s| !s.is_empty())
                                .cloned()
                                .unwrap_or_else(|| "admin".into()),
                        )
                        .build(),
                );
            }
            match cfg.fields.get("tls").map(String::as_str).unwrap_or("uri") {
                "uri" => {}
                "true" => {
                    if !matches!(options.tls, Some(Tls::Enabled(_))) {
                        options.tls = Some(Tls::Enabled(TlsOptions::default()));
                    }
                }
                "false" => options.tls = Some(Tls::Disabled),
                _ => return Err(DbError::new(ErrorKind::Connection, "Invalid TLS option")),
            }
            if let Some(path) = cfg.fields.get("tlsCAFile").filter(|s| !s.is_empty()) {
                let mut tls = match options.tls.take() {
                    Some(Tls::Disabled) => {
                        return Err(DbError::new(
                            ErrorKind::Connection,
                            "TLS CA file requires TLS",
                        ))
                    }
                    Some(Tls::Enabled(tls)) => tls,
                    None => TlsOptions::default(),
                };
                tls.ca_file_path = Some(path.into());
                options.tls = Some(Tls::Enabled(tls));
            }
            options.connect_timeout = Some(timeout);
            options.server_selection_timeout = Some(timeout);
            options.retry_reads = Some(false);
            options.retry_writes = Some(false);
            // The default 10s server monitor was the only CPU wake-up of an idle, hidden app (V1 report).
            // A desktop session re-checks topology on use, so a 60s heartbeat is enough.
            options.heartbeat_freq = Some(Duration::from_secs(60));
            let client = Client::with_options(options).map_err(|_| {
                DbError::new(ErrorKind::Connection, "Invalid MongoDB connection options")
            })?;
            let session = MongoSession {
                client,
                database,
                secret: cfg.secret.clone(),
                closed: Arc::new(AtomicBool::new(false)),
            };
            session.ping().await?;
            Ok(Box::new(session) as Box<dyn Session>)
        })
        .await
        .map_err(|_| DbError::new(ErrorKind::Connection, "MongoDB connection timed out"))?
    }
}

#[derive(Clone)]
struct MongoSession {
    client: Client,
    database: String,
    secret: Option<String>,
    closed: Arc<AtomicBool>,
}
impl MongoSession {
    fn db(&self) -> Database {
        self.client.database(&self.database)
    }
    fn error(&self, error: mongodb::error::Error) -> DbError {
        let (kind, message, detail) = match error.kind.as_ref() {
            MongoErrorKind::Command(e) => (
                match e.code {
                    13 | 18 => ErrorKind::Auth,
                    50 | 262 => ErrorKind::Timeout,
                    11601 => ErrorKind::Cancelled,
                    _ => ErrorKind::Query,
                },
                e.message.clone(),
                Some(format!("code={} codeName={}", e.code, e.code_name)),
            ),
            MongoErrorKind::Authentication { .. } => (
                ErrorKind::Auth,
                "MongoDB authentication failed".into(),
                None,
            ),
            MongoErrorKind::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => (
                ErrorKind::Timeout,
                "MongoDB operation timed out".into(),
                None,
            ),
            MongoErrorKind::Io(_)
            | MongoErrorKind::ServerSelection { .. }
            | MongoErrorKind::DnsResolve { .. }
            | MongoErrorKind::ConnectionPoolCleared { .. } => (
                ErrorKind::Connection,
                "MongoDB connection failed or was closed".into(),
                None,
            ),
            _ => (ErrorKind::Query, "MongoDB command failed".into(), None),
        };
        let redact = |s: String| match self.secret.as_deref().filter(|s| !s.is_empty()) {
            Some(secret) => s.replace(secret, "***"),
            None => s,
        };
        DbError {
            kind,
            message: redact(message),
            detail: detail.map(redact),
            position: None,
        }
    }
    async fn command(&self, db: &Database, command: Document) -> Result<Document, DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session is closed"));
        }
        db.run_command(command).await.map_err(|e| self.error(e))
    }
    async fn run(&self, req: &ExecRequest, cancel: &CancelToken) -> Result<ResultSet, DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session is closed"));
        }
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "MongoDB execution aborted",
            ));
        }
        if !req.params.is_empty() {
            return Err(DbError::unsupported(
                "MongoDB commands do not support parameters; use Extended JSON values",
            ));
        }
        let mut command = parse_command(&req.text)?;
        if req.read_only {
            check_read_only(&command)?;
        }
        let name = command.keys().next().unwrap().clone();
        let projection_is_original = command.get_document("projection").map(|projection| projection.values().all(|value|matches!(value,Bson::Boolean(_)|Bson::Int32(0|1)|Bson::Int64(0|1)))).unwrap_or(!command.contains_key("projection"));
        let find_collection = if name=="find" && projection_is_original {command.get_str("find").ok().map(str::to_owned)} else {None};
        if command.contains_key("$db")
            || command.contains_key("lsid")
            || command.contains_key("txnNumber")
            || command.contains_key("autocommit")
        {
            return Err(DbError::unsupported(
                "Database and session overrides are unsupported",
            ));
        }
        if command.get_bool("tailable").unwrap_or(false)
            || command.get_bool("awaitData").unwrap_or(false)
        {
            return Err(DbError::unsupported(
                "Tailable cursors are unsupported; use a bounded find command",
            ));
        }
        let limit = clamp_row_limit(req.row_limit) as usize;
        let batch = (limit + 1).min(101) as i64;
        // maxTimeMS bounds server work even when the caller drops an Abort execution.
        if matches!(
            name.as_str(),
            "find" | "aggregate" | "count" | "distinct" | "listCollections" | "listIndexes"
        ) {
            let requested = command
                .get("maxTimeMS")
                .and_then(|v| v.as_i64().or_else(|| v.as_i32().map(i64::from)))
                .filter(|n| *n > 0)
                .unwrap_or(i64::MAX);
            command.insert(
                "maxTimeMS",
                requested.min(req.timeout_ms.max(1).min(i32::MAX as u64) as i64),
            );
        }
        if name == "find" {
            command.insert("batchSize", batch);
        }
        if name == "aggregate" {
            command
                .entry("cursor".to_owned())
                .or_insert(Bson::Document(Document::new()));
        }
        if matches!(
            name.as_str(),
            "aggregate" | "listCollections" | "listIndexes"
        ) && let Ok(cursor) = command.get_document_mut("cursor")
        {
            cursor.insert("batchSize", batch);
        }
        let db = self.db();
        let mut session = self
            .client
            .start_session()
            .await
            .map_err(|e| self.error(e))?;
        let mut response = db
            .run_command(command)
            .session(&mut session)
            .await
            .map_err(|e| self.error(e))?;
        if response.contains_key("cursor") {
            let mut guard = CursorCleanup {
                db: db.clone(),
                id: 0,
                collection: String::new(),
            };
            let mut documents = Vec::new();
            let mut first = true;
            loop {
                let cursor = response
                    .get_document("cursor")
                    .map_err(|_| DbError::query("Invalid MongoDB cursor response"))?;
                guard.id = cursor
                    .get_i64("id")
                    .map_err(|_| DbError::query("Missing cursor id"))?;
                guard.collection = cursor
                    .get_str("ns")
                    .ok()
                    .and_then(|ns| ns.strip_prefix(&format!("{}.", self.database)))
                    .ok_or_else(|| DbError::query("Invalid cursor namespace"))?
                    .into();
                let values = cursor
                    .get_array(if first { "firstBatch" } else { "nextBatch" })
                    .map_err(|_| DbError::query("Missing cursor batch"))?;
                for value in values.iter().take(limit + 1 - documents.len()) {
                    documents.push(
                        value
                            .as_document()
                            .ok_or_else(|| DbError::query("Invalid cursor document"))?
                            .clone(),
                    );
                }
                if documents.len() > limit || guard.id == 0 || cancel.is_cancelled() {
                    break;
                }
                response=db.run_command(doc!{"getMore":guard.id,"collection":&guard.collection,"batchSize":(limit+1-documents.len()).min(101) as i64}).session(&mut session).await.map_err(|e| self.error(e))?;
                first = false;
            }
            if guard.id != 0 {
                db.run_command(doc! {"killCursors":&guard.collection,"cursors":[guard.id]})
                    .await
                    .map_err(|e| self.error(e))?;
                guard.id = 0;
            }
            let truncated = documents.len() > limit;
            documents.truncate(limit);
            let candidate=if let Some(collection)=find_collection {self.find_target(&collection,&documents).await} else {Err("집계 또는 projection 결과는 편집할 수 없습니다")};
            let mut result=document_result(documents, truncated);
            dbcore::query_edit::finish(&mut result,candidate,req.read_only);
            return Ok(result);
        }
        if let Ok(errors) = response.get_array("writeErrors")
            && let Some(Bson::Document(error)) = errors.first()
        {
            return Err(DbError {
                kind: ErrorKind::Query,
                message: "MongoDB write failed".into(),
                detail: Some(format!(
                    "code={} codeName={}",
                    error.get_i32("code").unwrap_or(0),
                    error.get_str("codeName").unwrap_or("WriteError")
                )),
                position: None,
            });
        }
        if response.contains_key("writeConcernError") {
            return Err(DbError::query(
                "MongoDB write concern failed; write outcome may be uncertain",
            ));
        }
        if matches!(
            name.as_str(),
            "insert" | "update" | "delete" | "findAndModify" | "bulkWrite"
        ) {
            let affected = response
                .get("n")
                .and_then(|v| v.as_i64().or_else(|| v.as_i32().map(i64::from)))
                .or_else(|| response.get_document("value").ok().map(|_| 1))
                .map(|n| n.max(0) as u64);
            Ok(ResultSet {
                message: Some(format!("MongoDB {name} completed")),
                affected,
                ..Default::default()
            })
        } else if let Ok(values) = response.get_array("values") {
            let truncated = values.len() > limit;
            Ok(document_result(
                values
                    .iter()
                    .take(limit)
                    .map(|v| doc! {"value":v.clone()})
                    .collect(),
                truncated,
            ))
        } else {
            Ok(document_result(vec![std::mem::take(&mut response)], false))
        }
    }
}
// Drain and clean up server cursors even when the caller has already aborted.
struct CursorCleanup {
    db: Database,
    id: i64,
    collection: String,
}
impl Drop for CursorCleanup {
    fn drop(&mut self) {
        if self.id != 0 {
            let db = self.db.clone();
            let id = self.id;
            let collection = self.collection.clone();
            tokio::spawn(async move {
                let _ = db
                    .run_command(doc! {"killCursors":collection,"cursors":[id]})
                    .await;
            });
        }
    }
}

#[async_trait]
impl Session for MongoSession {
    async fn preview_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<ChangePreview,DbError> {Ok(self.edit_document(req,cancel,true).await?.1)}
    async fn apply_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<EditResult,DbError> {Ok(self.edit_document(req,cancel,false).await?.0)}
    async fn restore_options(&self)->Result<RestoreOptions,DbError>{Ok(RestoreOptions{overwrite:false,new_database:true,guidance:"빈 대상에 복원한 뒤 행 수와 인덱스를 확인하고 연결을 교체하세요. 기존 대상 덮어쓰기는 지원하지 않습니다.".into()})}
    async fn restore_database(&self,summary:&BackupSummary,source:&mut (dyn std::io::Read+Send),name:&str,cancel:CancelToken)->Result<(),DbError>{
        validate_restore_database(name)?;
        if self.client.list_database_names().await.map_err(|e|self.error(e))?.iter().any(|db|db==name){return Err(DbError::new(ErrorKind::Conflict,"New restore database already exists"));}
        let target=MongoSession{database:name.into(),..self.clone()};target.restore_stream(summary,source,false,cancel).await
    }
    async fn backup_stream(&self,cancel:CancelToken,sink:&dyn BackupSink)->Result<BackupSummary,DbError>{self.stream_backup(cancel,sink).await}
    async fn restore_stream(&self,summary:&BackupSummary,source:&mut (dyn std::io::Read+Send),overwrite:bool,cancel:CancelToken)->Result<(),DbError>{if summary.format=="mongodb-ndjson-v1"{return self.native_restore(&legacy_backup(summary,source)?,overwrite,cancel).await;}self.stream_restore(summary,source,overwrite,cancel).await}
    async fn backup(&self,cancel:CancelToken)->Result<BackupData,DbError>{self.native_backup(cancel).await}
    async fn restore(&self,backup:&BackupData,overwrite:bool,cancel:CancelToken)->Result<(),DbError>{self.native_restore(backup,overwrite,cancel).await}
    async fn roles(&self)->Result<RolesSnapshot,DbError>{self.native_roles().await}
    async fn plan_grants(&self,changes:&[GrantChange])->Result<GrantPlan,DbError>{self.native_grants(changes)}
    async fn apply_grants(&self,changes:&[GrantChange],cancel:CancelToken)->Result<(),DbError>{self.native_apply_grants(changes,cancel).await}

    async fn import_rows(&self,req:&EditRequest,preview:bool,cancel:CancelToken)->Result<ImportOutput,DbError>{self.import_batch(req,preview,cancel).await}
    async fn schema_snapshot(&self,_scope:Option<&str>)->Result<SchemaSnapshot,DbError>{self.load_snapshot().await}
    async fn ddl_render(&self,diff:&SnapshotDiff)->Result<Vec<String>,DbError>{Self::render_schema(diff)}

    async fn explain(&self, query: &str, analyze: bool, cancel: CancelToken) -> Result<PlanTree, DbError> {
        let command = parse_command(query)?;
        check_read_only(&command)?;
        if !matches!(command.keys().next().map(String::as_str), Some("find" | "aggregate" | "count" | "distinct")) { return Err(DbError::unsupported("Explain supports read commands only")); }
        let command = doc! {"explain": command, "verbosity": if analyze {"executionStats"} else {"queryPlanner"}, "maxTimeMS": 30000};
        let database = self.db();
        let raw = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(DbError::new(ErrorKind::Cancelled, "Explain cancelled")),
            result = self.command(&database, command) => result?,
        };
        let raw = serde_json::to_value(raw).map_err(|_| DbError::query("Invalid Mongo explain plan"))?;
        Ok(PlanTree::mongo_explain(&raw, analyze))
    }

    async fn ping(&self) -> Result<(), DbError> {
        self.command(&self.db(), doc! {"ping":1}).await?;
        Ok(())
    }
    async fn server_info(&self) -> Result<String, DbError> {
        let r = self.command(&self.db(), doc! {"buildInfo":1}).await?;
        Ok(format!(
            "MongoDB {}",
            r.get_str("version").unwrap_or("unknown")
        ))
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "MongoDB execution aborted",
            ));
        }
        let session = self.clone();
        let request = req.clone();
        let stop = cancel.clone();
        // MongoDB futures are not cancellation safe; detach the task and let it drain the reply.
        let mut task = tokio::spawn(async move { session.run(&request, &stop).await });
        let mut result = tokio::select! { biased;
            _=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"MongoDB execution aborted; server work may continue")),
            _=tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)))=>{
                cancel.cancel();
                return Err(DbError::new(ErrorKind::Timeout,"MongoDB execution timed out"));
            },
            result=&mut task=>result.map_err(|_| DbError::internal("MongoDB execution task failed"))??,
        };
        result.title = "MongoDB command".into();
        result.elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(vec![result])
    }
    async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.client.clone().shutdown().await;
    }
    async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> {
        self.load_schema_graph(scope).await
    }
    async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> {
        Err(DbError::unsupported("Graph expansion is not supported"))
    }
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        if !page.filter.is_empty() {
            return Err(DbError::unsupported("Filtered browse is not supported"));
        }
        let path = parse_path(node_id)?;
        let parts = path.iter().map(String::as_str).collect::<Vec<_>>();
        let [kind, database, collection] = parts.as_slice() else {
            return Err(DbError::query("Not a collection node"));
        };
        if *kind != "collection" {
            return Err(DbError::query("Not a collection node"));
        }
        let limit = clamp_row_limit(page.limit);
        let offset = i64::try_from(page.offset)
            .map_err(|_| DbError::query("Page offset exceeds MongoDB int64"))?;
        let other = MongoSession {
            client: self.client.clone(),
            database: (*database).into(),
            secret: self.secret.clone(),
            closed: self.closed.clone(),
        };
        let req=ExecRequest { preview_sample_limit:default_preview_sample_limit(), text:serde_json::json!({"find":collection,"filter":{},"sort":{"_id":1},"skip":offset,"limit":limit+1}).to_string(),params:Default::default(),read_only:true,row_limit:limit,timeout_ms:30_000,graph_node_limit:0 };
        let mut result = other.execute(&req, CancelToken::new()).await?.remove(0);
        result.page = Some(PageInfo {
            offset: page.offset,
            limit,
            has_more: result.truncated,
        });
        Ok(result)
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        let Some(parent) = parent else {
            let r = self
                .command(
                    &self.client.database("admin"),
                    doc! {"listDatabases":1,"nameOnly":true,"authorizedDatabases":true},
                )
                .await?;
            return r
                .get_array("databases")
                .map_err(|_| DbError::query("Missing database list"))?
                .iter()
                .map(|v| {
                    let name = v
                        .as_document()
                        .and_then(|d| d.get_str("name").ok())
                        .ok_or_else(|| DbError::query("Invalid database name"))?;
                    let mut database_node = node(
                        &["database", name],
                        name,
                        "database",
                        Icon::Database,
                        true,
                        None,
                    );
                    database_node.actions.push(NodeAction {
                        id: "diagram".into(),
                        label: "스키마 그래프".into(),
                        kind: NodeActionKind::OpenSchemaGraph {
                            scope: Some(database_node.id.clone()),
                        },
                    });
                    Ok(database_node)
                })
                .collect();
        };
        let path = parse_path(parent)?;
        let parts = path.iter().map(String::as_str).collect::<Vec<_>>();
        match parts.as_slice() {
            ["database", database] => {
                let mut cursor = self
                    .client
                    .database(database)
                    .list_collections()
                    .await
                    .map_err(|e| self.error(e))?;
                let mut nodes = Vec::new();
                while cursor.advance().await.map_err(|e| self.error(e))? {
                    let spec = cursor.deserialize_current().map_err(|e| self.error(e))?;
                    let count = self
                        .client
                        .database(database)
                        .collection::<Document>(&spec.name)
                        .estimated_document_count()
                        .max_time(Duration::from_secs(3))
                        .await
                        .ok();
                    let mut n = node(
                        &["collection", database, &spec.name],
                        &spec.name,
                        "collection",
                        Icon::Collection,
                        true,
                        count.map(|n| format!("~{n} documents")),
                    );
                    n.actions = vec![
                        NodeAction {
                            id: "browse".into(),
                            label: "데이터 보기".into(),
                            kind: NodeActionKind::Browse,
                        },
                        NodeAction {
                            id: "query".into(),
                            label: "쿼리 열기".into(),
                            kind: NodeActionKind::OpenQuery {
                                text: serde_json::json!({"find":spec.name,"filter":{},"limit":50})
                                    .to_string(),
                            },
                        },
                    ];
                    nodes.push(n);
                }
                Ok(nodes)
            }
            ["collection", database, collection] => Ok(vec![
                node(
                    &["fields", database, collection],
                    "필드",
                    "folder",
                    Icon::Folder,
                    true,
                    None,
                ),
                node(
                    &["indexes", database, collection],
                    "인덱스",
                    "folder",
                    Icon::Folder,
                    true,
                    None,
                ),
            ]),
            ["fields", database, collection] => {
                let mut cursor = self
                    .client
                    .database(database)
                    .collection::<Document>(collection)
                    .find(doc! {})
                    .limit(100)
                    .max_time(Duration::from_secs(10))
                    .await
                    .map_err(|e| self.error(e))?;
                let mut fields: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                while cursor.advance().await.map_err(|e| self.error(e))? {
                    for (name, value) in cursor.deserialize_current().map_err(|e| self.error(e))? {
                        fields
                            .entry(name)
                            .or_default()
                            .insert(type_name(&value).into());
                    }
                }
                Ok(fields
                    .into_iter()
                    .map(|(name, types)| {
                        node(
                            &["field", database, collection, &name],
                            &name,
                            "field",
                            Icon::Property,
                            false,
                            Some(types.into_iter().collect::<Vec<_>>().join(" | ")),
                        )
                    })
                    .collect())
            }
            ["indexes", database, collection] => {
                let mut cursor = self
                    .client
                    .database(database)
                    .collection::<Document>(collection)
                    .list_indexes()
                    .await
                    .map_err(|e| self.error(e))?;
                let mut nodes = Vec::new();
                while cursor.advance().await.map_err(|e| self.error(e))? {
                    let index = cursor.deserialize_current().map_err(|e| self.error(e))?;
                    let name = index
                        .options
                        .and_then(|o| o.name)
                        .unwrap_or_else(|| index.keys.to_string());
                    nodes.push(node(
                        &["index", database, collection, &name],
                        &name,
                        "index",
                        Icon::Index,
                        false,
                        Some(index.keys.to_string()),
                    ));
                }
                Ok(nodes)
            }
            _ => Err(DbError::query("Invalid explorer node")),
        }
    }
}
fn node(
    parts: &[&str],
    label: &str,
    kind: &str,
    icon: Icon,
    has_children: bool,
    detail: Option<String>,
) -> ExplorerNode {
    ExplorerNode {
        id: serde_json::to_string(parts).expect("string array"),
        label: label.into(),
        kind: kind.into(),
        icon,
        has_children,
        detail,
        actions: vec![],
    }
}
fn parse_path(id: &str) -> Result<Vec<String>, DbError> {
    serde_json::from_str(id).map_err(|_| DbError::query("Invalid explorer node id"))
}

fn parse_command(text: &str) -> Result<Document, DbError> {
    struct CommandVisitor;
    impl<'de> Visitor<'de> for CommandVisitor {
        type Value = serde_json::Map<String, serde_json::Value>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("one Extended JSON command document")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut out = serde_json::Map::new();
            while let Some((key, value)) = map.next_entry::<String, serde_json::Value>()? {
                if out.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("Duplicate command field"));
                }
            }
            Ok(out)
        }
    }
    let parse = || {
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let map = serde::de::Deserializer::deserialize_map(&mut deserializer, CommandVisitor)
            .map_err(|e| e.to_string())?;
        deserializer.end().map_err(|e| e.to_string())?;
        if map.is_empty() {
            return Err("Empty command".into());
        }
        match Bson::try_from(serde_json::Value::Object(map)).map_err(|e| e.to_string())? {
            Bson::Document(d) => Ok(d),
            _ => Err("Expected document".into()),
        }
    };
    parse().map_err(|e:String|DbError::query(format!("Enter one Extended JSON command, e.g. {{\"find\":\"users\",\"filter\":{{}},\"limit\":50}}; shell syntax db.users.find() is unsupported ({e})")))
}
fn check_read_only(command: &Document) -> Result<(), DbError> {
    let name = command.keys().next().map(String::as_str).unwrap_or("");
    if !matches!(
        name,
        "find"
            | "aggregate"
            | "count"
            | "distinct"
            | "listCollections"
            | "listIndexes"
            | "listDatabases"
            | "dbStats"
            | "collStats"
            | "ping"
            | "buildInfo"
    ) || command.contains_key("$db")
    {
        return Err(DbError::unsupported(
            "MongoDB read-only execution permits only whitelisted read commands",
        ));
    }
    if name == "aggregate" {
        let pipeline = command
            .get_array("pipeline")
            .map_err(|_| DbError::query("aggregate requires a pipeline array"))?;
        check_pipeline(pipeline)?;
    }
    Ok(())
}
fn check_pipeline(pipeline: &[Bson]) -> Result<(), DbError> {
    for stage in pipeline {
        let stage = stage
            .as_document()
            .filter(|d| d.len() == 1)
            .ok_or_else(|| DbError::query("Each aggregation stage must be one document"))?;
        let (name, value) = stage.iter().next().unwrap();
        if !matches!(
            name.as_str(),
            "$match"
                | "$project"
                | "$addFields"
                | "$set"
                | "$unset"
                | "$group"
                | "$sort"
                | "$sortByCount"
                | "$limit"
                | "$skip"
                | "$unwind"
                | "$replaceRoot"
                | "$replaceWith"
                | "$count"
                | "$sample"
                | "$bucket"
                | "$bucketAuto"
                | "$facet"
                | "$lookup"
                | "$graphLookup"
                | "$unionWith"
                | "$setWindowFields"
                | "$densify"
                | "$fill"
                | "$documents"
                | "$redact"
                | "$geoNear"
        ) {
            return Err(DbError::unsupported(format!(
                "Aggregation stage {name} is unavailable in read-only execution"
            )));
        }
        if let Some(doc) = value.as_document() {
            if name == "$facet" {
                for (_, v) in doc {
                    check_pipeline(
                        v.as_array()
                            .ok_or_else(|| DbError::query("Facet requires pipeline arrays"))?,
                    )?;
                }
            }
            if matches!(name.as_str(), "$lookup" | "$unionWith")
                && let Some(v) = doc.get("pipeline")
            {
                check_pipeline(
                    v.as_array()
                        .ok_or_else(|| DbError::query("Nested pipeline must be an array"))?,
                )?;
            }
        }
    }
    Ok(())
}
fn type_name(value: &Bson) -> &'static str {
    match value {
        Bson::Double(_) => "Double",
        Bson::String(_) => "String",
        Bson::Array(_) => "Array",
        Bson::Document(_) => "Document",
        Bson::Boolean(_) => "Bool",
        Bson::Null => "Null",
        Bson::Int32(_) => "Int32",
        Bson::Int64(_) => "Int64",
        Bson::Timestamp(_) => "Timestamp",
        Bson::Binary(_) => "Binary",
        Bson::ObjectId(_) => "ObjectId",
        Bson::DateTime(_) => "DateTime",
        Bson::Decimal128(_) => "Decimal128",
        Bson::RegularExpression(_) => "Regex",
        Bson::JavaScriptCode(_) => "JavaScript",
        Bson::JavaScriptCodeWithScope(_) => "JavaScriptWithScope",
        Bson::MinKey => "MinKey",
        Bson::MaxKey => "MaxKey",
        Bson::Undefined => "Undefined",
        Bson::Symbol(_) => "Symbol",
        Bson::DbPointer(_) => "DbPointer",
    }
}
fn map_value(value: &Bson) -> Value {
    match value {
        Bson::Null => Value::Null,
        Bson::Boolean(v) => Value::Bool(*v),
        Bson::Int32(v) => Value::Int(v.to_string()),
        Bson::Int64(v) => Value::Int(v.to_string()),
        Bson::Double(v) => Value::Float(if v.is_finite() {
            FloatValue::Number(*v)
        } else {
            FloatValue::Special(
                if v.is_nan() {
                    "NaN"
                } else if v.is_sign_negative() {
                    "-Infinity"
                } else {
                    "Infinity"
                }
                .into(),
            )
        }),
        Bson::Decimal128(v) => Value::Decimal(v.to_string()),
        Bson::String(v) => Value::Text(v.clone()),
        Bson::ObjectId(v) => Value::Other {
            type_name: "ObjectId".into(),
            text: v.to_hex(),
        },
        Bson::DateTime(v) => match v.try_to_rfc3339_string() {
            Ok(text) => Value::DateTimeTz(text),
            Err(_) => Value::Other {
                type_name: "DateTime".into(),
                text: v.timestamp_millis().to_string(),
            },
        },
        Bson::Binary(v) => Value::Bytes {
            len: v.bytes.len() as u64,
            hex: v
                .bytes
                .iter()
                .take(64)
                .map(|b| format!("{b:02x}"))
                .collect(),
        },
        Bson::Array(v) => Value::List(v.iter().map(map_value).collect()),
        Bson::Document(v) => Value::Map(v.iter().map(|(k, v)| (k.clone(), map_value(v))).collect()),
        other => Value::Other {
            type_name: type_name(other).into(),
            text: other.to_string(),
        },
    }
}
fn document_result(documents: Vec<Document>, truncated: bool) -> ResultSet {
    let mut columns: Vec<Column> = Vec::new();
    for document in documents.iter().take(100) {
        for (name, value) in document {
            if !columns.iter().any(|c| c.name == *name) {
                columns.push(Column {
                    name: name.clone(),
                    type_name: type_name(value).into(),
                });
            }
        }
    }
    let rows = documents
        .iter()
        .map(|d| {
            columns
                .iter()
                .map(|c| d.get(&c.name).map(map_value).unwrap_or(Value::Null))
                .collect()
        })
        .collect();
    ResultSet {
        row_count: documents.len() as u64,
        documents: Some(
            documents
                .iter()
                .map(|d| map_value(&Bson::Document(d.clone())))
                .collect(),
        ),
        table: Some(Table { columns, rows }),
        truncated,
        ..Default::default()
    }
}
#[cfg(test)]
mod tests;

mod management;
mod administration;
mod backup_stream;
