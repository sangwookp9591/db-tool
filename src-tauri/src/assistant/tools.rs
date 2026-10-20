use crate::{permissions::{self, AccessMode, AgentClient, Permission}, Host};
use dbcore::{DbError, ExecRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlparser::{ast::{Statement, Visit, Visitor}, dialect::GenericDialect, parser::Parser};
use std::{collections::BTreeMap, ops::ControlFlow};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRequest {
    pub name: String,
    pub arguments: Value,
    pub client_id: Option<String>,
}
#[derive(Default)]
struct Risk { read: bool, destructive: bool }
impl Visitor for Risk {
    type Break = ();
    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        match statement {
            Statement::Query(_) => (),
            Statement::Drop { .. } | Statement::Truncate { .. } => { self.read = false; self.destructive = true; },
            Statement::Delete(delete) => { self.read = false; self.destructive |= delete.selection.is_none(); },
            Statement::Update(sqlparser::ast::Update { selection, .. }) => { self.read = false; self.destructive |= selection.is_none(); },
            _ => self.read = false,
        }
        ControlFlow::Continue(())
    }
}
fn risk(text: &str) -> Risk {
    let Ok(statements) = Parser::parse_sql(&GenericDialect {}, text) else { return Risk { read: false, destructive: true }; };
    let mut risk = Risk { read: !statements.is_empty(), destructive: false };
    let _ = statements.visit(&mut risk);
    // SELECT INTO writes even though sqlparser represents it as a Query.
    if dbcore::preview::tokens(text).is_ok_and(|tokens| tokens.iter().any(|t| t.is("INTO"))) { risk.read = false; }
    risk
}
#[derive(Serialize)]
pub struct Policy {
    pub allowed: bool,
    pub reason: Option<String>,
    pub read_only: bool,
    pub destructive: bool,
    pub can_apply: bool,
    pub connection: String,
    pub mode: &'static str,
    pub client: String,
}
fn client(host: &Host, request: &ToolRequest, profile: &str) -> Result<AgentClient, DbError> {
    if let Some(id) = &request.client_id {
        return permissions::read(&host.paths.config)?.into_iter().find(|c| &c.id == id && c.active())
            .ok_or_else(|| DbError::query("에이전트가 만료되었거나 폐기되었습니다"));
    }
    Ok(AgentClient {
        id: "assistant".into(), name: "앱 AI 도우미".into(), token_hash: String::new(), token_salt: String::new(),
        connections: BTreeMap::from([(profile.into(), Permission::WriteWithApproval)]),
        allowed_tools: vec!["run_query".into(), "preview_changes".into()], max_rows: 1000,
        max_result_rows: 200, max_result_bytes: 256*1024, expires_at: None, revoked: false,
    })
}
fn policy(host: &Host, session: &str, request: &ToolRequest, read_only: bool) -> Result<(Policy, AgentClient), DbError> {
    let record = host.session(session)?;
    let client = client(host, request, &record.profile.id)?;
    let mode = host.access_mode(&record)?;
    let driver = host.driver(&record.profile.driver_id)?;
    let query = matches!(request.name.as_str(), "run_query" | "preview_changes");
    let sql = request.arguments["text"].as_str().unwrap_or("");
    let mut risk = if query { risk(sql) } else { Risk { read: !matches!(request.name.as_str(), "plan_import" | "plan_migration" | "plan_grants" | "open_connection" | "close_connection" | "open_query_tab"), destructive: false } };
    if request.name == "preview_changes" { risk.read = false; }
    let args_valid = crate::agent_dispatch::valid_arguments(&request.name, &request.arguments)
        && request.arguments.get("session_id").is_none_or(|v| v == session)
        && request.arguments.get("profile_id").is_none_or(|v| v == record.profile.id.as_str())
        && permissions::TOOLS.contains(&request.name.as_str())
        && sql.len() <= 256*1024;
    let requested_read_only = request.name == "run_query" && (request.arguments["read_only"] == true || request.client_id.is_some() && request.arguments["read_only"] != false);
    let checked = permissions::check(&client, Some(&record.profile.id), &request.name, mode, risk.read, driver.manifest().capabilities.read_only_exec);
    let reason = if !args_valid { Some("도구 인자 또는 대상 연결이 올바르지 않습니다") }
        else if !risk.read && (read_only || requested_read_only || mode == AccessMode::ReadOnly) { Some("읽기 전용 연결 또는 권한 모드에서는 쓰기를 허용할 수 없습니다") }
        else if checked.is_err() { Some("이 에이전트의 연결·도구 권한이 허용하지 않습니다") }
        else if !risk.read && query && driver.manifest().capabilities.preview_changes == dbcore::PreviewSupport::None { Some("이 드라이버는 안전한 변경 미리보기를 지원하지 않습니다") }
        else { None };
    let can_apply = reason.is_none() && !read_only && !requested_read_only && matches!(permissions::check(&client, Some(&record.profile.id), "run_query", mode, false, driver.manifest().capabilities.read_only_exec), Ok(false));
    let permission = client.permission(&record.profile.id);
    Ok((Policy { allowed: reason.is_none(), reason: reason.map(str::to_owned), read_only: risk.read,
        destructive: risk.destructive, can_apply, connection: record.profile.name.clone(), mode: if read_only || requested_read_only || mode == AccessMode::ReadOnly || permission == Permission::Read { "read_only" } else if permission == Permission::Preview { "preview" } else if permission == Permission::None { "none" } else { "write_with_approval" }, client: client.name.clone() }, client))
}
fn query_request(client: &AgentClient, args: &Value, read_only: bool) -> Result<ExecRequest, DbError> {
    serde_json::from_value(json!({"text":args["text"],"params":args.get("params").cloned().unwrap_or(json!({})),"row_limit":args["row_limit"].as_u64().unwrap_or(1000).min(u64::from(client.max_rows)),"graph_node_limit":client.max_rows.min(200),"timeout_ms":args["timeout_ms"].as_u64().unwrap_or(30000).clamp(1,3600000),"read_only":read_only,"preview_sample_limit":args["sample_limit"].as_u64().unwrap_or(50).min(u64::from(client.max_rows))}))
        .map_err(|_| DbError::query("도구 인자가 올바르지 않습니다"))
}
#[tauri::command]
pub fn ai_tool_policy(host: tauri::State<'_, Host>, session_id: String, request: ToolRequest, read_only: bool) -> Result<Policy, DbError> {
    policy(&host, &session_id, &request, read_only).map(|(p, _)| p)
}
#[tauri::command]
pub async fn ai_tool_decide(host: tauri::State<'_, Host>, bridge: tauri::State<'_, crate::agent_bridge::Bridge>, session_id: String, request: ToolRequest, read_only: bool, decision: String) -> Result<Value, DbError> {
    let (policy, client) = policy(&host, &session_id, &request, read_only)?;
    let record = host.session(&session_id)?;
    let text = request.arguments["text"].as_str();
    if decision == "deny" {
        permissions::audit(&host, &client, &request.name, Some(&record.profile.id), "denied", text)?;
        return Ok(json!({"denied":true}));
    }
    if !matches!(decision.as_str(), "allow" | "apply") || !policy.allowed {
        permissions::audit(&host, &client, &request.name, Some(&record.profile.id), "denied", text)?;
        return Err(DbError::query(policy.reason.unwrap_or_else(|| "승인 결정이 올바르지 않습니다".into())));
    }
    permissions::audit(&host, &client, &request.name, Some(&record.profile.id), if decision == "apply" {"approved"} else {"allowed"}, text)?;
    let started = std::time::Instant::now();
    let result = async {
        if matches!(request.name.as_str(), "run_query" | "preview_changes") {
            let args = &request.arguments;
            let req = query_request(&client, args, policy.read_only)?;
            let run = uuid::Uuid::new_v4().to_string();
            if !policy.read_only && decision == "allow" {
                permissions::check(&client, Some(&record.profile.id), "preview_changes", host.access_mode(&record)?, false, true).map_err(|_| DbError::query("미리보기 권한이 없습니다"))?;
                let preview = crate::preview(host.clone(), session_id, run, req, Some(crate::storage::HistorySource::Agent)).await?;
                return Ok(json!({"preview":preview}));
            }
            if decision == "apply" {
                if policy.read_only { return Err(DbError::query("읽기 도구에는 적용 단계가 없습니다")); }
                permissions::check(&client, Some(&record.profile.id), "run_query", host.access_mode(&record)?, false, true).map_err(|_| DbError::query("쓰기 실행 권한이 없습니다"))?;
                crate::permissions::agent_approve_write(host.clone(), session_id.clone(), run.clone(), req.clone())?;
            }
            let results = crate::execute_agent(host.clone(), session_id, run, req).await?;
            Ok(json!({"row_count":results.iter().map(|r|r.row_count).sum::<u64>()}))
        } else {
            if decision == "apply" { return Err(DbError::query("이 도구에는 적용 단계가 없습니다")); }
            if request.client_id.is_none() { return Err(DbError::query("MCP 도구에는 저장된 에이전트 클라이언트가 필요합니다")); }
            let value = bridge.call_tool(&client, &request.name, request.arguments.clone()).await?;
            if value.get("error").is_some() { return Err(DbError::query("MCP 도구 호출에 실패했습니다. 에이전트 권한과 인자를 확인하세요")); }
            let mut summary = json!({});
            if let Some(results) = value["results"].as_array() {
                summary["row_count"] = json!(results.iter().filter_map(|r| r["row_count"].as_u64()).sum::<u64>());
            }
            Ok(summary)
        }
    }.await;
    permissions::audit(&host, &client, &request.name, Some(&record.profile.id), if result.is_ok() {"completed"} else {"failed"}, text)?;
    result.map(|mut result: Value| {result["elapsed_ms"] = json!(started.elapsed().as_millis()); result})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_limits_preserve_agent_caps_and_explicit_no_samples() {
        let client:AgentClient=serde_json::from_value(json!({"id":"fixture","name":"fixture","token_hash":"","token_salt":"","connections":{},"allowed_tools":[],"max_rows":3,"expires_at":null})).unwrap();
        let req=query_request(&client,&json!({"text":"SELECT 1","row_limit":1000,"sample_limit":200}),true).unwrap();assert_eq!(req.row_limit,3);assert_eq!(req.graph_node_limit,3);assert_eq!(req.preview_sample_limit,3);assert!(req.read_only);
        assert_eq!(query_request(&client,&json!({"text":"SELECT 1","sample_limit":0}),true).unwrap().preview_sample_limit,0);
    }
    #[test]
    fn sql_risk_uses_syntax_including_nested_and_multi_statements() {
        for sql in ["DROP TABLE t", "TRUNCATE t", "DELETE FROM t", "UPDATE t SET x='WHERE'", "WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x", "SELECT 1; DELETE FROM t", "invalid sql"] {
            let risk=risk(sql); assert!(!risk.read && risk.destructive, "{sql}");
        }
        for sql in ["DELETE FROM t WHERE id=1", "UPDATE t SET x=2 WHERE id=1"] { let r=risk(sql); assert!(!r.read && !r.destructive); }
        for sql in ["SELECT 'DROP; DELETE WHERE'", "/* DROP TABLE t */ SELECT 1", "WITH x AS (SELECT 1) SELECT * FROM x"] { let r=risk(sql); assert!(r.read && !r.destructive); }
        assert!(!risk("SELECT 1 INTO t").read);
    }
}
