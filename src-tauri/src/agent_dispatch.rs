use crate::{
    agent_bridge::{forward, Shared},
    permissions::{self, AccessMode, AgentClient, Permission},
    storage, Host,
};
use dbcore::{DbError, ExecRequest, PreviewSupport};
use serde_json::{json, Value};
use tauri::Manager;

fn encoded<T: serde::Serialize>(value: Result<T, DbError>) -> Value {
    match value {
        Ok(v) => serde_json::to_value(v).unwrap_or_else(|_| permissions::error("Internal")),
        Err(e) => json!({"error":e}),
    }
}
fn profiles(host: &Host) -> Result<Vec<storage::Profile>, DbError> {
    storage::read_profiles(&host.paths.config.join("profiles.json"))
}
fn connection_id(host: &Host, tool: &str, args: &Value) -> Option<String> {
    if tool == "open_connection" {
        return args["profile_id"].as_str().map(str::to_owned);
    }
    args["session_id"].as_str().and_then(|id| {
        host.sessions
            .lock()
            .ok()?
            .get(id)
            .map(|r| r.profile.id.clone())
    })
}
fn mode(host: &Host, id: Option<&str>) -> Result<AccessMode, DbError> {
    Ok(profiles(host)?
        .into_iter()
        .find(|p| Some(p.id.as_str()) == id)
        .map(|p| p.access_mode)
        .unwrap_or_default())
}
fn driver_read(host: &Host, id: Option<&str>) -> bool {
    profiles(host)
        .ok()
        .and_then(|p| p.into_iter().find(|p| Some(p.id.as_str()) == id))
        .and_then(|p| host.driver(&p.driver_id).ok())
        .is_some_and(|d| d.manifest().capabilities.read_only_exec)
}
fn valid_arguments(tool: &str, args: &Value) -> bool {
    let Some(args) = args.as_object() else {
        return false;
    };
    let (keys, required): (&[&str], &[&str]) = match tool {
        "run_query" => (
            &[
                "session_id",
                "text",
                "params",
                "row_limit",
                "timeout_ms",
                "read_only",
            ],
            &["session_id", "text"],
        ),
        "preview_changes" => (
            &["session_id", "text", "params", "timeout_ms", "sample_limit"],
            &["session_id", "text"],
        ),
        "browse" => (
            &["session_id", "node_id", "offset", "limit"],
            &["session_id", "node_id"],
        ),
        "expand_node" => (
            &["session_id", "element_id", "limit"],
            &["session_id", "element_id"],
        ),
        "get_history" => (&["limit"], &[]),
        _ => return true,
    };
    if required.iter().any(|key| {
        args.get(*key)
            .is_none_or(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
    }) {
        return false;
    }
    args.iter().all(|(key, v)| {
        keys.contains(&key.as_str())
            && match key.as_str() {
                "row_limit" | "limit" => v.as_u64().is_some_and(|n| {
                    (1..=if tool == "expand_node" { 200 } else { 1000 }).contains(&n)
                }),
                "timeout_ms" => v.as_u64().is_some_and(|n| (1..=3600000).contains(&n)),
                "sample_limit" => v.as_u64().is_some_and(|n| n <= 200),
                "offset" => v.as_u64().is_some_and(|n| n <= 9007199254740991),
                "read_only" => v.is_boolean(),
                "params" => v.is_object(),
                _ => v.is_string(),
            }
    })
}
fn visible_connections(host: &Host, client: &AgentClient) -> Value {
    let list = match profiles(host) {
        Ok(p) => p,
        Err(e) => return json!({"error":e}),
    };
    let sessions = match host.sessions.lock() {
        Ok(s) => s,
        Err(_) => return permissions::error("Internal"),
    };
    json!(list.into_iter().filter(|p|client.permission(&p.id)!=Permission::None).map(|p|json!({"id":p.id,"name":p.name,"driver_id":p.driver_id,"access_mode":p.access_mode,"session_id":sessions.iter().find(|(_,r)|r.profile.id==p.id&&!r.dead.load(std::sync::atomic::Ordering::Acquire)).map(|(id,_)|id)})).collect::<Vec<_>>())
}
pub async fn dispatch(shared: &Shared, client: &AgentClient, method: &str, params: Value) -> Value {
    let Some(app) = shared.app.as_ref() else {
        return forward(shared, method, params).await;
    };
    if !shared.prepared(std::time::Duration::from_secs(10)).await {
        return json!({"error":{"kind":"Unavailable","message":"App webview is not ready"}});
    }
    let host = app.state::<Host>();
    let client = match permissions::current(&host.paths.config, client) {
        Ok(client) => client,
        Err(_) => return permissions::error("Forbidden"),
    };
    let client = &client;
    if method == "tools/list" {
        let mut value = forward(shared, method, params).await;
        if let Some(tools) = value["tools"].as_array_mut() {
            tools.retain(|t| {
                client
                    .allowed_tools
                    .iter()
                    .any(|n| Some(n.as_str()) == t["name"].as_str())
                    && (t["name"] != "preview_changes"
                        || profiles(&host).is_ok_and(|profiles| {
                            profiles.iter().any(|p| {
                                p.access_mode != AccessMode::ReadOnly
                                    && matches!(
                                        client.permission(&p.id),
                                        Permission::Preview | Permission::WriteWithApproval
                                    )
                            })
                        }))
            });
        }
        if let Err(e) = permissions::audit(&host, client, "tools/list", None, "allowed", None) {
            return json!({"error":e});
        }
        return value;
    }
    let tool = params["name"].as_str().unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let args = &arguments;
    let id = connection_id(&host, tool, args);
    let text = args["text"].as_str();
    let requires_connection = matches!(
        tool,
        "open_connection"
            | "close_connection"
            | "explore"
            | "run_query"
            | "preview_changes"
            | "browse"
            | "expand_node"
            | "open_query_tab"
    );
    let decision = if requires_connection && id.is_none() {
        Err("NotFound")
    } else {
        match mode(&host, id.as_deref()) {
            Ok(mode) => permissions::check(
                client,
                id.as_deref(),
                tool,
                mode,
                args["read_only"] != false,
                driver_read(&host, id.as_deref()),
            ),
            Err(_) => Err("Forbidden"),
        }
    };
    let forced = match decision {
        Ok(read) => read,
        Err(kind) => {
            let _ = permissions::audit(&host, client, tool, id.as_deref(), "denied", text);
            return permissions::error(kind);
        }
    };
    if !valid_arguments(tool, args) {
        let _ = permissions::audit(&host, client, tool, id.as_deref(), "denied", text);
        return permissions::error("InvalidInput");
    }
    if let Err(e) = permissions::audit(&host, client, tool, id.as_deref(), "allowed", text) {
        return json!({"error":e});
    }
    let mut result = match tool {
        "list_connections" => visible_connections(&host, client),
        "get_history" => {
            let limit = args["limit"].as_u64().unwrap_or(100).min(1000) as usize;
            encoded(crate::history_list(app.state(), 1000).await.map(|entries| {
                entries
                    .into_iter()
                    .filter(|h| {
                        (client.id == "local" && client.connections.is_empty())
                            || h.profile_id
                                .as_ref()
                                .is_some_and(|id| client.permission(id) != Permission::None)
                    })
                    .take(limit)
                    .collect::<Vec<_>>()
            }))
        }
        "get_ui_state" if client.id != "local" || !client.connections.is_empty() => {
            json!({"connections":visible_connections(&host,client),"active_tab":null,"last_result":[]})
        }
        "run_query" | "preview_changes" => {
            let session = args["session_id"].as_str().unwrap_or("").to_owned();
            let req = serde_json::from_value::<ExecRequest>(
                json!({"text":args["text"],"params":args.get("params").cloned().unwrap_or(json!({})),"row_limit":args["row_limit"].as_u64().unwrap_or(1000).min(u64::from(client.max_rows)),"graph_node_limit":client.max_rows.min(200),"timeout_ms":args["timeout_ms"].as_u64().unwrap_or(30000).clamp(1,3600000),"read_only":forced,"preview_sample_limit":args["sample_limit"].as_u64().unwrap_or(50).min(u64::from(client.max_rows))}),
            );
            match req {
                Err(_) => permissions::error("InvalidInput"),
                Ok(mut req) => {
                    req.read_only = forced && tool == "run_query";
                    if tool == "preview_changes" {
                        encoded(
                            crate::preview(
                                app.state(),
                                session,
                                uuid::Uuid::new_v4().to_string(),
                                req,
                                Some(storage::HistorySource::Agent),
                            )
                            .await,
                        )
                    } else {
                        run_query(
                            shared,
                            &host,
                            client,
                            &session,
                            id.as_deref().unwrap_or(""),
                            req,
                        )
                        .await
                    }
                }
            }
        }
        "browse" => encoded(
            crate::browse(
                app.state(),
                args["session_id"].as_str().unwrap_or("").into(),
                args["node_id"].as_str().unwrap_or("").into(),
                args["offset"].as_u64().unwrap_or(0),
                args["limit"]
                    .as_u64()
                    .unwrap_or(100)
                    .min(u64::from(client.max_rows)) as u32,
                uuid::Uuid::new_v4().to_string(),
                None,
            )
            .await
            .map(|r| json!({"results":[r]})),
        ),
        "expand_node" => encoded(
            crate::expand(
                app.state(),
                args["session_id"].as_str().unwrap_or("").into(),
                args["element_id"].as_str().unwrap_or("").into(),
                args["limit"]
                    .as_u64()
                    .unwrap_or(200)
                    .min(u64::from(client.max_rows)) as u32,
            )
            .await
            .map(|r| json!({"results":[r]})),
        ),
        _ => forward(shared, method, params.clone()).await,
    };
    // A delayed credential dialog or UI response cannot return data after a grant is withdrawn.
    let valid = permissions::current(&host.paths.config, client).is_ok_and(|now| {
        !requires_connection
            || id
                .as_ref()
                .is_some_and(|id| now.permission(id) != Permission::None)
    });
    if !valid {
        let _ = permissions::audit(&host, client, tool, id.as_deref(), "denied", text);
        return permissions::error("Forbidden");
    }
    bound(&mut result, client.max_rows.min(200) as usize);
    result
}
async fn run_query(
    shared: &Shared,
    host: &Host,
    client: &AgentClient,
    session: &str,
    profile_id: &str,
    req: ExecRequest,
) -> Value {
    if !req.read_only {
        let Some(app) = shared.app.as_ref() else {
            return permissions::error("Forbidden");
        };
        let record = match host.session(session) {
            Ok(r) => r,
            Err(e) => return json!({"error":e}),
        };
        let can_preview = host
            .driver(&record.profile.driver_id)
            .is_ok_and(|d| d.manifest().capabilities.preview_changes != PreviewSupport::None);
        if !can_preview {
            return permissions::error("Forbidden");
        }
        let preview = match crate::preview(
            app.state(),
            session.into(),
            uuid::Uuid::new_v4().to_string(),
            req.clone(),
            Some(storage::HistorySource::Agent),
        )
        .await
        {
            Ok(p) => p,
            Err(e) => return json!({"error":e}),
        };
        let mut display = json!(preview);
        bound(&mut display, client.max_rows.min(200) as usize);
        let reply=forward(shared,"approval",json!({"arguments":{"timeout_ms":req.timeout_ms},"text":req.text,"connection":record.profile.name,"preview":display,"client":client.name})).await;
        if reply["approved"] != true {
            let _ = permissions::audit(
                host,
                client,
                "run_query",
                Some(profile_id),
                "rejected",
                Some(&req.text),
            );
            return json!({"error":{"kind":"UserDenied","message":"Agent request denied or approval expired"}});
        }
        let valid = permissions::current(&host.paths.config, client).is_ok_and(|current| {
            permissions::check(
                &current,
                Some(profile_id),
                "run_query",
                host.access_mode(&record).unwrap_or(AccessMode::ReadOnly),
                false,
                true,
            ) == Ok(false)
        });
        if !valid {
            let _ = permissions::audit(
                host,
                client,
                "run_query",
                Some(profile_id),
                "denied",
                Some(&req.text),
            );
            return permissions::error("Forbidden");
        }
        if let Err(e) = permissions::audit(
            host,
            client,
            "run_query",
            Some(profile_id),
            "approved",
            Some(&req.text),
        ) {
            return json!({"error":e});
        }
    }
    let result = crate::execute_impl(
        host,
        session.into(),
        uuid::Uuid::new_v4().to_string(),
        req,
        None,
        storage::HistorySource::Agent,
    )
    .await;
    if let Ok(results) = &result {
        let _ = forward(shared, "results", json!({"results":results})).await;
    }
    encoded(result.map(|results| json!({"results":results})))
}
// Count limits are aggregate across result sets; typed values remain unchanged.
pub fn bound(value: &mut Value, limit: usize) {
    if let Some(results) = value.get_mut("results").and_then(Value::as_array_mut) {
        let omitted = results.len() > 200;
        results.truncate(200);
        if omitted {
            for result in results {
                result["truncated"] = json!(true);
            }
        }
    }
    fn walk(value: &mut Value, rows: &mut usize, nodes: &mut usize, docs: &mut usize) {
        match value {
            Value::Object(object) => {
                let mut truncated = false;
                for key in ["table", "before", "after"] {
                    if let Some(table) = object.get_mut(key) {
                        if let Some(items) = table.get_mut("rows").and_then(Value::as_array_mut) {
                            let before = items.len();
                            items.truncate(*rows);
                            *rows = rows.saturating_sub(items.len());
                            truncated |= before > items.len();
                        }
                    }
                }
                if let Some(items) = object.get_mut("documents").and_then(Value::as_array_mut) {
                    let before = items.len();
                    items.truncate(*docs);
                    *docs = docs.saturating_sub(items.len());
                    truncated |= before > items.len();
                }
                if let Some(graph) = object.get_mut("graph") {
                    if let Some(items) = graph.get_mut("nodes").and_then(Value::as_array_mut) {
                        let before = items.len();
                        items.truncate(*nodes);
                        *nodes = nodes.saturating_sub(items.len());
                        truncated |= before > items.len();
                    }
                    let ids = graph["nodes"]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|n| n["id"].as_str().map(str::to_owned))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if let Some(rels) = graph.get_mut("rels").and_then(Value::as_array_mut) {
                        let before = rels.len();
                        rels.retain(|r| {
                            r["start"]
                                .as_str()
                                .is_some_and(|s| ids.iter().any(|id| id == s))
                                && r["end"]
                                    .as_str()
                                    .is_some_and(|s| ids.iter().any(|id| id == s))
                        });
                        rels.truncate(200);
                        truncated |= before > rels.len();
                    }
                }
                if truncated {
                    object.insert("truncated".into(), json!(true));
                }
                for (key, child) in object.iter_mut() {
                    if !matches!(
                        key.as_str(),
                        "table" | "before" | "after" | "graph" | "documents"
                    ) {
                        walk(child, rows, nodes, docs);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, rows, nodes, docs);
                }
            }
            _ => {}
        }
    }
    let (mut rows, mut nodes, mut docs) = (limit, limit, limit);
    walk(value, &mut rows, &mut nodes, &mut docs);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caps_rows_documents_preview_samples_and_graph_without_dangling_edges() {
        let mut value = json!({"results":[{"table":{"rows":[[1],[2],[3]]},"documents":[1,2,3],"graph":{"nodes":[{"id":"a"},{"id":"b"},{"id":"c"}],"rels":[{"start":"a","end":"b"},{"start":"a","end":"c"}]}},{"table":{"rows":[[4]]},"documents":[4]}],"statements":[{"before":{"rows":[[1],[2]]},"after":{"rows":[[3]]}}]});
        bound(&mut value, 2);
        assert_eq!(
            value["results"][0]["table"]["rows"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(value["results"][1]["table"]["rows"], json!([]));
        assert_eq!(value["results"][0]["documents"], json!([1, 2]));
        assert_eq!(
            value["results"][0]["graph"]["rels"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(value["statements"][0]["after"]["rows"], json!([]));
        assert_eq!(value["results"][0]["truncated"], true);
    }
    #[test]
    fn validates_direct_host_tool_arguments() {
        let mut many = json!({"results":vec![json!({"message":"done"});201]});
        bound(&mut many, 3);
        assert_eq!(many["results"].as_array().unwrap().len(), 200);
        assert_eq!(many["results"][0]["truncated"], true);
        for args in [
            json!({"session_id":"s","text":""}),
            json!({"session_id":"s","text":"SELECT 1","read_only":"false"}),
            json!({"session_id":"s","text":"SELECT 1","row_limit":0}),
            json!({"session_id":"s","text":"SELECT 1","params":[]}),
            json!({"session_id":"s","text":"SELECT 1","unknown":1}),
        ] {
            assert!(!valid_arguments("run_query", &args));
        }
        assert!(valid_arguments(
            "run_query",
            &json!({"session_id":"s","text":"SELECT 1","params":{"x":null},"read_only":true,"row_limit":3})
        ));
    }
}
