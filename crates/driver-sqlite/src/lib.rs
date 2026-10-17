//! SQLite driver. Every connection operation runs on the blocking pool.
mod preview;
mod edits;
mod schema;
use async_trait::async_trait;
use dbcore::*;
use fallible_iterator::FallibleIterator;
use preview::RunOutput;
use rusqlite::{
    hooks::{AuthAction, AuthContext, Authorization},
    types::ValueRef,
    Batch, Connection, OpenFlags,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub struct SqliteDriver {
    manifest: DriverManifest,
}
impl Default for SqliteDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl SqliteDriver {
    pub fn new() -> Self {
        Self {
            manifest: DriverManifest {
                id: "sqlite".into(),
                display_name: "SQLite".into(),
                paradigm: Paradigm::Relational,
                query_language: QueryLanguage {
                    id: "sql".into(),
                    display: "SQL".into(),
                    editor_mode: "sql".into(),
                    supports_params: false,
                },
                connection_fields: vec![
                    FieldSpec {
                        key: "path".into(),
                        label: "Database file".into(),
                        kind: FieldKind::FilePath,
                        required: true,
                        default: None,
                        secret: false,
                        placeholder: "/path/to/database.sqlite".into(),
                    },
                    FieldSpec {
                        key: "read_only_open".into(),
                        label: "Open read-only".into(),
                        kind: FieldKind::Bool,
                        required: false,
                        default: Some("false".into()),
                        secret: false,
                        placeholder: String::new(),
                    },
                ],
                capabilities: Capabilities {
                    schema_graph: true,
                    filtered_browse: true,
                    preview_changes: PreviewSupport::Transactional,
                    cancel: CancelSupport::Server,
                    timeout: true,
                    multi_statement: true,
                    paged_browse: true,
                    read_only_exec: true,
                    transactions: true,
                    params: false,
                    graph_expand: false,
                    edit_rows: true,
                },
                default_query: "SELECT sqlite_version();".into(),
            },
        }
    }
}
#[async_trait]
impl Driver for SqliteDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        let path = cfg
            .fields
            .get("path")
            .filter(|p| !p.trim().is_empty())
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "Database path is required"))?
            .clone();
        let flags = match cfg
            .fields
            .get("read_only_open")
            .map(String::as_str)
            .unwrap_or("false")
        {
            "true" => OpenFlags::SQLITE_OPEN_READ_ONLY,
            "false" => OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
            _ => {
                return Err(DbError::new(
                    ErrorKind::Connection,
                    "Invalid read_only_open",
                ))
            }
        };
        let connection = tokio::task::spawn_blocking(move || {
            let conn = Connection::open_with_flags(path, flags)
                .map_err(|e| DbError::new(ErrorKind::Connection, e.to_string()))?;
            conn.busy_timeout(Duration::from_millis(50))
                .map_err(sql_error)?;
            Ok::<_, DbError>(conn)
        })
        .await
        .map_err(|e| DbError::internal(e.to_string()))??;
        Ok(Box::new(SqliteSession {
            connection: Arc::new(Mutex::new(Some(connection))),
        }))
    }
}
pub struct SqliteSession {
    connection: Arc<Mutex<Option<Connection>>>,
}
impl Drop for SqliteSession {
    fn drop(&mut self) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let connection = self.connection.clone();
            // Implicit rollback/close can perform disk I/O too.
            runtime.spawn_blocking(move || {
                connection.blocking_lock().take();
            });
        }
    }
}
fn sql_error(e: rusqlite::Error) -> DbError {
    DbError::query(e.to_string())
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn node(id: String, label: impl Into<String>, icon: Icon, has_children: bool) -> ExplorerNode {
    ExplorerNode {
        id,
        label: label.into(),
        detail: None,
        kind: format!("{icon:?}").to_lowercase(),
        icon,
        has_children,
        actions: vec![],
    }
}
fn request(text: String) -> ExecRequest {
    ExecRequest {
        text,
        read_only: true,
        params: Default::default(),
        row_limit: 100_000,
        graph_node_limit: 1,
        timeout_ms: 30_000,
        preview_sample_limit: default_preview_sample_limit(),
    }
}
impl SqliteSession {
    async fn bounded(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
        preview: bool,
        edits: Option<&EditRequest>,
    ) -> Result<RunOutput, DbError> {
        if !req.params.is_empty() {
            return Err(DbError::unsupported("SQLite parameters are not enabled"));
        }
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(req.timeout_ms.max(1)))
            .ok_or_else(|| DbError::query("Timeout exceeds supported range"))?;
        let mut guard = tokio::select! {
            _ = tokio::time::sleep_until(deadline.into()) => return Err(DbError::new(ErrorKind::Timeout, "Execution timed out waiting for session")),
            _ = cancel.cancelled() => return Err(DbError::new(ErrorKind::Cancelled, "Execution cancelled")),
            guard = self.connection.clone().lock_owned() => guard,
        };
        let conn = guard
            .as_ref()
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "Session closed"))?;
        let interrupt = conn.get_interrupt_handle();
        let done = CancelToken::new();
        let finished = done.clone();
        let interrupt_gate = Arc::new(std::sync::Mutex::new(()));
        let watcher_gate = interrupt_gate.clone();
        let token = cancel.clone();
        let req = req.clone();
        let edits = edits.cloned();
        // Keep the watcher and connection lease alive even if the host drops this future.
        let watcher = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = finished.cancelled() => {},
                _ = token.cancelled() => {
                    let _lease = watcher_gate.lock().expect("interrupt gate");
                    if !finished.is_cancelled() { interrupt.interrupt(); }
                },
                _ = tokio::time::sleep_until(deadline.into()) => {
                    let _lease = watcher_gate.lock().expect("interrupt gate");
                    if !finished.is_cancelled() { interrupt.interrupt(); }
                },
            }
        });
        tokio::task::spawn_blocking(move || {
            let conn = guard.as_ref().expect("connection lease");
            let result = if let Some(edit) = edits {
                edits::run(conn, &edit, &cancel, deadline, preview)
            } else if preview {
                preview::run(conn, &req, &cancel, deadline).map(RunOutput::Preview)
            } else {
                run(conn, &req, &cancel, deadline).map(RunOutput::Execute)
            };
            if result
                .as_ref()
                .is_err_and(|e| e.kind == ErrorKind::Connection)
            {
                guard.take();
            }
            {
                // Exclude a late interrupt before releasing the connection to its next run.
                let _lease = interrupt_gate.lock().expect("interrupt gate");
                done.cancel();
            }
            watcher.abort();
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "Execution cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(DbError::new(ErrorKind::Timeout, "Execution timed out"));
            }
            result
        })
        .await
        .map_err(|e| DbError::internal(e.to_string()))?
    }
    async fn with_connection<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
    ) -> Result<T, DbError> {
        let guard = self.connection.clone().lock_owned().await;
        tokio::task::spawn_blocking(move || {
            f(guard
                .as_ref()
                .ok_or_else(|| DbError::new(ErrorKind::Connection, "Session closed"))?)
        })
        .await
        .map_err(|e| DbError::internal(e.to_string()))?
    }
}
#[async_trait]
impl Session for SqliteSession {
    async fn preview_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<ChangePreview,DbError> {
        let mut exec = request(String::new());
        exec.timeout_ms=req.timeout_ms;
        match self.bounded(&exec,cancel,true,Some(req)).await? {RunOutput::Preview(r)=>Ok(r),_=>unreachable!()}
    }
    async fn apply_edits(&self, req: &EditRequest, cancel: CancelToken) -> Result<EditResult,DbError> {
        let mut exec = request(String::new());
        exec.timeout_ms=req.timeout_ms;
        match self.bounded(&exec,cancel,false,Some(req)).await? {RunOutput::Edit(r)=>Ok(r),_=>unreachable!()}
    }

    async fn schema_graph(&self, _scope: Option<&str>) -> Result<SchemaGraph, DbError> { self.load_schema_graph().await }
    async fn ping(&self) -> Result<(), DbError> {
        self.with_connection(|c| c.query_row("SELECT 1", [], |_| Ok(())).map_err(sql_error))
            .await
    }
    async fn server_info(&self) -> Result<String, DbError> {
        self.with_connection(|c| {
            c.query_row("SELECT sqlite_version()", [], |r| r.get(0))
                .map_err(sql_error)
        })
        .await
    }
    async fn close(&self) {
        let mut guard = self.connection.clone().lock_owned().await;
        let _ = tokio::task::spawn_blocking(move || {
            guard.take();
        })
        .await;
    }
    async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> {
        Err(DbError::unsupported("SQLite has no graph expansion"))
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        match self.bounded(req, cancel, false, None).await? {
            RunOutput::Execute(r) => Ok(r),
            _ => unreachable!(),
        }
    }
    async fn preview(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        match self.bounded(req, cancel, true, None).await? {
            RunOutput::Preview(r) => Ok(r),
            _ => unreachable!(),
        }
    }
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        let name = node_id
            .strip_prefix("table:")
            .ok_or_else(|| DbError::query("Invalid table node"))?;
        let limit = clamp_row_limit(page.limit);
        let offset = i64::try_from(page.offset)
            .map_err(|_| DbError::query("Page offset exceeds SQLite range"))?;
        let predicates = page.filter.iter().map(|(field,value)| Ok(format!("{} {} {}",quote(field),if matches!(value,Value::Null) {"IS"}else{"="},dbcore::filter::sql_literal(value)?))).collect::<Result<Vec<_>,DbError>>()?;
        let condition=if predicates.is_empty(){String::new()}else{format!(" WHERE {}",predicates.join(" AND "))};
        let mut req = request(format!(
            "SELECT * FROM main.{}{condition} LIMIT {} OFFSET {offset}",
            quote(name),
            u64::from(limit) + 1
        ));
        req.row_limit = limit;
        let mut result = self.execute(&req, CancelToken::new()).await?.remove(0);
        let owned=node_id.to_string();
        result.edit_target=self.with_connection(move |c| edits::target(c,&owned).map(|(_,keys)| if keys.is_empty(){None}else{Some(EditTarget{node_id:owned,key_columns:keys})})).await?;
        result.page = Some(PageInfo {
            offset: page.offset,
            limit,
            has_more: result.truncated,
        });
        Ok(result)
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        let parent = parent.map(str::to_owned);
        self.with_connection(move |c| {
            let Some(parent) = parent else { let mut tables=node("tables".into(), "Tables", Icon::Folder, true); tables.actions.push(NodeAction {id:"diagram".into(),label:"다이어그램 보기".into(),kind:NodeActionKind::OpenSchemaGraph {scope:None}}); return Ok(vec![tables,node("views".into(), "Views", Icon::Folder, true)]); };
            if parent == "tables" || parent == "views" {
                let mut stmt = c.prepare("SELECT name FROM sqlite_master WHERE type=?1 AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(sql_error)?;
                let names = stmt.query_map([if parent == "tables" { "table" } else { "view" }], |r| r.get::<_, String>(0)).map_err(sql_error)?;
                return names.map(|name| {
                    let name = name.map_err(sql_error)?;
                    let text = format!("SELECT * FROM {} LIMIT 100;", quote(&name));
                    let mut n = node(format!("table:{name}"), name, if parent == "tables" { Icon::Table } else { Icon::View }, true);
                    n.actions = vec![NodeAction { id: "browse".into(), label: "Browse".into(), kind: NodeActionKind::Browse }, NodeAction { id: "query".into(), label: "Open query".into(), kind: NodeActionKind::OpenQuery { text } }];
                    Ok(n)
                }).collect();
            }
            if let Some(name) = parent.strip_prefix("table:") {
                return Ok(vec![node(format!("columns:{name}"), "Columns", Icon::Folder, true), node(format!("indexes:{name}"), "Indexes", Icon::Folder, true)]);
            }
            if let Some(name) = parent.strip_prefix("columns:") {
                let mut stmt = c.prepare("SELECT name, type, pk FROM pragma_table_info(?1) ORDER BY cid").map_err(sql_error)?;
                return stmt.query_map([name], |r| {
                    let name: String = r.get(0)?;
                    let pk: i64 = r.get(2)?;
                    let mut n = node(format!("column:{name}"), name, if pk > 0 { Icon::Key } else { Icon::Column }, false);
                    n.detail = Some(format!("{}{}", r.get::<_, String>(1)?, if pk > 0 { " PRIMARY KEY" } else { "" }));
                    Ok(n)
                }).map_err(sql_error)?.map(|r| r.map_err(sql_error)).collect();
            }
            if let Some(name) = parent.strip_prefix("indexes:") {
                let mut stmt = c.prepare("SELECT p.name, m.sql FROM pragma_index_list(?1) p LEFT JOIN sqlite_master m ON m.name=p.name AND m.type='index' ORDER BY p.seq").map_err(sql_error)?;
                return stmt.query_map([name], |r| {
                    let name: String = r.get(0)?;
                    let mut n = node(format!("index:{name}"), name, Icon::Index, false);
                    n.detail = r.get(1)?;
                    Ok(n)
                }).map_err(sql_error)?.map(|r| r.map_err(sql_error)).collect();
            }
            Err(DbError::query("Unknown explorer node"))
        }).await
    }
}
fn run(
    conn: &Connection,
    req: &ExecRequest,
    cancel: &CancelToken,
    deadline: Instant,
) -> Result<Vec<ResultSet>, DbError> {
    let original: bool = conn
        .pragma_query_value(None, "query_only", |r| r.get(0))
        .map_err(sql_error)?;
    if req.read_only {
        conn.pragma_update(None, "query_only", true)
            .map_err(sql_error)?;
        conn.authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
            AuthAction::Pragma { .. }
            | AuthAction::Attach { .. }
            | AuthAction::Detach { .. }
            | AuthAction::Transaction { .. }
            | AuthAction::Savepoint { .. } => Authorization::Deny,
            _ => Authorization::Allow,
        }))
        .map_err(sql_error)?;
    }
    let token = cancel.clone();
    conn.progress_handler(
        1000,
        Some(move || token.is_cancelled() || Instant::now() >= deadline),
    )
    .map_err(sql_error)?;
    let result = (|| {
        let mut results = Vec::new();
        let mut batch = Batch::new(conn, &req.text);
        loop {
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "Execution cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(DbError::new(ErrorKind::Timeout, "Execution timed out"));
            }
            let Some(mut stmt) = batch.next().map_err(sql_error)? else {
                break;
            };
            let start = Instant::now();
            let mut result = ResultSet {
                title: result_title(&req.text),
                ..Default::default()
            };
            if stmt.column_count() == 0 {
                result.affected = Some(stmt.execute([]).map_err(sql_error)? as u64);
                result.message = Some(format!("Affected {} rows", result.affected.unwrap_or(0)));
            } else {
                let mut columns: Vec<Column> = stmt
                    .columns()
                    .iter()
                    .map(|c| Column {
                        name: c.name().into(),
                        type_name: c.decl_type().unwrap_or("").into(),
                    })
                    .collect();
                let mut rows = stmt.query([]).map_err(sql_error)?;
                let mut output = Vec::new();
                while let Some(row) = rows.next().map_err(sql_error)? {
                    if output.len() == clamp_row_limit(req.row_limit) as usize {
                        result.truncated = true;
                        break;
                    }
                    let mut cells = Vec::new();
                    for (i, column) in columns.iter_mut().enumerate() {
                        let value = row.get_ref(i).map_err(sql_error)?;
                        if output.is_empty() && column.type_name.is_empty() {
                            column.type_name = storage_type(value).into();
                        }
                        cells.push(map_value(value, &column.type_name)?);
                    }
                    output.push(cells);
                }
                result.row_count = output.len() as u64;
                result.table = Some(Table {
                    columns,
                    rows: output,
                });
            }
            result.elapsed_ms = start.elapsed().as_millis() as u64;
            results.push(result);
        }
        Ok(results)
    })();
    conn.progress_handler(0, None::<fn() -> bool>)
        .map_err(sql_error)?;
    conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .map_err(sql_error)?;
    let restored = if req.read_only {
        conn.pragma_update(None, "query_only", original)
            .map_err(sql_error)
    } else {
        Ok(())
    };
    restored?;
    result
}
fn storage_type(value: ValueRef<'_>) -> &'static str {
    match value {
        ValueRef::Null => "NULL",
        ValueRef::Integer(_) => "INTEGER",
        ValueRef::Real(_) => "REAL",
        ValueRef::Text(_) => "TEXT",
        ValueRef::Blob(_) => "BLOB",
    }
}
fn map_value(value: ValueRef<'_>, declared: &str) -> Result<Value, DbError> {
    Ok(match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => Value::Int(n.to_string()),
        ValueRef::Real(n) => Value::Float(if n.is_finite() {
            FloatValue::Number(n)
        } else {
            FloatValue::Special(
                if n.is_nan() {
                    "NaN"
                } else if n.is_sign_positive() {
                    "Infinity"
                } else {
                    "-Infinity"
                }
                .into(),
            )
        }),
        ValueRef::Blob(b) => Value::Bytes {
            len: b.len() as u64,
            hex: b.iter().take(64).map(|b| format!("{b:02x}")).collect(),
        },
        ValueRef::Text(b) => {
            let text = std::str::from_utf8(b)
                .map_err(|_| DbError::query("Invalid UTF-8 in SQLite TEXT"))?
                .to_owned();
            match declared.to_ascii_uppercase().as_str() {
                "DATE" => Value::Date(text),
                "DATETIME" | "TIMESTAMP" => Value::DateTime(text),
                "JSON" => Value::Json(text),
                _ => Value::Text(text),
            }
        }
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
}
