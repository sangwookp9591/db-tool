use crate::{
    agent_bridge::{forward, Shared},
    context,
    permissions::{self, AccessMode, AgentClient, Permission},
    storage, Host,
};
use dbcore::{DbError, ExecRequest, PreviewSupport};
use serde_json::{json, Value};
use tauri::{Emitter, Manager};

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
        "backup_status" => (&["session_id"], &["session_id"]),
        "plan_grants" => (&["session_id","changes"], &["session_id"]),
        "schema_diff" => (&["session_id","after"], &["session_id"]),
        "plan_import" => (&["session_id","filename","content","node_id"], &["session_id","filename","content","node_id"]),
        "plan_migration" => (&["session_id","text"], &["session_id","text"]),
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
        "explain_query" | "lint_query" => (&["session_id", "query"], &["session_id", "query"]),
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
        "get_schema_context" => (&["session_id", "focus"], &["session_id"]),
        "get_work_context" => (&[], &[]),
        "explain_error" => (&["run"], &["run"]),
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
                "changes" => serde_json::from_value::<Vec<dbcore::GrantChange>>(v.clone()).is_ok_and(|c|dbcore::validate_grants(&c).is_ok()),
                "after" => v.is_object(),
                "content" => v.as_str().is_some_and(|s|s.len()<=1024*1024),
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
fn activity_summary(tool: &str, requested_write: bool, result: &Value) -> Value {
    let error = result.get("error").filter(|error| !error.is_null() && result.as_object().is_some_and(|value| value.keys().all(|key| matches!(key.as_str(), "error" | "run"))));
    let denied = error.and_then(|value| value["kind"].as_str()) == Some("Forbidden");
    let rows = result["results"].as_array().map(|results| results.iter().filter_map(|result| result["row_count"].as_u64()).fold(0_u64, u64::saturating_add));
    json!({"status":if error.is_some() {"error"} else {"success"}, "decision":if denied {Some("denied")} else if error.is_some() {None} else if tool == "run_query" && requested_write {Some("approved")} else {Some("allowed")}, "row_count":rows})
}

pub async fn dispatch(shared: &Shared, client: &AgentClient, method: &str, params: Value) -> Value {
    let Some(app) = shared.app.as_ref() else { return dispatch_inner(shared, client, method, params).await; };
    let requested = if method == "tools/list" { "tools/list" } else { params["name"].as_str().unwrap_or("") };
    let tool = if requested == "tools/list" || permissions::TOOLS.contains(&requested) { requested } else { "unknown" }.to_owned();
    let host = app.state::<Host>();
    let connection = connection_id(&host, &tool, &params["arguments"]).filter(|id| profiles(&host).is_ok_and(|profiles| profiles.iter().any(|profile| profile.id == *id)));
    let id = uuid::Uuid::new_v4().to_string();
    let started = std::time::Instant::now();
    let requested_write = params["arguments"]["read_only"] == false;
    let mut event = json!({"id":id,"name":tool,"client_id":client.id,"connection_id":connection,"status":"running"});
    let _ = app.emit("agent-call", &event);
    let result = dispatch_inner(shared, client, method, params).await;
    let summary = activity_summary(&tool, requested_write, &result);
    for key in ["status", "decision", "row_count"] { event[key] = summary[key].clone(); }
    event["duration_ms"] = json!(started.elapsed().as_millis());
    let _ = app.emit("agent-call", &event);
    result
}

async fn dispatch_inner(shared: &Shared, client: &AgentClient, method: &str, params: Value) -> Value {
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
                    && (!matches!(
                        t["name"].as_str(),
                        Some("get_schema_context" | "get_work_context" | "explain_error")
                    ) || profiles(&host).is_ok_and(|profiles| {
                        profiles
                            .iter()
                            .any(|p| client.permission(&p.id) != Permission::None)
                    }))
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
    let context_tool = matches!(
        tool,
        "get_schema_context" | "get_work_context" | "explain_error"
    );
    let work = if context_tool {
        if !client.allowed_tools.iter().any(|t| t == tool) {
            let _ = permissions::audit(&host, client, tool, None, "denied", None);
            return permissions::error("Forbidden");
        }
        if !valid_arguments(tool, args) {
            let _ = permissions::audit(&host, client, tool, None, "denied", None);
            return permissions::error("InvalidInput");
        }
        let input = match tool {
            "get_schema_context" => Some(context::WorkInput {
                session_id: args["session_id"].as_str().unwrap_or("").into(),
                scope: context::Scope {
                    focus: args["focus"].as_str().map(str::to_owned),
                    ..Default::default()
                },
            }),
            "get_work_context" => serde_json::from_value::<context::WorkInput>(
                forward(shared, "context", json!({})).await,
            )
            .ok(),
            "explain_error" => context::run(&host, args["run"].as_str().unwrap_or(""))
                .ok()
                .map(|r| context::WorkInput {
                    session_id: r.session_id,
                    scope: context::Scope {
                        text: r.text,
                        run: Some(r.run),
                        ..Default::default()
                    },
                }),
            _ => None,
        };
        if input.is_none() {
            let _ = permissions::audit(&host, client, tool, None, "denied", None);
            return permissions::error("NotFound");
        }
        input
    } else {
        None
    };
    let id = work
        .as_ref()
        .and_then(|w| connection_id(&host, tool, &json!({"session_id":w.session_id})))
        .or_else(|| connection_id(&host, tool, args));
    let text = args["text"].as_str().or_else(|| args["query"].as_str());
    let requires_connection = matches!(
        tool,
        "open_connection"
            | "close_connection"
            | "explore"
            | "run_query"
            | "preview_changes"
            | "schema_diff" | "plan_import" | "plan_migration" | "backup_status" | "plan_grants"
            | "explain_query"
            | "lint_query"
            | "browse"
            | "expand_node"
            | "open_query_tab"
            | "get_schema_context"
            | "get_work_context"
            | "explain_error"
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
        "schema_diff" => {
            let after=serde_json::from_value::<dbcore::SchemaSnapshot>(args["after"].clone());
            match after {Ok(after)=>match crate::management::schema_snapshot(app.state(),args["session_id"].as_str().unwrap_or("").into(),None).await{Ok(before)=>encoded(crate::management::schema_diff(before,after)),Err(e)=>json!({"error":e})},Err(_)=>permissions::error("InvalidInput")}
        },
        "plan_import" => {
            match crate::management::import_plan(args["filename"].as_str().unwrap_or("").into(),args["content"].as_str().unwrap_or("").as_bytes().to_vec(),args["node_id"].as_str().unwrap_or("").into(),None,None) {
                Ok(plan)=>{
                    if plan.errors.is_empty(){match crate::management::import_preview(app.clone(),app.state(),args["session_id"].as_str().unwrap_or("").into(),uuid::Uuid::new_v4().to_string(),plan.request.clone()).await{Ok(preview)=>json!({"plan":plan,"preview":preview,"needs_approval":true}),Err(e)=>json!({"plan":plan,"error":e})}}else{json!({"plan":plan,"needs_approval":true})}
                },Err(e)=>json!({"error":e})
            }
        },
        "backup_status" => encoded(crate::management::backup_status(app.state(),args["session_id"].as_str().unwrap_or("").into())),
        "plan_grants" => match serde_json::from_value::<Vec<dbcore::GrantChange>>(args["changes"].clone()){Ok(changes)=>encoded(crate::management::plan_grants(app.state(),args["session_id"].as_str().unwrap_or("").into(),changes).await),Err(_)=>permissions::error("InvalidInput")},
        "plan_migration" => encoded(crate::management::migration_preview(app.state(),args["session_id"].as_str().unwrap_or("").into(),uuid::Uuid::new_v4().to_string(),args["text"].as_str().unwrap_or("").into()).await),
        "explain_query" => encoded(crate::editor::explain_query(app.state(), args["session_id"].as_str().unwrap_or("").into(), uuid::Uuid::new_v4().to_string(), args["query"].as_str().unwrap_or("").into(), false).await),
        "lint_query" => encoded(crate::editor::lint_query(app.state(), args["session_id"].as_str().unwrap_or("").into(), args["query"].as_str().unwrap_or("").into()).await),
        "get_schema_context" | "get_work_context" | "explain_error" => {
            let mut work = work.expect("context input checked");
            work.scope.include_rows = false;
            work.scope.selected_rows = None;
            encoded(
                context::build(&host, &work.session_id, work.scope)
                    .await
                    .map(|mut value| {
                        value["permissions"]["client_grant"] =
                            json!(client.permission(id.as_deref().unwrap_or("")));
                        value["permissions"]["write"] = json!(
                            value["access_mode"] != "read_only"
                                && client.permission(id.as_deref().unwrap_or(""))
                                    == Permission::WriteWithApproval
                        );
                        value
                    }),
            )
        }
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
        (!context_tool || now.allowed_tools.iter().any(|name| name == tool))
            && (!requires_connection
                || id
                    .as_ref()
                    .is_some_and(|id| now.permission(id) != Permission::None))
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
    let run_id = uuid::Uuid::new_v4().to_string();
    let result = crate::execute_impl(
        host,
        session.into(),
        run_id.clone(),
        req,
        None,
        storage::HistorySource::Agent,
    )
    .await;
    if let Ok(results) = &result {
        let _ = forward(shared, "results", json!({"results":results})).await;
    }
    let mut response = encoded(result.map(|results| json!({"results":results})));
    response["run"] = json!(run_id);
    response
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
    fn activity_is_metadata_only_and_never_assumes_approval_on_failure() {
        let result = json!({"results":[{"row_count":3,"table":{"rows":[["private-password"]]}}],"arguments":{"token":"private-token"}});
        let summary = activity_summary("run_query", true, &result);
        assert_eq!(summary["row_count"], 3);
        assert_eq!(activity_summary("get_work_context", false, &json!({"schema":"users","error":{"message":"private-token"}}))["status"], "success");
        assert_eq!(activity_summary("get_work_context", false, &json!({"error":null}))["status"], "success");
        assert_eq!(summary["decision"], "approved");
        assert!(!summary.to_string().contains("private-"));
        let failed = activity_summary("run_query", true, &json!({"error":{"kind":"UserDenied","message":"private-token"}}));
        assert_eq!(failed["status"], "error");
        assert!(failed["decision"].is_null());
        assert!(!failed.to_string().contains("private-"));
        assert_eq!(activity_summary("run_query", false, &json!({"error":{"kind":"Forbidden"}}))["decision"], "denied");
    }

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
    fn context_arguments_cannot_enable_row_export() {
        assert!(valid_arguments(
            "get_schema_context",
            &json!({"session_id":"s","focus":"users"})
        ));
        assert!(valid_arguments("get_work_context", &json!({})));
        assert!(valid_arguments("explain_error", &json!({"run":"r"})));
        assert!(!valid_arguments(
            "get_schema_context",
            &json!({"session_id":"s","include_rows":true})
        ));
        assert!(!valid_arguments(
            "get_work_context",
            &json!({"session_id":"s"})
        ));
        assert!(!valid_arguments("explain_error", &json!({"run":1})));
    }
    #[test]
    fn validates_direct_host_tool_arguments() {
        assert!(valid_arguments("explain_query", &json!({"session_id":"s","query":"SELECT 1"})));
        assert!(!valid_arguments("explain_query", &json!({"session_id":"s","query":"UPDATE people SET id=2","analyze":true})));
        assert!(!valid_arguments("lint_query", &json!({"session_id":"s","query":""})));
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
