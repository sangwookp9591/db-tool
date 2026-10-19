//! Shared, host-independent database contracts.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::{Arc, RwLock}};
pub use tokio_util::sync::CancellationToken as CancelToken;
pub mod preview;
pub mod filter;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriverManifest {
    pub id: String,
    pub display_name: String,
    pub paradigm: Paradigm,
    pub query_language: QueryLanguage,
    pub connection_fields: Vec<FieldSpec>,
    pub capabilities: Capabilities,
    pub default_query: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Paradigm {
    Relational,
    Graph,
    Document,
    KeyValue,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryLanguage {
    pub id: String,
    pub display: String,
    pub editor_mode: String,
    pub supports_params: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    pub default: Option<String>,
    pub secret: bool,
    pub placeholder: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldKind {
    Text,
    Number,
    Password,
    FilePath,
    Bool,
    Select { options: Vec<String> },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    #[serde(default)]
    pub schema_graph: bool,
    #[serde(default)]
    pub filtered_browse: bool,
    #[serde(default)]
    pub preview_changes: PreviewSupport,
    #[serde(default)]
    pub read_only_exec: bool,
    pub cancel: CancelSupport,
    pub timeout: bool,
    pub params: bool,
    pub transactions: bool,
    pub paged_browse: bool,
    pub graph_expand: bool,
    pub multi_statement: bool,
    pub edit_rows: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum CancelSupport {
    #[default]
    None,
    Abort,
    Server,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum PreviewSupport {
    #[default]
    None,
    Transactional,
    DryRun,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChangePreview {
    pub statements: Vec<StatementPreview>,
    pub committed: bool,
    pub notes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StatementKind {
    Insert,
    Update,
    Delete,
    Ddl,
    GraphWrite,
    Other,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatementPreview {
    pub text: String,
    pub kind: StatementKind,
    pub affected: Option<u64>,
    pub before: Option<Table>,
    pub after: Option<Table>,
    pub schema_diff: Option<SchemaDiff>,
    pub counters: Vec<(String, i64)>,
    pub warnings: Vec<String>,
}
impl StatementPreview {
    pub fn new(text: String, kind: StatementKind) -> Self {
        Self {
            text,
            kind,
            affected: None,
            before: None,
            after: None,
            schema_diff: None,
            counters: vec![],
            warnings: vec![],
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SchemaDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ConnectConfig {
    pub fields: BTreeMap<String, String>,
    pub secret: Option<String>,
    pub connect_timeout_ms: u64,
}
impl fmt::Debug for ConnectConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Field names are driver-defined, so omit field values as well as the secret.
        f.debug_struct("ConnectConfig")
            .field("fields", &self.fields.keys().collect::<Vec<_>>())
            .field("secret", &"***")
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .finish()
    }
}
#[async_trait]
pub trait Driver: Send + Sync {
    fn manifest(&self) -> &DriverManifest;
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError>;
}
#[async_trait]
pub trait Session: Send + Sync {
    /// Stream large tables to a sink; returned results retain metadata, not streamed rows.
    async fn execute_stream(&self, req: &ExecRequest, cancel: CancelToken, sink: &dyn ResultSink) -> Result<Vec<ResultSet>, DbError> {
        let mut results = self.execute(req, cancel.clone()).await?;
        if streamable_tables(&results) {
            for (index, result) in results.iter_mut().enumerate() {
                let rows = result.table.as_mut().map(|t| std::mem::take(&mut t.rows)).unwrap_or_default();
                sink.start(index, result)?;
                let mut rows = rows.into_iter().peekable();
                let mut first = true;
                while rows.peek().is_some() {
                    if cancel.is_cancelled() { return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다")); }
                    sink.rows(index, rows.by_ref().take(if first { 200 } else { 2000 }).collect())?;
                    first = false;
                }
            }
        }
        Ok(results)
    }
    async fn preview_edits(
        &self,
        _req: &EditRequest,
        _cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        Err(DbError::unsupported("이 드라이버는 행 편집을 지원하지 않습니다"))
    }
    async fn apply_edits(
        &self,
        _req: &EditRequest,
        _cancel: CancelToken,
    ) -> Result<EditResult, DbError> {
        Err(DbError::unsupported("이 드라이버는 행 편집을 지원하지 않습니다"))
    }
    async fn schema_graph(&self, _scope: Option<&str>) -> Result<SchemaGraph, DbError> {
        Err(DbError::unsupported("이 드라이버는 스키마 그래프를 지원하지 않습니다"))
    }
    async fn preview(
        &self,
        _req: &ExecRequest,
        _cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        Err(DbError::unsupported("이 드라이버는 변경 미리보기를 지원하지 않습니다"))
    }
    async fn ping(&self) -> Result<(), DbError>;
    async fn server_info(&self) -> Result<String, DbError>;
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError>;
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError>;
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError>;
    async fn expand(&self, element_id: &str, limit: u32) -> Result<ResultSet, DbError>;
    async fn close(&self);
}

pub trait ResultSink: Send + Sync {
    fn start(&self, index: usize, result: &ResultSet) -> Result<(), DbError>;
    fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError>;
}

pub fn streamable_tables(results: &[ResultSet]) -> bool {
    results.iter().all(|r| r.graph.is_none() && r.documents.is_none())
        && results.iter().any(|r| r.table.as_ref().is_some_and(|t| t.rows.len() > 2000))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExplorerNode {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub kind: String,
    pub icon: Icon,
    pub has_children: bool,
    pub actions: Vec<NodeAction>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Icon {
    Folder,
    Table,
    Column,
    Key,
    Index,
    Label,
    Relationship,
    Property,
    Collection,
    Database,
    View,
    Function,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeAction {
    pub id: String,
    pub label: String,
    pub kind: NodeActionKind,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeActionKind {
    OpenSchemaGraph { scope: Option<String> },
    Browse,
    OpenQuery { text: String },
    Insert { text: String },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PageRequest {
    #[serde(default)]
    pub filter: Vec<(String, Value)>,
    pub offset: u64,
    pub limit: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SchemaGraph {
    pub entities: Vec<SchemaEntity>,
    pub relations: Vec<SchemaRelation>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaEntity {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub fields: Vec<SchemaField>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaField {
    pub name: String,
    pub type_name: String,
    pub pk: bool,
    pub nullable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaRelation {
    pub id: String,
    pub name: String,
    pub from: String,
    pub from_fields: Vec<String>,
    pub to: String,
    pub to_fields: Vec<String>,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecRequest {
    #[serde(default = "default_preview_sample_limit")]
    pub preview_sample_limit: u32,
    #[serde(default)]
    pub read_only: bool,
    pub text: String,
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
    #[serde(default = "default_row_limit")]
    pub row_limit: u32,
    #[serde(default = "default_graph_node_limit")]
    pub graph_node_limit: u32,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}
pub fn default_preview_sample_limit() -> u32 {
    50
}
pub fn default_row_limit() -> u32 {
    1_000
}
pub fn default_graph_node_limit() -> u32 {
    300
}
pub fn default_timeout_ms() -> u64 {
    30_000
}
pub fn clamp_row_limit(limit: u32) -> u32 {
    limit.clamp(1, 100_000)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResultSet {
    #[serde(default)]
    pub edit_target: Option<EditTarget>,
    pub title: String,
    pub table: Option<Table>,
    pub graph: Option<Graph>,
    pub documents: Option<Vec<Value>>,
    pub message: Option<String>,
    pub affected: Option<u64>,
    pub row_count: u64,
    pub truncated: bool,
    pub page: Option<PageInfo>,
    pub elapsed_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageInfo {
    pub offset: u64,
    pub limit: u32,
    pub has_more: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Column {
    pub name: String,
    pub type_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Graph {
    pub nodes: Vec<GNode>,
    pub rels: Vec<GRel>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GNode {
    pub id: String,
    pub labels: Vec<String>,
    pub props: Vec<(String, Value)>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GRel {
    pub id: String,
    pub rel_type: String,
    pub start: String,
    pub end: String,
    pub props: Vec<(String, Value)>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FloatValue {
    Number(f64),
    Special(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Null,
    Bool(bool),
    Int(String),
    Float(FloatValue),
    Decimal(String),
    Text(String),
    Bytes {
        len: u64,
        hex: String,
    },
    Date(String),
    Time(String),
    DateTime(String),
    DateTimeTz(String),
    Duration(String),
    Uuid(String),
    Json(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
    Node(GNode),
    Rel(GRel),
    Path {
        nodes: Vec<GNode>,
        rels: Vec<GRel>,
    },
    Point {
        srid: i32,
        x: f64,
        y: f64,
        z: Option<f64>,
    },
    Other {
        type_name: String,
        text: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbError {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: Option<String>,
    pub position: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorKind {
    Connection,
    Auth,
    Query,
    Timeout,
    Cancelled,
    Unsupported,
    Internal,
    Conflict,
}
impl DbError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: None,
            position: None,
        }
    }
    pub fn query(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Query, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }
}
impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
impl std::error::Error for DbError {}
#[derive(Default)]
pub struct Registry {
    drivers: RwLock<BTreeMap<String, Arc<dyn Driver>>>,
}
impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&self, driver: Arc<dyn Driver>) {
        self.drivers.write().unwrap().insert(driver.manifest().id.clone(), driver);
    }
    pub fn unregister(&self, id: &str) {
        self.drivers.write().unwrap().remove(id);
    }
    pub fn get(&self, id: &str) -> Option<Arc<dyn Driver>> {
        self.drivers.read().unwrap().get(id).cloned()
    }
    pub fn manifests(&self) -> Vec<DriverManifest> {
        self.drivers
            .read().unwrap()
            .values()
            .map(|d| d.manifest().clone())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditTarget {
    pub node_id: String,
    pub key_columns: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op")]
pub enum RowEdit {
    Update {
        key: Vec<(String, Value)>,
        set: Vec<(String, Value)>,
    },
    Insert {
        values: Vec<(String, Value)>,
    },
    Delete {
        key: Vec<(String, Value)>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditRequest {
    pub node_id: String,
    pub edits: Vec<RowEdit>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_preview_sample_limit")]
    pub preview_sample_limit: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditResult {
    pub affected: Vec<u64>,
    pub statements: Vec<String>,
}
pub mod edits;
