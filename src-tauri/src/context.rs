//! Shared, bounded context for the in-app assistant and agent tools. Never executes user text.
use crate::{permissions::AccessMode, storage, Host};
use dbcore::{Column, DbError, ErrorKind, NodeActionKind, ResultSet, SchemaGraph, Table};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

const SCHEMA_TTL: Duration = Duration::from_secs(60);
#[derive(Default)]
pub(crate) struct SchemaCache {
    generation: u64,
    graphs: HashMap<(String, Option<String>), (Instant, SchemaGraph)>,
}
impl SchemaCache {
    pub(crate) fn invalidate(&mut self, session: &str) {
        self.generation = self.generation.wrapping_add(1);
        self.graphs.retain(|(id, _), _| id != session);
    }
}
async fn cached_graph(host: &Host, id: &str, scope: Option<&str>) -> Result<SchemaGraph, DbError> {
    let key = (id.to_owned(), scope.map(str::to_owned));
    let generation = if let Ok(mut cache) = host.schema_cache.lock() {
        cache.graphs.retain(|_, (at, _)| at.elapsed() < SCHEMA_TTL);
        if let Some((_, graph)) = cache.graphs.get(&key) {
            return Ok(graph.clone());
        }
        cache.generation
    } else {
        0
    };
    let record = host.session(id)?;
    let graph = match record.session.schema_graph(scope).await {
        Ok(graph) => graph,
        Err(e) if e.kind == ErrorKind::Unsupported => SchemaGraph::default(),
        Err(e) => return Err(e),
    };
    if !record.dead.load(std::sync::atomic::Ordering::Acquire) {
        if let Ok(mut cache) = host.schema_cache.lock() {
            // Do not repopulate a cache invalidated while this read was in flight.
            if cache.generation == generation {
                cache.graphs.insert(key, (Instant::now(), graph.clone()));
            }
        }
    }
    Ok(graph)
}
pub const SCHEMA_BYTES: usize = 6000; // Conservative bound: at most one token per UTF-8 byte.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scope {
    pub focus: Option<String>,
    pub text: String,
    pub selection: String,
    pub run: Option<String>,
    pub include_rows: bool,
    pub read_only: bool,
    pub selected_rows: Option<Table>,
    pub masked_columns: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct WorkInput {
    pub session_id: String,
    pub scope: Scope,
}
#[derive(Clone, Serialize)]
pub struct RunContext {
    pub run: String,
    pub session_id: String,
    pub text: String,
    pub error: Option<Value>,
    pub results: Vec<Value>,
}
pub fn remember(
    host: &Host,
    session: &str,
    run: &str,
    text: &str,
    result: Result<&[ResultSet], &DbError>,
) {
    // A multi-statement execution may change metadata before returning an error.
    // Conservatively invalidate for every execution rather than parse driver-specific DDL.
    if let Ok(mut cache) = host.schema_cache.lock() {
        cache.invalidate(session);
    }
    let secret = host.sessions.lock().ok().and_then(|sessions| {
        sessions
            .get(session)
            .and_then(|record| record.secret.clone())
    });
    let item = RunContext {
        run: run.into(), session_id: session.into(), text: clipped(&storage::redact(text, secret.as_deref()), 8000),
        error: result.as_ref().err().map(|e| json!({"kind": e.kind, "message": clipped(&storage::redact(&e.message, secret.as_deref()), 2000)})),
        results: result.as_ref().map(|results| results.iter().take(20).map(|r| json!({
            "columns": r.table.as_ref().map(|t| t.columns.iter().take(100).map(|c| Column {name:clipped(&c.name,128),type_name:clipped(&c.type_name,128)}).collect::<Vec<_>>()),
            "row_count":r.row_count,"truncated":r.truncated,"affected":r.affected
        })).collect()).unwrap_or_default(),
    };
    if let Ok(mut runs) = host.context_runs.lock() {
        if runs.len() >= 20 {
            runs.pop_front();
        }
        runs.push_back(item);
    }
}
pub fn run(host: &Host, id: &str) -> Result<RunContext, DbError> {
    host.context_runs
        .lock()
        .map_err(|_| DbError::internal("Context unavailable"))?
        .iter()
        .find(|r| r.run == id)
        .cloned()
        .ok_or_else(|| DbError::query("Run context expired or not found"))
}
fn clipped(text: &str, bytes: usize) -> String {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
fn sensitive(name: &str, masked: &[String]) -> bool {
    let name = name.to_lowercase();
    masked.iter().any(|m| m.eq_ignore_ascii_case(&name))
        || [
            "password",
            "passwd",
            "secret",
            "token",
            "api_key",
            "apikey",
            "credential",
            "authorization",
            "private_key",
            "ssn",
            "credit_card",
        ]
        .iter()
        .any(|s| name.contains(s))
}
fn rows(scope: &Scope) -> Option<Table> {
    if !scope.include_rows {
        return None;
    }
    let table = scope.selected_rows.as_ref()?;
    let indices = table
        .columns
        .iter()
        .enumerate()
        .take(100)
        .filter(|(_, c)| !sensitive(&c.name, &scope.masked_columns))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    Some(Table {
        columns: indices.iter().map(|i| table.columns[*i].clone()).collect(),
        rows: table
            .rows
            .iter()
            .take(20)
            .map(|row| {
                indices
                    .iter()
                    .map(|i| {
                        // Structured values may hide credentials under arbitrary nested paths; omit them entirely.
                        match row.get(*i) {
                            Some(
                                dbcore::Value::Json(_)
                                | dbcore::Value::List(_)
                                | dbcore::Value::Map(_)
                                | dbcore::Value::Bytes { .. }
                                | dbcore::Value::Node(_)
                                | dbcore::Value::Rel(_)
                                | dbcore::Value::Path { .. },
                            )
                            | None => dbcore::Value::Null,
                            Some(value) => value.clone(),
                        }
                    })
                    .collect()
            })
            .collect(),
    })
}
pub fn summarize(graph: &SchemaGraph, focus: &str, budget: usize) -> (String, bool) {
    let focus = focus.to_lowercase();
    let mut entities = graph.entities.iter().collect::<Vec<_>>();
    entities.sort_by_key(|e| {
        (
            !(focus.contains(&e.name.to_lowercase()) || e.id.to_lowercase().contains(&focus)),
            std::cmp::Reverse(e.name.len()),
            &e.id,
        )
    });
    let mut lines = Vec::new();
    for e in entities {
        lines.push(format!("{} {} [{}]", e.kind, e.name, e.id));
        for r in graph.relations.iter().filter(|r| r.from == e.id) {
            lines.push(format!(
                "  {} {}({}) -> {}({})",
                r.kind,
                r.name,
                r.from_fields.join(","),
                r.to,
                r.to_fields.join(",")
            ));
        }
        for f in &e.fields {
            lines.push(format!(
                "  {}: {}{}{}",
                f.name,
                f.type_name,
                if f.pk { " PK" } else { "" },
                if f.nullable { " nullable" } else { "" }
            ));
        }
    }
    let all = lines.join("\n");
    (clipped(&all, budget), all.len() > budget)
}
async fn schema(host: &Host, id: &str, focus: &str, partial: &mut (String, bool)) -> Result<(String, bool), DbError> {
    let record = host.session(id)?;
    let manifest = host.driver(&record.profile.driver_id)?.manifest().clone();
    let mut root_nodes = None;
    if manifest.capabilities.schema_graph {
        let initial = cached_graph(host, id, None).await?;
        // Focus only affects ranking: never discard a graph that already has metadata.
        if !initial.entities.is_empty() {
            return Ok(summarize(&initial, focus, SCHEMA_BYTES));
        }
        let mut roots = record.session.list_children(None).await?;
        let focus_lower = focus.to_lowercase();
        roots.sort_by_key(|n| !(focus_lower.contains(&n.label.to_lowercase()) || focus_lower.contains(&n.id.to_lowercase())));
        root_nodes = Some(roots.clone());
        let limited = roots.len() > 100;
        let mut chunks = Vec::new();
        let mut scopes = HashSet::from([None]);
        let mut entities = HashSet::new();
        for root in roots.into_iter().take(100) {
            let scope = root
                .actions
                .iter()
                .find_map(|action| match &action.kind {
                    NodeActionKind::OpenSchemaGraph { scope } => Some(scope.as_deref()),
                    _ => None,
                })
                .unwrap_or(Some(&root.id));
            if !scopes.insert(scope.map(str::to_owned)) {
                continue;
            }
            // Session has no concurrent-metadata capability contract; keep loads serial.
            let mut graph = cached_graph(host, id, scope).await?;
            graph
                .entities
                .retain(|entity| entities.insert(entity.id.clone()));
            if graph.entities.is_empty() {
                continue;
            }
            let matches = focus_lower.contains(&root.label.to_lowercase())
                || focus_lower.contains(&root.id.to_lowercase())
                || graph.entities.iter().any(|e| {
                    focus_lower.contains(&e.name.to_lowercase())
                        || e.id.to_lowercase().contains(&focus_lower)
                });
            let (summary, truncated) = summarize(&graph, focus, SCHEMA_BYTES);
            chunks.push((
                !matches,
                format!("scope {}\n{}", root.label, summary),
                truncated,
            ));
            let mut ranked = chunks.clone();
            ranked.sort_by_key(|c| c.0);
            partial.0 = clipped(&ranked.iter().map(|c| c.1.as_str()).collect::<Vec<_>>().join("\n"), SCHEMA_BYTES);
            partial.1 = true;
        }
        if !chunks.is_empty() {
            chunks.sort_by_key(|c| c.0);
            let mut summary = String::new();
            let mut truncated = limited;
            for (_, chunk, clipped_chunk) in chunks {
                truncated |= clipped_chunk;
                if !summary.is_empty() {
                    summary.push('\n');
                }
                summary.push_str(&chunk);
                if summary.len() > SCHEMA_BYTES {
                    return Ok((clipped(&summary, SCHEMA_BYTES), true));
                }
            }
            return Ok((summary, truncated));
        }
    }
    let mut queue = VecDeque::from([None]);
    let mut seen = HashSet::new();
    let mut summary = String::new();
    let mut visits = 0;
    let mut metadata = false;
    let mut incomplete = false;
    while let Some(parent) = queue.pop_front() {
        visits += 1;
        if visits > 100 || summary.len() >= SCHEMA_BYTES {
            return Ok((clipped(&summary, SCHEMA_BYTES), true));
        }
        let fetched = if parent.is_none() && root_nodes.is_some() {
            root_nodes.take().ok_or_else(|| DbError::internal("Schema roots unavailable"))
        } else {
            record.session.list_children(parent.as_deref()).await
        };
        let mut nodes = match fetched {
            Ok(nodes) => nodes,
            // Metadata access can differ per collection even when its parent is visible.
            Err(e) if e.kind == ErrorKind::Auth && parent.is_some() => {
                incomplete = true;
                continue;
            }
            Err(e) => return Err(e),
        };
        nodes.sort_by_key(|n| !focus.to_lowercase().contains(&n.label.to_lowercase()));
        for node in nodes {
            if !seen.insert(node.id.clone()) {
                continue;
            }
            metadata |= !matches!(node.kind.as_str(), "folder" | "database" | "schema");
            summary.push_str(&format!(
                "{} {} {}\n",
                node.kind,
                node.label,
                node.detail.as_deref().unwrap_or("")
            ));
            if metadata {
                *partial = (clipped(&summary, SCHEMA_BYTES), true);
            }
            if summary.len() >= SCHEMA_BYTES {
                return Ok((clipped(&summary, SCHEMA_BYTES), true));
            }
            if node.has_children {
                queue.push_back(Some(node.id));
            }
        }
    }
    Ok((if metadata { summary } else { String::new() }, incomplete))
}
pub async fn build(host: &Host, session_id: &str, scope: Scope) -> Result<Value, DbError> {
    build_with_timeout(host, session_id, scope, Duration::from_secs(15)).await
}
async fn build_with_timeout(host: &Host, session_id: &str, scope: Scope, timeout: Duration) -> Result<Value, DbError> {
    let record = host.session(session_id)?;
    let manifest = host.driver(&record.profile.driver_id)?.manifest().clone();
    let recent = scope.run.as_deref().and_then(|id| run(host, id).ok());
    if recent.as_ref().is_some_and(|r| r.session_id != session_id) {
        return Err(DbError::query("Run belongs to another session"));
    }
    let last_results = recent.as_ref().map(|recent| {
        if recent.error.is_none() {
            return recent.results.clone();
        }
        host.context_runs
            .lock()
            .ok()
            .and_then(|runs| {
                runs.iter()
                    .rev()
                    .skip_while(|r| r.run != recent.run)
                    .skip(1)
                    .find(|r| r.session_id == session_id && r.error.is_none())
                    .map(|r| r.results.clone())
            })
            .unwrap_or_default()
    });
    let focus = scope.focus.as_deref().unwrap_or(&scope.text);
    let mut partial = (String::new(), false);
    let (summary, truncated, timed_out) = match tokio::time::timeout(
        timeout, schema(host, session_id, focus, &mut partial),
    ).await {
        Ok(Err(error)) if error.kind == ErrorKind::Timeout && !partial.0.is_empty() => (partial.0, true, true),
        Ok(result) => {
            let (summary, truncated) = result?;
            (summary, truncated, false)
        }
        Err(_) if !partial.0.is_empty() => (partial.0, true, true),
        Err(_) => return Err(DbError::new(ErrorKind::Timeout, "Schema context timed out")),
    };
    let schema_note = if timed_out {
        Some("Schema context is partial because metadata loading timed out")
    } else if summary.is_empty() {
        Some("No schema metadata was returned by the driver graph or explorer")
    } else {
        None
    };
    let mode = if scope.read_only { AccessMode::ReadOnly } else { host.access_mode(&record)? };
    let value = json!({"session_id":session_id,"schema_note":schema_note,"schema":summary,"schema_truncated":truncated,"schema_byte_limit":SCHEMA_BYTES,
        "query_language":manifest.query_language,"paradigm":manifest.paradigm,"text":clipped(&storage::redact(&scope.text,record.secret.as_deref()),8000),"selection":clipped(&storage::redact(&scope.selection,record.secret.as_deref()),8000),
        "run":recent.as_ref().map(|r|&r.run),"run_context_expired":scope.run.is_some() && recent.is_none(),"error":recent.as_ref().and_then(|r|r.error.as_ref()),
        "results":last_results,"run_text":recent.as_ref().map(|r|&r.text),"access_mode":mode,
        "permissions":{"read":true,"write":mode!=AccessMode::ReadOnly,"write_requires_preview_and_approval":true},
        "selected_rows":rows(&scope)});
    let encoded =
        serde_json::to_string(&value).map_err(|_| DbError::internal("Context encoding failed"))?;
    if encoded.len() > 128_000 {
        return Err(DbError::query(
            "Selected context exceeds 128 KB; select fewer or smaller rows",
        ));
    }
    // Redact before serialization to preserve escaping for credentials containing quotes/backslashes.
    fn redact(value: &mut Value, secret: Option<&str>) {
        match value {
            Value::String(s) => *s = storage::redact(s, secret),
            Value::Array(a) => a.iter_mut().for_each(|v| redact(v, secret)),
            Value::Object(o) => o.values_mut().for_each(|v| redact(v, secret)),
            _ => {}
        }
    }
    let mut value = value;
    redact(&mut value, record.secret.as_deref());
    for field in manifest.connection_fields.iter().filter(|f| f.secret) {
        redact(
            &mut value,
            record.profile.fields.get(&field.key).map(String::as_str),
        );
    }
    Ok(value)
}
#[tauri::command]
pub async fn context_get(
    host: tauri::State<'_, Host>,
    session_id: String,
    scope: Scope,
) -> Result<Value, DbError> {
    build(&host, &session_id, scope).await
}
#[tauri::command]
pub async fn context_error(host: tauri::State<'_, Host>, run_id: String) -> Result<Value, DbError> {
    let recent = run(&host, &run_id)?;
    build(
        &host,
        &recent.session_id,
        Scope {
            text: recent.text,
            run: Some(run_id),
            ..Default::default()
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbcore::*;
    use std::{
        collections::BTreeMap,
        sync::{atomic::AtomicBool, Arc},
    };
    fn host() -> (Host, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("axdb-ai1-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (
            Host::new(storage::Paths {
                config: dir.clone(),
                data: dir.clone(),
                log: dir.clone(),
            }),
            dir,
        )
    }
    struct MetadataSession {
        calls: std::sync::Mutex<Vec<Option<String>>>,
        initial: bool,
        stall: bool,
    }
    fn graph() -> SchemaGraph {
        SchemaGraph { entities: vec![SchemaEntity {
            id: "items".into(), name: "items".into(), kind: "table".into(), fields: vec![],
        }], relations: vec![] }
    }
    #[async_trait::async_trait]
    impl Session for MetadataSession {
        async fn ping(&self) -> Result<(), DbError> { Ok(()) }
        async fn server_info(&self) -> Result<String, DbError> { Ok("test".into()) }
        async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> {
            self.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(scope.map(str::to_owned));
            if self.stall && (self.initial || scope == Some("slow")) {
                std::future::pending::<()>().await;
            }
            Ok(if self.initial || scope == Some("ready") { graph() } else { SchemaGraph::default() })
        }
        async fn list_children(&self, _: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
            Ok([None, Some("ready"), Some("ready"), Some("slow")].into_iter().enumerate().map(|(i, scope)| ExplorerNode {
                id: i.to_string(), label: i.to_string(), kind: "schema".into(), detail: None,
                icon: Icon::Database, has_children: false,
                actions: vec![NodeAction { id: "graph".into(), label: "Graph".into(), kind: NodeActionKind::OpenSchemaGraph { scope: scope.map(str::to_owned) } }],
            }).collect())
        }
        async fn browse(&self, _: &str, _: PageRequest) -> Result<ResultSet, DbError> { Err(DbError::unsupported("test")) }
        async fn execute(&self, _: &ExecRequest, _: CancelToken) -> Result<Vec<ResultSet>, DbError> { Err(DbError::unsupported("test")) }
        async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> { Err(DbError::unsupported("test")) }
        async fn close(&self) {}
    }
    fn metadata_host(initial: bool, stall: bool) -> (Host, Arc<MetadataSession>, std::path::PathBuf) {
        let (host, dir) = host();
        host.registry.register(Arc::new(driver_sqlite::SqliteDriver::new()));
        let session = Arc::new(MetadataSession { calls: Default::default(), initial, stall });
        host.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert("s".into(), Arc::new(crate::SessionRecord {
            session: session.clone(),
            profile: storage::Profile {
                id: "context-test".into(), name: "Context test".into(), driver_id: "sqlite".into(),
                fields: Default::default(), save_secret: false, network: None,
                auth: Default::default(), access_mode: Default::default(),
            },
            secret: None, _tunnel: None, dead: AtomicBool::new(false),
        }));
        (host, session, dir)
    }
    #[tokio::test]
    async fn context_cache_hit_expiry_and_execution_invalidation() {
        let (host, session, dir) = metadata_host(true, false);
        for focus in ["items", "different focus"] {
            let value = build(&host, "s", Scope { focus: Some(focus.into()), ..Default::default() }).await.unwrap();
            assert!(value["schema"].as_str().unwrap().contains("items"));
        }
        assert_eq!(*session.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner), vec![None]);
        host.schema_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).graphs.values_mut().for_each(|(at, _)| *at = Instant::now() - SCHEMA_TTL);
        build(&host, "s", Scope::default()).await.unwrap();
        assert_eq!(session.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 2);
        remember(&host, "s", "ddl", "CREATE TABLE next(id int)", Ok(&[]));
        build(&host, "s", Scope::default()).await.unwrap();
        assert_eq!(session.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 3);
        host.schema_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).invalidate("s");
        assert!(host.schema_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).graphs.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn context_each_scope_is_read_once_including_none() {
        let (host, session, dir) = metadata_host(false, false);
        for _ in 0..2 { build(&host, "s", Scope::default()).await.unwrap(); }
        assert_eq!(*session.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner), vec![None, Some("ready".into()), Some("slow".into())]);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn context_timeout_returns_partial_metadata_and_note() {
        let (host, session, dir) = metadata_host(false, true);
        let value = build_with_timeout(&host, "s", Scope::default(), Duration::from_millis(20)).await.unwrap();
        assert!(value["schema"].as_str().unwrap().contains("items"));
        assert_eq!(value["schema_truncated"], true);
        assert!(value["schema_note"].as_str().unwrap().contains("partial"));
        assert_eq!(*session.calls.lock().unwrap_or_else(std::sync::PoisonError::into_inner), vec![None, Some("ready".into()), Some("slow".into())]);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn context_timeout_without_metadata_is_an_error() {
        let (host, _, dir) = metadata_host(true, true);
        let error = build_with_timeout(&host, "s", Scope::default(), Duration::from_millis(20)).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rows_require_consent_and_remove_sensitive_and_nested_values() {
        let table = Table {
            columns: ["name", "password", "custom", "payload"]
                .map(|n| Column {
                    name: n.into(),
                    type_name: "text".into(),
                })
                .into(),
            rows: vec![
                vec![
                    dbcore::Value::Text("Alice".into()),
                    dbcore::Value::Text("secret".into()),
                    dbcore::Value::Text("hidden".into()),
                    dbcore::Value::Json("{\"token\":\"nested-secret\"}".into())
                ];
                30
            ],
        };
        let mut scope = Scope {
            selected_rows: Some(table),
            masked_columns: vec!["CUSTOM".into()],
            ..Default::default()
        };
        assert!(rows(&scope).is_none());
        scope.include_rows = true;
        let table = rows(&scope).unwrap();
        assert_eq!(table.rows.len(), 20);
        assert_eq!(table.columns.len(), 2);
        let encoded = serde_json::to_string(&table).unwrap();
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("hidden"));
        assert!(encoded.contains("Alice"));
    }
    #[test]
    fn schema_budget_keeps_focus_first_and_utf8_intact() {
        let graph = SchemaGraph {
            entities: (0..500)
                .map(|i| SchemaEntity {
                    id: format!("entity{i}"),
                    name: format!("테이블{i}"),
                    kind: "table".into(),
                    fields: vec![SchemaField {
                        name: "id".into(),
                        type_name: "integer".into(),
                        pk: true,
                        nullable: false,
                    }],
                })
                .collect(),
            relations: vec![],
        };
        let (summary, truncated) = summarize(&graph, "테이블499", 99);
        assert!(summary.starts_with("table 테이블499"));
        assert!(summary.len() <= 99);
        assert!(truncated);
    }
    async fn verify(
        driver: Arc<dyn Driver>,
        cfg: ConnectConfig,
        setup: &str,
        cleanup: &str,
        entity: &str,
        relation: &str,
        root_focus: &str,
    ) {
        let session = driver.connect(&cfg).await.unwrap();
        if root_focus.starts_with("ai1b_scope_") {
            assert!(
                session
                    .schema_graph(None)
                    .await
                    .unwrap()
                    .entities
                    .is_empty(),
                "regression fixture requires empty public schema"
            );
        }
        let req = |text: &str| serde_json::from_value::<ExecRequest>(json!({"text":text})).unwrap();
        session
            .execute(&req(setup), CancelToken::new())
            .await
            .unwrap();
        let (host, dir) = host();
        host.registry.register(driver.clone());
        let profile = storage::Profile {
            id: "ai-test".into(),
            name: "AI test".into(),
            driver_id: driver.manifest().id.clone(),
            fields: cfg.fields,
            save_secret: false, network: None, auth: Default::default(),
            access_mode: Default::default(),
        };
        host.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(
            "s".into(),
            Arc::new(crate::SessionRecord {
                session: Arc::from(session),
                profile,
                secret: Some("private-password".into()),
                _tunnel: None,
                dead: AtomicBool::new(false),
            }),
        );
        let previous = [ResultSet {
            row_count: 1,
            table: Some(Table {
                columns: vec![Column {
                    name: "note".into(),
                    type_name: "text".into(),
                }],
                rows: vec![vec![dbcore::Value::Text("row-private-value".into())]],
            }),
            ..Default::default()
        }];
        remember(&host, "s", "previous", "SELECT note", Ok(&previous));
        let result: Result<Vec<ResultSet>, DbError> =
            Err(DbError::query("bad query private-password"));
        remember(
            &host,
            "s",
            "r",
            "SELECT 'private-password'",
            result.as_deref(),
        );
        for focus in [None, Some(entity.into()), Some(root_focus.into())] {
            let read_only = focus.is_some();
            let value = build(
                &host,
                "s",
                Scope {
                    text: if focus.is_none() {
                        String::new()
                    } else {
                        "SELECT 'private-password'".into()
                    },
                    focus,
                    read_only,
                    run: Some("r".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            assert!(
                value["schema"].as_str().unwrap().contains(entity),
                "{value}"
            );
            assert!(
                value["schema"].as_str().unwrap().contains(relation),
                "{value}"
            );
            if entity.is_empty() {
                assert_eq!(value["schema"], "");
                assert!(value["schema_note"].as_str().is_some());
            }
            assert!(!value.to_string().contains("private-password"));
            assert!(!value.to_string().contains("row-private-value"));
            assert_eq!(value["permissions"]["write"], !read_only);
            assert!(value["selected_rows"].is_null());
            assert_eq!(value["results"][0]["row_count"], 1);
            assert_eq!(value["results"][0]["columns"][0]["name"], "note");
            assert_eq!(
                value["query_language"],
                json!(driver.manifest().query_language)
            );
            assert!(value["schema"].as_str().unwrap().len() <= SCHEMA_BYTES);
        }
        let record = host.session("s").unwrap();
        record
            .session
            .execute(&req(cleanup), CancelToken::new())
            .await
            .unwrap();
        record.session.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn sqlite_real_context() {
        verify(Arc::new(driver_sqlite::SqliteDriver::new()),ConnectConfig{fields:BTreeMap::from([("path".into(),":memory:".into())]),secret:None,connect_timeout_ms:3000},
            "CREATE TABLE ai_parent(id INTEGER PRIMARY KEY); CREATE TABLE ai_child(id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES ai_parent(id), note TEXT); INSERT INTO ai_child VALUES(1,NULL,'row-private-value');",
            "DROP TABLE ai_child; DROP TABLE ai_parent;","ai_child","foreign_key", "main").await;
    }
    #[tokio::test]
    async fn sqlite_empty_context_explains_missing_metadata() {
        verify(
            Arc::new(driver_sqlite::SqliteDriver::new()),
            ConnectConfig {
                fields: BTreeMap::from([("path".into(), ":memory:".into())]),
                secret: None,
                connect_timeout_ms: 3000,
            },
            "SELECT 1",
            "SELECT 1",
            "",
            "",
            "main",
        )
        .await;
    }
    #[tokio::test]
    #[allow(clippy::print_stderr)] // Intentional skip notice for optional live integration tests.
    async fn postgres_real_context() {
        let Ok(target) = std::env::var("AXDB_TEST_PG") else {
            eprintln!("AXDB_TEST_PG unset: live AI context not run");
            return;
        };
        assert!(
            target.contains("host=127.0.0.1")
                && target.contains("port=55432")
                && (target.contains("dbname=axdb_test") || target.contains("dbname=dbtool_test"))
        );
        let database = if target.contains("dbname=dbtool_test") {
            "dbtool_test"
        } else {
            "axdb_test"
        };
        let schema = format!("ai1b_scope_{}", uuid::Uuid::new_v4().simple());
        verify(Arc::new(driver_postgres::PostgresDriver::new()),ConnectConfig{fields:BTreeMap::from([("host".into(),"127.0.0.1".into()),("port".into(),"55432".into()),("database".into(),database.into()),("user".into(),"postgres".into())]),secret:Some("testpw".into()),connect_timeout_ms:3000},
            &format!("CREATE SCHEMA {schema}; CREATE TABLE {schema}.ai1_parent(id INTEGER PRIMARY KEY); CREATE TABLE {schema}.ai1_child(id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES {schema}.ai1_parent(id), note TEXT); INSERT INTO {schema}.ai1_child VALUES(1,NULL,'row-private-value');"),
            &format!("DROP SCHEMA {schema} CASCADE"),"ai1_child","foreign_key", &schema).await;
    }
    #[tokio::test]
    #[allow(clippy::print_stderr)] // Intentional skip notice for optional live integration tests.
    async fn neo4j_real_context() {
        let Ok(target) = std::env::var("AXDB_TEST_NEO4J") else {
            eprintln!("AXDB_TEST_NEO4J unset: live AI context not run");
            return;
        };
        let parts = target.split('|').collect::<Vec<_>>();
        assert_eq!(parts[0], "bolt://127.0.0.1:57687");
        verify(
            Arc::new(driver_neo4j::Neo4jDriver::new()),
            ConnectConfig {
                fields: BTreeMap::from([
                    ("uri".into(), parts[0].into()),
                    ("user".into(), parts[1].into()),
                ]),
                secret: Some(parts[2].into()),
                connect_timeout_ms: 3000,
            },
            "CREATE (:Ai1Parent {id:1})-[:AI1_LINK]->(:Ai1Child {id:2,note:'row-private-value'})",
            "MATCH (n) WHERE n:Ai1Parent OR n:Ai1Child DETACH DELETE n",
            "Ai1Child",
            "AI1_LINK",
            "Labels",
        )
        .await;
    }
    #[tokio::test]
    #[allow(clippy::print_stderr)] // Intentional skip notice for optional live integration tests.
    async fn mongodb_real_context() {
        let Ok(uri) = std::env::var("AXDB_TEST_MONGO") else {
            eprintln!("AXDB_TEST_MONGO unset: live Mongo context not run");
            return;
        };
        assert_eq!(
            uri,
            "mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin"
        );
        let test_database=format!("axdbt_d10_context_{}",uuid::Uuid::new_v4().simple());
        let collection = format!("ai1b_{}", uuid::Uuid::new_v4().simple());
        verify(
            Arc::new(driver_mongodb::MongoDriver::new()),
            ConnectConfig {
                fields: BTreeMap::from([
                    ("uri".into(), "mongodb://127.0.0.1:57017".into()),
                    ("database".into(), test_database.clone()),
                    ("user".into(), "axdb".into()),
                ]),
                secret: Some("testmongo".into()),
                connect_timeout_ms: 3000,
            },
            &json!({"insert": collection, "documents": [{"note":"row-private-value"}]}).to_string(),
            &json!({"drop": collection}).to_string(),
            &collection,
            "collection",
            &test_database,
        )
        .await;
    }
    #[test]
    fn run_context_is_bounded_and_cannot_contain_result_rows() {
        let (host, dir) = host();
        for i in 0..30 {
            remember(&host, "s", &i.to_string(), "select", Ok(&[]));
        }
        assert!(run(&host, "0").is_err());
        assert!(run(&host, "29").is_ok());
        assert_eq!(host.context_runs.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 20);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
