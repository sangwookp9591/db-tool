//! Minimal process-local key/value driver. Intentionally not registered in the app.
use async_trait::async_trait;
use dbcore::*;
use std::{collections::BTreeMap, sync::{Arc, Mutex}};

pub struct KvDriver {
    manifest: DriverManifest,
    data: Arc<Mutex<BTreeMap<String, String>>>,
}
impl Default for KvDriver {
    fn default() -> Self { Self::new() }
}
impl KvDriver {
    pub fn new() -> Self {
        Self {
            data: Arc::default(),
            manifest: DriverManifest {
                id: "example-kv".into(), display_name: "Example KV (memory)".into(),
                paradigm: Paradigm::KeyValue,
                query_language: QueryLanguage { id: "kv".into(), display: "KV commands".into(), editor_mode: "text".into(), supports_params: false },
                connection_fields: vec![],
                capabilities: Capabilities {
                    backup: false, roles: false,
                    import_rows: false, schema_snapshot: false, ddl_render: false,
                    explain: false,
                    lint: false,
                    schema_graph: false,
                    filtered_browse: false,
                    preview_changes: PreviewSupport::None,
                    cancel: CancelSupport::None, timeout: false, params: false,
                    transactions: false, paged_browse: false, graph_expand: false,
                    multi_statement: false, edit_rows: false, read_only_exec: false,
                },
                default_query: "KEYS *".into(),
            },
        }
    }
}
struct KvSession { data: Arc<Mutex<BTreeMap<String, String>>> }
#[async_trait]
impl Driver for KvDriver {
    fn manifest(&self) -> &DriverManifest { &self.manifest }
    async fn connect(&self, _: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        Ok(Box::new(KvSession { data: self.data.clone() }))
    }
}
#[async_trait]
impl Session for KvSession {
    async fn ping(&self) -> Result<(), DbError> { Ok(()) }
    async fn server_info(&self) -> Result<String, DbError> { Ok("Example in-memory KV".into()) }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        if parent.is_some() { return Ok(vec![]); }
        let data = self.data.lock().map_err(|_| DbError::internal("KV lock poisoned"))?;
        Ok(data.keys().map(|key| ExplorerNode {
            id: key.clone(), label: key.clone(), detail: None, kind: "key".into(), icon: Icon::Key,
            has_children: false, actions: vec![NodeAction { id: "get".into(), label: "값 가져오기".into(), kind: NodeActionKind::OpenQuery { text: format!("GET {key}") } }],
        }).collect())
    }
    async fn browse(&self, _: &str, _: PageRequest) -> Result<ResultSet, DbError> { Err(DbError::unsupported("No paged browse")) }
    async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> { Err(DbError::unsupported("No graph")) }
    async fn close(&self) {}
    async fn execute(&self, req: &ExecRequest, _: CancelToken) -> Result<Vec<ResultSet>, DbError> {
        if req.read_only || !req.params.is_empty() { return Err(DbError::unsupported("Read-only execution and parameters are not supported")); }
        let mut words = req.text.trim().splitn(3, char::is_whitespace);
        let command = words.next().unwrap_or("");
        let key = words.next().unwrap_or("");
        let value = words.next();
        let mut data = self.data.lock().map_err(|_| DbError::internal("KV lock poisoned"))?;
        let mut rows = Vec::new();
        let mut affected = None;
        let limit = clamp_row_limit(req.row_limit) as usize;
        let mut truncated = false;
        match (command, key, value) {
            ("GET", key, None) if !key.is_empty() => {
                if let Some(value) = data.get(key) { rows.push(vec![Value::Text(key.into()), Value::Text(value.clone())]); }
            }
            ("SET", key, Some(value)) if !key.is_empty() && !value.is_empty() => { data.insert(key.into(), value.into()); affected = Some(1); }
            ("KEYS", pattern, None) if !pattern.is_empty() => {
                if pattern.contains('*') && (pattern.matches('*').count() != 1 || !pattern.ends_with('*')) {
                    return Err(DbError::query("KEYS supports exact keys or one trailing *"));
                }
                for (key, value) in data.iter().filter(|(key, _)| pattern.strip_suffix('*').map_or(key.as_str() == pattern, |prefix| key.starts_with(prefix))) {
                    if rows.len() == limit { truncated = true; break; }
                    rows.push(vec![Value::Text(key.clone()), Value::Text(value.clone())]);
                }
            }
            _ => return Err(DbError::query("Expected GET k, SET k v, or KEYS prefix*")),
        }
        Ok(vec![ResultSet {
            title: command.into(), row_count: rows.len() as u64, truncated, affected,
            table: Some(Table { columns: vec![Column { name: "key".into(), type_name: "TEXT".into() }, Column { name: "value".into(), type_name: "TEXT".into() }], rows }),
            ..Default::default()
        }])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn commands_and_capabilities() {
        let driver = KvDriver::new();
        assert!(!driver.manifest().capabilities.paged_browse);
        let session = driver.connect(&ConnectConfig { fields: Default::default(), secret: None, connect_timeout_ms: 100 }).await.unwrap();
        for command in ["SET a first value", "SET b second"] {
            let req = serde_json::from_value(serde_json::json!({"text":command})).unwrap();
            assert_eq!(session.execute(&req, CancelToken::new()).await.unwrap()[0].affected, Some(1));
        }
        let mut req: ExecRequest = serde_json::from_value(serde_json::json!({"text":"GET a"})).unwrap();
        assert_eq!(session.execute(&req, CancelToken::new()).await.unwrap()[0].table.as_ref().unwrap().rows[0][1], Value::Text("first value".into()));
        req.text = "KEYS *".into(); req.row_limit = 1;
        assert!(session.execute(&req, CancelToken::new()).await.unwrap()[0].truncated);
        assert_eq!(session.list_children(None).await.unwrap().len(), 2);
        req.read_only = true;
        assert_eq!(session.execute(&req, CancelToken::new()).await.unwrap_err().kind, ErrorKind::Unsupported);
        req.read_only = false; req.text = "DELETE a".into();
        assert_eq!(session.execute(&req, CancelToken::new()).await.unwrap_err().kind, ErrorKind::Query);
    }
}
