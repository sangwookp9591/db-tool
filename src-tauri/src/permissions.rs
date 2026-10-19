use crate::{storage, Host};
use dbcore::{DbError, ExecRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, path::Path, time::Instant};

pub const TOOLS: &[&str] = &[
    "backup_status", "plan_grants",
    "schema_diff", "plan_import", "plan_migration",
    "list_drivers",
    "list_connections",
    "open_connection",
    "close_connection",
    "explore",
    "run_query",
    "preview_changes",
    "explain_query",
    "lint_query",
    "browse",
    "expand_node",
    "get_history",
    "get_perf_metrics",
    "get_ui_state",
    "get_schema_context",
    "get_work_context",
    "explain_error",
    "open_query_tab",
];
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    ReadOnly,
    #[default]
    ReadWrite,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    #[default]
    None,
    Read,
    Preview,
    WriteWithApproval,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentClient {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub token_salt: String,
    pub connections: BTreeMap<String, Permission>,
    pub allowed_tools: Vec<String>,
    pub max_rows: u32,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub revoked: bool,
}
impl AgentClient {
    pub fn permission(&self, id: &str) -> Permission {
        self.connections
            .get(id)
            .copied()
            .unwrap_or(if self.id == "local" {
                Permission::WriteWithApproval
            } else {
                Permission::None
            })
    }
    pub fn active(&self) -> bool {
        !self.revoked
            && self.expires_at.as_ref().is_none_or(|s| {
                time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                    .is_ok_and(|at| at > time::OffsetDateTime::now_utc())
            })
    }
    pub fn view(&self) -> Value {
        json!({"id":self.id,"name":self.name,"connections":self.connections,"allowed_tools":self.allowed_tools,"max_rows":self.max_rows,"expires_at":self.expires_at,"revoked":self.revoked})
    }
}
pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn set_token(client: &mut AgentClient, token: &str) {
    client.token_salt = uuid::Uuid::new_v4().to_string();
    client.token_hash = hash(&format!("{}:{token}", client.token_salt));
}
pub fn read(config: &Path) -> Result<Vec<AgentClient>, DbError> {
    match std::fs::read(config.join("agent_clients.json")) {
        Ok(data) => serde_json::from_slice(&data)
            .map_err(|_| DbError::internal("Invalid agent clients file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err(DbError::internal("Cannot read agent clients")),
    }
}
fn save(config: &Path, clients: &[AgentClient]) -> Result<(), DbError> {
    storage::write_private(
        &config.join("agent_clients.json"),
        &serde_json::to_vec_pretty(clients)
            .map_err(|_| DbError::internal("Cannot encode agent clients"))?,
    )
}
pub fn migrate_local(config: &Path, raw: &str) -> Result<(), DbError> {
    let mut clients = read(config)?;
    let mut local = clients
        .iter()
        .find(|c| c.id == "local")
        .cloned()
        .unwrap_or(AgentClient {
            id: "local".into(),
            name: "기본(로컬) 클라이언트".into(),
            token_hash: String::new(),
            token_salt: String::new(),
            connections: BTreeMap::new(),
            allowed_tools: TOOLS.iter().map(|s| s.to_string()).collect(),
            max_rows: 1000,
            expires_at: None,
            revoked: false,
        });
    set_token(&mut local, raw);
    clients.retain(|c| c.id != "local");
    clients.insert(0, local);
    save(config, &clients)
}
pub fn authenticate(config: &Path, raw: &str) -> Result<Option<AgentClient>, DbError> {
    Ok(read(config)?.into_iter().find(|c| {
        let actual = hash(&format!("{}:{raw}", c.token_salt));
        c.active()
            && actual.len() == c.token_hash.len()
            && actual
                .bytes()
                .zip(c.token_hash.bytes())
                .fold(0u8, |diff, (a, b)| diff | (a ^ b))
                == 0
    }))
}
pub fn current(config: &Path, client: &AgentClient) -> Result<AgentClient, DbError> {
    read(config)?
        .into_iter()
        .find(|c| c.id == client.id && c.token_hash == client.token_hash && c.active())
        .ok_or_else(|| {
            DbError::new(
                dbcore::ErrorKind::Auth,
                "에이전트 클라이언트가 만료되었거나 폐기되었습니다",
            )
        })
}
pub fn error(kind: &str) -> Value {
    json!({"error":{"kind":kind,"message":match kind {"NotFound"=>"Connection not found",_=>"Agent permission denied"}}})
}
pub fn check(
    client: &AgentClient,
    profile: Option<&str>,
    tool: &str,
    mode: AccessMode,
    read_only: bool,
    supports_read: bool,
) -> Result<bool, &'static str> {
    let permission = profile.map(|p| client.permission(p));
    if permission == Some(Permission::None) {
        return Err("NotFound");
    }
    if !client.allowed_tools.iter().any(|t| t == tool) {
        return Err("Forbidden");
    }
    if matches!(tool, "preview_changes" | "plan_import" | "plan_migration" | "plan_grants")
        && (mode == AccessMode::ReadOnly
            || !matches!(
                permission,
                Some(Permission::Preview | Permission::WriteWithApproval)
            ))
    {
        return Err("Forbidden");
    }
    if tool == "run_query" {
        let forced = mode == AccessMode::ReadOnly || read_only;
        if !forced && permission != Some(Permission::WriteWithApproval) {
            return Err("Forbidden");
        }
        if forced && !supports_read {
            return Err("Forbidden");
        }
        return Ok(forced);
    }
    Ok(true)
}
pub fn audit(
    host: &Host,
    client: &AgentClient,
    tool: &str,
    profile: Option<&str>,
    decision: &str,
    text: Option<&str>,
) -> Result<(), DbError> {
    let _guard = host
        .agent_storage
        .lock()
        .map_err(|_| DbError::internal("Agent storage lock unavailable"))?;
    let mut file = storage::open_private_log(&host.paths.log.join("agent-audit.jsonl"))
        .map_err(|_| DbError::internal("Cannot open agent audit log"))?;
    let tool = if tool == "tools/list" || TOOLS.contains(&tool) {
        tool
    } else {
        "unknown"
    };
    let profiles = storage::read_profiles(&host.paths.config.join("profiles.json"))?;
    let profile = profile.filter(|id| profiles.iter().any(|p| p.id == *id));
    let entry = json!({"timestamp":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),"client_id":client.id,"tool":tool,"connection_id":profile,"decision":decision,"statement_hash":text.map(hash)});
    writeln!(file, "{entry}")
        .and_then(|_| file.flush())
        .map_err(|_| DbError::internal("Cannot write agent audit log"))
}
#[derive(Deserialize)]
pub struct ClientInput {
    pub id: Option<String>,
    pub name: String,
    pub connections: BTreeMap<String, Permission>,
    pub allowed_tools: Vec<String>,
    pub max_rows: u32,
    pub expires_at: Option<String>,
}
#[tauri::command]
pub fn agent_clients_list(host: tauri::State<'_, Host>) -> Result<Vec<Value>, DbError> {
    Ok(read(&host.paths.config)?
        .iter()
        .map(AgentClient::view)
        .collect())
}
#[tauri::command]
pub fn agent_client_save(
    host: tauri::State<'_, Host>,
    input: ClientInput,
) -> Result<Value, DbError> {
    if input.name.trim().is_empty()
        || !(1..=1000).contains(&input.max_rows)
        || input
            .allowed_tools
            .iter()
            .any(|t| !TOOLS.contains(&t.as_str()))
    {
        return Err(DbError::query("에이전트 클라이언트 설정이 올바르지 않습니다"));
    }
    if let Some(at) = &input.expires_at {
        time::OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339)
            .map_err(|_| DbError::query("expires_at은 RFC3339 형식이어야 합니다"))?;
    }
    let _guard = host
        .agent_storage
        .lock()
        .map_err(|_| DbError::internal("Agent storage lock unavailable"))?;
    let mut clients = read(&host.paths.config)?;
    let fresh = input.id.is_none();
    let mut client = match input.id {
        Some(id) => clients
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| DbError::query("클라이언트를 찾을 수 없습니다"))?,
        None => AgentClient {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            connections: BTreeMap::new(),
            allowed_tools: vec![],
            max_rows: 200,
            expires_at: None,
            revoked: false,
            token_hash: String::new(),
            token_salt: String::new(),
        },
    };
    let profiles = storage::read_profiles(&host.paths.config.join("profiles.json"))?;
    for (id, permission) in &input.connections {
        let profile = profiles
            .iter()
            .find(|p| &p.id == id)
            .ok_or_else(|| DbError::query("Connection not found"))?;
        if matches!(permission, Permission::Read | Permission::Preview)
            && !host
                .driver(&profile.driver_id)?
                .manifest()
                .capabilities
                .read_only_exec
        {
            return Err(DbError::unsupported(
                "이 드라이버는 읽기 전용 실행을 강제할 수 없습니다",
            ));
        }
    }
    client.name = input.name;
    client.connections = input.connections;
    client.allowed_tools = input.allowed_tools;
    client.max_rows = input.max_rows;
    client.expires_at = input.expires_at;
    let raw = fresh.then(token);
    if let Some(raw) = raw.as_ref() {
        set_token(&mut client, raw);
    }
    clients.retain(|c| c.id != client.id);
    clients.push(client.clone());
    save(&host.paths.config, &clients)?;
    Ok(json!({"client":client.view(),"token":raw}))
}
#[tauri::command]
pub fn agent_client_revoke(host: tauri::State<'_, Host>, client_id: String) -> Result<(), DbError> {
    let _guard = host
        .agent_storage
        .lock()
        .map_err(|_| DbError::internal("Agent storage lock unavailable"))?;
    let mut clients = read(&host.paths.config)?;
    let client = clients
        .iter_mut()
        .find(|c| c.id == client_id)
        .ok_or_else(|| DbError::query("클라이언트를 찾을 수 없습니다"))?;
    client.revoked = true;
    save(&host.paths.config, &clients)
}
#[tauri::command]
pub async fn agent_client_rotate(
    host: tauri::State<'_, Host>,
    bridge: tauri::State<'_, crate::agent_bridge::Bridge>,
    client_id: String,
) -> Result<Value, DbError> {
    let raw = token();
    {
        let _guard = host
            .agent_storage
            .lock()
            .map_err(|_| DbError::internal("Agent storage lock unavailable"))?;
        let mut clients = read(&host.paths.config)?;
        let client = clients
            .iter_mut()
            .find(|c| c.id == client_id)
            .ok_or_else(|| DbError::query("클라이언트를 찾을 수 없습니다"))?;
        set_token(client, &raw);
        client.revoked = false;
        save(&host.paths.config, &clients)?;
    }
    if client_id == "local" {
        bridge.update_local_token(&host.paths.config, &raw).await?;
    }
    Ok(json!({"token":raw}))
}
#[tauri::command]
pub fn agent_audit_list(host: tauri::State<'_, Host>) -> Result<Vec<Value>, DbError> {
    let text = match std::fs::read_to_string(host.paths.log.join("agent-audit.jsonl")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(DbError::internal("Cannot read audit log")),
    };
    text.lines()
        .rev()
        .take(50)
        .map(|s| serde_json::from_str(s).map_err(|_| DbError::internal("Invalid audit entry")))
        .collect()
}

pub struct Approval {
    pub fingerprint: String,
    pub created: Instant,
}
pub fn fingerprint(session: &str, req: &impl Serialize) -> Result<String, DbError> {
    Ok(hash(&format!(
        "{session}:{}",
        serde_json::to_string(req).map_err(|_| DbError::query("승인 요청이 올바르지 않습니다"))?
    )))
}
#[tauri::command]
pub fn agent_approve_write(
    host: tauri::State<'_, Host>,
    session_id: String,
    run_id: String,
    req: ExecRequest,
) -> Result<(), DbError> {
    let record = host.session(&session_id)?;
    if host.access_mode(&record)? == AccessMode::ReadOnly {
        return Err(DbError::query("읽기 전용 연결입니다"));
    }
    let fingerprint = fingerprint(&session_id, &req)?;
    let preview = host
        .previews
        .lock()
        .map_err(|_| DbError::internal("Preview lock unavailable"))?
        .remove(&fingerprint);
    if preview.is_none_or(|p| p.created.elapsed().as_secs() >= 60) {
        return Err(DbError::new(
            dbcore::ErrorKind::Auth,
            "일치하는 최신 미리보기가 필요합니다",
        ));
    }
    let mut approvals = host
        .approvals
        .lock()
        .map_err(|_| DbError::internal("Approval lock unavailable"))?;
    approvals.retain(|_, a| a.created.elapsed().as_secs() < 60);
    if approvals.len() >= 64 {
        return Err(DbError::query("승인 대기 요청이 너무 많습니다"));
    }
    approvals.insert(
        run_id,
        Approval {
            fingerprint,
            created: Instant::now(),
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn client(permission: Permission) -> AgentClient {
        AgentClient {
            id: "test".into(),
            name: "test".into(),
            token_hash: String::new(),
            token_salt: String::new(),
            connections: BTreeMap::from([("p".into(), permission)]),
            allowed_tools: TOOLS.iter().map(|s| s.to_string()).collect(),
            max_rows: 3,
            expires_at: None,
            revoked: false,
        }
    }
    #[test]
    fn full_permission_matrix() {
        for permission in [
            Permission::None,
            Permission::Read,
            Permission::Preview,
            Permission::WriteWithApproval,
        ] {
            for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
                for allowed in [false, true] {
                    for requested_read in [false, true] {
                        for supports_read in [false, true] {
                            for tool in TOOLS {
                                let mut c = client(permission);
                                if !allowed {
                                    c.allowed_tools.clear();
                                }
                                let actual =
                                    check(&c, Some("p"), tool, mode, requested_read, supports_read);
                                let expected = if permission == Permission::None {
                                    Err("NotFound")
                                } else if !allowed {
                                    Err("Forbidden")
                                } else if matches!(*tool, "preview_changes" | "plan_import" | "plan_migration" | "plan_grants") {
                                    if mode == AccessMode::ReadOnly
                                        || permission == Permission::Read
                                    {
                                        Err("Forbidden")
                                    } else {
                                        Ok(true)
                                    }
                                } else if *tool == "run_query" {
                                    if mode == AccessMode::ReadOnly || requested_read {
                                        if supports_read {
                                            Ok(true)
                                        } else {
                                            Err("Forbidden")
                                        }
                                    } else if permission == Permission::WriteWithApproval {
                                        Ok(false)
                                    } else {
                                        Err("Forbidden")
                                    }
                                } else {
                                    Ok(true)
                                };
                                assert_eq!(actual,expected,"{permission:?} {mode:?} allowed={allowed} read={requested_read} supported={supports_read} {tool}");
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn tokens_are_salted_private_rotatable_revocable_and_expiring() {
        let dir = std::env::temp_dir().join(format!("axdb-a1-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        migrate_local(&dir, "old-token-secret").unwrap();
        let old = authenticate(&dir, "old-token-secret").unwrap().unwrap();
        let disk = std::fs::read_to_string(dir.join("agent_clients.json")).unwrap();
        assert!(!disk.contains("old-token-secret"));
        assert!(authenticate(&dir, "wrong").unwrap().is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.join("agent_clients.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        migrate_local(&dir, "new-token-secret").unwrap();
        assert!(authenticate(&dir, "old-token-secret").unwrap().is_none());
        assert!(current(&dir, &old).is_err());
        let mut clients = read(&dir).unwrap();
        clients[0].expires_at = Some("2000-01-01T00:00:00Z".into());
        save(&dir, &clients).unwrap();
        assert!(authenticate(&dir, "new-token-secret").unwrap().is_none());
        clients[0].expires_at = None;
        clients[0].revoked = true;
        save(&dir, &clients).unwrap();
        assert!(authenticate(&dir, "new-token-secret").unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn legacy_profile_defaults_to_read_write() {
        let p: storage::Profile =
            serde_json::from_value(json!({"id":"p","name":"p","driver_id":"fixture","fields":{}}))
                .unwrap();
        assert_eq!(p.access_mode, AccessMode::ReadWrite);
    }
}
