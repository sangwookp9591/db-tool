//! Bolt graph driver with bounded results and disposable execution connections.
mod explorer;
mod preview;
mod value;
use async_trait::async_trait;
use dbcore::*;
use neo4rs::{query, BoltType, Config, ConfigBuilder};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use value::{map_value, parameter, type_name, Collector};

pub struct Neo4jDriver {
    manifest: DriverManifest,
}
impl Default for Neo4jDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl Neo4jDriver {
    pub fn new() -> Self {
        Self {
            manifest: DriverManifest {
                id: "neo4j".into(),
                display_name: "Neo4j".into(),
                paradigm: Paradigm::Graph,
                query_language: QueryLanguage {
                    id: "cypher".into(),
                    display: "Cypher".into(),
                    editor_mode: "cypher".into(),
                    supports_params: true,
                },
                connection_fields: [
                    ("uri", "URI", Some("bolt://localhost:7687"), false),
                    ("database", "데이터베이스", None, false),
                    ("tlsCAFile", "TLS CA 파일", None, false),
                    ("user", "사용자", Some("neo4j"), false),
                    ("password", "비밀번호", None, true),
                ]
                .into_iter()
                .map(|(key, label, default, secret)| FieldSpec {
                    key: key.into(),
                    label: label.into(),
                    kind: if secret {
                        FieldKind::Password
                    } else if key == "tlsCAFile" {
                        FieldKind::FilePath
                    } else {
                        FieldKind::Text
                    },
                    required: key != "database" && key != "tlsCAFile",
                    default: default.map(str::to_owned),
                    secret,
                    placeholder: default.unwrap_or("").into(),
                })
                .collect(),
                capabilities: Capabilities {
                    schema_graph: true,
                    filtered_browse: true,
                    preview_changes: PreviewSupport::Transactional,
                    read_only_exec: true,
                    cancel: CancelSupport::Abort,
                    timeout: true,
                    params: true,
                    transactions: false,
                    paged_browse: true,
                    graph_expand: true,
                    multi_statement: false,
                    edit_rows: false,
                },
                default_query: "MATCH (n) RETURN n LIMIT 25".into(),
            },
        }
    }
}
#[async_trait]
impl Driver for Neo4jDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        let uri = cfg
            .fields
            .get("uri")
            .map(String::as_str)
            .unwrap_or("bolt://localhost:7687");
        let parsed = url::Url::parse(uri)
            .map_err(|_| DbError::new(ErrorKind::Connection, "Invalid Neo4j URI"))?;
        if !matches!(
            parsed.scheme(),
            "bolt" | "bolt+s" | "bolt+ssc" | "neo4j" | "neo4j+s" | "neo4j+ssc"
        ) || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(DbError::new(
                ErrorKind::Connection,
                "Use a supported Bolt URI and separate credentials",
            ));
        }
        let secret = cfg.secret.clone().unwrap_or_default();
        let mut builder = ConfigBuilder::default()
            .uri(uri)
            .user(
                cfg.fields
                    .get("user")
                    .map(String::as_str)
                    .unwrap_or("neo4j"),
            )
            .password(secret.clone())
            .fetch_size(64)
            .max_connections(1)
            .connection_timeout(Duration::from_millis(cfg.connect_timeout_ms.max(1)));
        if let Some(db) = cfg.fields.get("database").filter(|v| !v.trim().is_empty()) {
            builder = builder.db(db.as_str());
        }
        if let Some(path) = cfg.fields.get("tlsCAFile").filter(|v| !v.is_empty()) {
            if !matches!(parsed.scheme(), "bolt+s" | "neo4j+s") {
                return Err(DbError::new(
                    ErrorKind::Connection,
                    "TLS CA file requires bolt+s or neo4j+s",
                ));
            }
            builder = builder.with_client_certificate(path);
        }
        if matches!(
            parsed.scheme(),
            "bolt+s" | "bolt+ssc" | "neo4j+s" | "neo4j+ssc"
        ) {
            // Workspace feature unification can enable two providers; preserve an existing choice.
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let config = builder
            .build()
            .map_err(|_| DbError::new(ErrorKind::Connection, "Invalid Neo4j configuration"))?;
        let session = NeoSession {
            config,
            secret,
            closed: AtomicBool::new(false),
        };
        tokio::time::timeout(
            Duration::from_millis(cfg.connect_timeout_ms.max(1)),
            session.ping(),
        )
        .await
        .map_err(|_| DbError::new(ErrorKind::Connection, "Neo4j connection timed out"))??;
        Ok(Box::new(session))
    }
}
struct NeoSession {
    config: Config,
    secret: String,
    closed: AtomicBool,
}
impl NeoSession {
    fn error(&self, e: neo4rs::Error) -> DbError {
        let redact = |s: &str| {
            if self.secret.is_empty() {
                s.into()
            } else {
                s.replace(&self.secret, "***")
            }
        };
        match e {
            neo4rs::Error::Neo4j(e) => {
                let code = e.code();
                let kind =
                    if code.contains(".Security.Unauthorized") || code.contains("Authentication") {
                        ErrorKind::Auth
                    } else if code.contains("TimedOut") || code.contains("Timeout") {
                        ErrorKind::Timeout
                    } else {
                        ErrorKind::Query
                    };
                DbError {
                    kind,
                    message: redact(e.message()),
                    detail: Some(redact(code)),
                    position: None,
                }
            }
            neo4rs::Error::AuthenticationError(_) => {
                DbError::new(ErrorKind::Auth, "Neo4j authentication failed")
            }
            _ => DbError::new(
                ErrorKind::Connection,
                "Neo4j connection failed or was closed",
            ),
        }
    }
    async fn run(
        &self,
        req: &ExecRequest,
        row_limit: usize,
        detect_more: bool,
    ) -> Result<ResultSet, DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session is closed"));
        }
        let start = Instant::now();
        // An execution owns its pool: dropping this future also destroys any interrupted socket.
        let graph = neo4rs::Graph::connect(self.config.clone()).map_err(|e| self.error(e))?;
        let mut q = query(&req.text).extra(
            "tx_timeout",
            i64::try_from(req.timeout_ms.max(1)).unwrap_or(i64::MAX),
        );
        for (key, v) in &req.params {
            q = q.param(key, parameter(v)?);
        }
        let mut stream = if req.read_only {
            graph.execute_read(q).await
        } else {
            graph.execute(q).await
        }
        .map_err(|e| self.error(e))?;
        let mut columns: Vec<Column> = stream
            .fields()
            .iter()
            .map(|field| match field {
                BoltType::String(name) => Ok(Column {
                    name: name.value.clone(),
                    type_name: "Null".into(),
                }),
                _ => Err(DbError::query("Invalid Bolt column name")),
            })
            .collect::<Result<_, DbError>>()?;
        let mut rows = Vec::new();
        let mut collector = Collector::default();
        let mut truncated = false;
        while let Some(row) = stream.next().await.map_err(|e| self.error(e))? {
            if rows.len() == row_limit {
                truncated = true;
                break;
            }
            let mut cells = Vec::with_capacity(columns.len());
            for col in &mut columns {
                let raw: BoltType = row
                    .get(&col.name)
                    .map_err(|_| DbError::query("Cannot decode Bolt record"))?;
                let cell = map_value(&raw)?;
                if col.type_name == "Null" && cell != Value::Null {
                    col.type_name = type_name(&cell).into();
                }
                collector.collect(&cell, req.graph_node_limit as usize);
                cells.push(cell);
            }
            rows.push(cells);
            if !detect_more && rows.len() == row_limit {
                truncated = true;
                break;
            }
        }
        let summary = stream.finish().await.map_err(|e| self.error(e))?;
        let writes = matches!(
            summary.query_type(),
            neo4rs::summary::Type::Write
                | neo4rs::summary::Type::ReadWrite
                | neo4rs::summary::Type::SchemaOnly
        );
        let stats = summary.stats();
        let counters = [
            ("nodes created", stats.nodes_created),
            ("nodes deleted", stats.nodes_deleted),
            ("relationships created", stats.relationships_created),
            ("relationships deleted", stats.relationships_deleted),
            ("properties set", stats.properties_set),
            ("labels added", stats.labels_added),
            ("labels removed", stats.labels_removed),
            ("indexes added", stats.indexes_added),
            ("indexes removed", stats.indexes_removed),
            ("constraints added", stats.constraints_added),
            ("constraints removed", stats.constraints_removed),
            ("system updates", stats.system_updates),
        ];
        let affected = counters.iter().map(|(_, v)| v).sum::<u64>();
        let message = counters
            .iter()
            .filter(|(_, v)| *v > 0)
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        truncated |= collector.truncated;
        Ok(ResultSet {
            title: result_title(req.text.lines().next().unwrap_or("")),
            row_count: rows.len() as u64,
            table: Some(Table { columns, rows }),
            graph: collector.finish(),
            message: if !message.is_empty() {
                Some(message)
            } else if writes {
                Some("No updates".into())
            } else {
                None
            },
            affected: if writes || affected > 0 {
                Some(affected)
            } else {
                None
            },
            truncated,
            elapsed_ms: start.elapsed().as_millis() as u64,
            ..Default::default()
        })
    }
    async fn bounded(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
        detect_more: bool,
    ) -> Result<ResultSet, DbError> {
        // Internal browse may retain one extra row solely to determine has_more.
        let row_limit = if detect_more {
            req.row_limit.clamp(1, 100_001)
        } else {
            clamp_row_limit(req.row_limit)
        };
        tokio::select! { biased;
            _=cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"실행이 취소되어 연결을 폐기했습니다")),
            _=tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)))=>Err(DbError::new(ErrorKind::Timeout,"실행 시간이 초과되어 연결을 폐기했습니다")),
            result=self.run(req,row_limit as usize,detect_more)=>result,
        }
    }
    async fn internal(
        &self,
        text: String,
        params: serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResultSet, DbError> {
        self.bounded(
            &ExecRequest {
                text,
                params,
                read_only: true,
                row_limit: 100_000,
                graph_node_limit: 300,
                timeout_ms: 30_000,
                preview_sample_limit: default_preview_sample_limit(),
            },
            CancelToken::new(),
            true,
        )
        .await
    }
}
#[async_trait]
impl Session for NeoSession {
    async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> { self.load_schema_graph(scope).await }
    async fn preview(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        self.preview_changes(req, cancel).await
    }
    async fn ping(&self) -> Result<(), DbError> {
        self.internal("RETURN 1".into(), Default::default()).await?;
        Ok(())
    }
    async fn server_info(&self) -> Result<String, DbError> {
        let result = self
            .internal(
                "CALL dbms.components() YIELD name, versions, edition RETURN name, versions, edition".into(),
                Default::default(),
            )
            .await?;
        let table = result
            .table
            .ok_or_else(|| DbError::query("Missing server information"))?;
        let row = table.rows.first().ok_or_else(|| DbError::query("Missing server version"))?;
        let version = first_text(row.get(1)).ok_or_else(|| DbError::query("Missing server version"))?;
        let edition = row.get(2).and_then(|value| first_text(Some(value))).filter(|value| !value.is_empty());
        Ok(match edition {
            Some(edition) => format!("Neo4j {version} ({edition})"),
            None => format!("Neo4j {version}"),
        })
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        Ok(vec![self.bounded(req, cancel, false).await?])
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        self.explore(parent).await
    }
    async fn browse(&self, id: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        self.browse_node(id, page).await
    }
    async fn expand(&self, id: &str, limit: u32) -> Result<ResultSet, DbError> {
        let (predicate, param) = match id.parse::<i64>() {
            Ok(n) => ("id(n)", serde_json::json!(n)),
            Err(_) => ("elementId(n)", serde_json::json!(id)),
        };
        let limit = clamp_row_limit(limit);
        let params = serde_json::json!({"id":param,"limit":limit})
            .as_object()
            .unwrap()
            .clone();
        self.bounded(
            &ExecRequest {
                read_only: true,
                text: format!("MATCH (n) WHERE {predicate} = $id MATCH (n)-[r]-(m) RETURN n, r, m LIMIT $limit"),
                params,
                row_limit: limit,
                graph_node_limit: limit.saturating_add(1),
                timeout_ms: 30_000,
            preview_sample_limit: default_preview_sample_limit(),
            },
            CancelToken::new(),
            true,
        ).await
    }
    async fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}

fn first_text(value: Option<&Value>) -> Option<&str> {
    match value? {
        Value::Text(text) => Some(text),
        Value::List(values) => values.iter().find_map(|value| first_text(Some(value))),
        _ => None,
    }
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
mod server_info_tests {
    use super::*;

    #[test]
    fn result_title_caps_at_80_chars_with_ellipsis() {
        assert_eq!(result_title("MATCH (n) RETURN n"), "MATCH (n) RETURN n");
        let exact = "a".repeat(80);
        assert_eq!(result_title(&exact), exact);
        let long = "한".repeat(81);
        assert_eq!(result_title(&long), format!("{}…", "한".repeat(80)));
    }

    #[test]
    fn extracts_version_from_nested_component_values() {
        let value = Value::List(vec![Value::List(vec![Value::Text("5.26.30".into())])]);
        assert_eq!(first_text(Some(&value)), Some("5.26.30"));
    }
}
