//! Redis commands and SCAN navigation through the existing key/value contract.
mod administration;
use async_trait::async_trait;
use dbcore::*;
use redis::{aio::MultiplexedConnection, IntoConnectionInfo, Value as Resp};
use std::{
    collections::BTreeSet,
    future::Future,
    time::{Duration, Instant},
};

pub struct RedisDriver {
    manifest: DriverManifest,
}
impl Default for RedisDriver {
    fn default() -> Self {
        Self::new()
    }
}
impl RedisDriver {
    pub fn new() -> Self {
        let field = |key: &str, kind, default: Option<&str>, secret| FieldSpec {
            key: key.into(),
            label: match key {
                "host" => "Host",
                "port" => "Port",
                "password" => "비밀번호",
                "db" => "데이터베이스 번호",
                "username" => "사용자",
                other => other,
            }
            .into(),
            kind,
            required: key == "host",
            default: default.map(str::to_owned),
            secret,
            placeholder: default.unwrap_or("").into(),
        };
        Self {
            manifest: DriverManifest {
                id: "redis".into(),
                icon_key: Some("redis".into()),
                display_name: "Redis".into(),
                paradigm: Paradigm::KeyValue,
                query_language: QueryLanguage {
                    id: "redis".into(),
                    display: "Redis 명령".into(),
                    editor_mode: "text".into(),
                    supports_params: true,
                },
                connection_fields: vec![
                    field("host", FieldKind::Text, Some("127.0.0.1"), false),
                    field("port", FieldKind::Number, Some("6379"), false),
                    field("password", FieldKind::Password, None, true),
                    field("db", FieldKind::Number, Some("0"), false),
                    field("username", FieldKind::Text, None, false),
                ],
                capabilities: Capabilities {
                    execute_stream: false,
                    backup: false, roles: true,
                    import_rows: true, schema_snapshot: false, ddl_render: false,
                    explain: false,
                    lint: false,
                    read_only_exec: true,
                    cancel: CancelSupport::Abort,
                    timeout: true,
                    params: true,
                    paged_browse: true,
                    ..Default::default()
                },
                default_query: "SCAN 0 MATCH * COUNT 100".into(),
            },
        }
    }
}
#[async_trait]
impl Driver for RedisDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        let host = cfg
            .fields
            .get("host")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "Redis host is required"))?;
        // A host field is persisted; URI credentials must never cross that boundary.
        if host.contains(['/', '@', '?', '#']) {
            return Err(DbError::new(
                ErrorKind::Connection,
                "Use a hostname and separate credential fields",
            ));
        }
        let port = cfg
            .fields
            .get("port")
            .map(String::as_str)
            .unwrap_or("6379")
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "Invalid Redis port"))?;
        let db = cfg
            .fields
            .get("db")
            .map(String::as_str)
            .unwrap_or("0")
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0)
            .ok_or_else(|| DbError::new(ErrorKind::Connection, "Invalid Redis database number"))?;
        let mut info = (host.as_str(), port)
            .into_connection_info()
            .map_err(|_| DbError::new(ErrorKind::Connection, "Invalid Redis host"))?;
        let mut settings = redis::RedisConnectionInfo::default().set_db(db);
        if let Some(user) = cfg.fields.get("username").filter(|s| !s.is_empty()) {
            settings = settings.set_username(user);
        }
        if let Some(secret) = &cfg.secret {
            settings = settings.set_password(secret);
        }
        info = info.set_redis_settings(settings);
        let client = redis::Client::open(info).map_err(|_| {
            DbError::new(ErrorKind::Connection, "Invalid Redis connection settings")
        })?;
        let session = RedisSession {
            client,
            closed: CancelToken::new(),
        };
        session
            .guard(cfg.connect_timeout_ms, CancelToken::new(), async {
                let mut con = session.connection().await?;
                query(&mut con, &mut redis::cmd("PING")).await?;
                Ok(())
            })
            .await?;
        Ok(Box::new(session))
    }
}
struct RedisSession {
    client: redis::Client,
    closed: CancelToken,
}
impl RedisSession {
    async fn connection(&self) -> Result<MultiplexedConnection, DbError> {
        if self.closed.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Connection,
                "Redis session is closed",
            ));
        }
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(error)
    }
    async fn guard<T>(
        &self,
        ms: u64,
        cancel: CancelToken,
        work: impl Future<Output = Result<T, DbError>>,
    ) -> Result<T, DbError> {
        tokio::select! { biased;
            _ = self.closed.cancelled() => Err(DbError::new(ErrorKind::Connection, "Redis session is closed")),
            _ = cancel.cancelled() => Err(DbError::new(ErrorKind::Cancelled, "Redis execution aborted; server work may continue and writes are not rolled back")),
            _ = tokio::time::sleep(Duration::from_millis(ms.max(1))) => Err(DbError::new(ErrorKind::Timeout, "Redis execution timed out; server work may continue and writes are not rolled back")),
            result = work => result,
        }
    }
    async fn scan(
        &self,
        prefix: &[u8],
        pattern: &[u8],
        offset: u64,
        limit: usize,
    ) -> Result<(Vec<Vec<u8>>, bool), DbError> {
        let offset = checked_offset(offset)?;
        let mut con = self.connection().await?;
        let mut prefix_pattern = Vec::new();
        for b in prefix {
            if b"*?[]\\".contains(b) {
                prefix_pattern.push(b'\\');
            }
            prefix_pattern.push(*b);
        }
        prefix_pattern.push(b'*');
        let pattern = if pattern == b"*" {
            prefix_pattern.as_slice()
        } else {
            pattern
        };
        let mut cursor = 0u64;
        let mut seen = BTreeSet::new();
        let mut keys = Vec::new();
        loop {
            let reply = query(
                &mut con,
                redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(pattern)
                    .arg("COUNT")
                    .arg(100),
            )
            .await?;
            let (next, batch): (u64, Vec<Vec<u8>>) = redis::from_redis_value(reply)
                .map_err(|_| DbError::query("Invalid SCAN response"))?;
            for key in batch {
                if key.starts_with(prefix) && seen.insert(key.clone()) {
                    if seen.len() > offset {
                        keys.push(key);
                    }
                    if keys.len() > limit {
                        keys.pop();
                        return Ok((keys, true));
                    }
                }
            }
            cursor = next;
            if cursor == 0 {
                return Ok((keys, false));
            }
        }
    }
    async fn key_result(&self, key: &[u8], page: &PageRequest) -> Result<ResultSet, DbError> {
        let offset = checked_offset(page.offset)?;
        let limit = clamp_row_limit(page.limit) as usize;
        let mut con = self.connection().await?;
        let kind: String =
            redis::from_redis_value(query(&mut con, redis::cmd("TYPE").arg(key)).await?)
                .map_err(|_| DbError::query("Invalid TYPE response"))?;
        let ttl: i64 = redis::from_redis_value(query(&mut con, redis::cmd("TTL").arg(key)).await?)
            .map_err(|_| DbError::query("Invalid TTL response"))?;
        let (columns, mut rows) = match kind.as_str() {
            "none" => (vec!["value"], vec![]),
            "string" => {
                let value = query(&mut con, redis::cmd("GET").arg(key)).await?;
                (
                    vec!["value"],
                    if offset == 0 {
                        vec![vec![map_value(value)]]
                    } else {
                        vec![]
                    },
                )
            }
            "list" => {
                let values = array(
                    query(
                        &mut con,
                        redis::cmd("LRANGE")
                            .arg(key)
                            .arg(offset)
                            .arg(offset + limit),
                    )
                    .await?,
                )?;
                (
                    vec!["index", "value"],
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| vec![Value::Int((offset + i).to_string()), map_value(v)])
                        .collect(),
                )
            }
            "hash" | "set" | "zset" => {
                let command = match kind.as_str() {
                    "hash" => "HSCAN",
                    "set" => "SSCAN",
                    _ => "ZSCAN",
                };
                let mut cursor = 0u64;
                let mut all = Vec::new();
                let mut seen = BTreeSet::new();
                loop {
                    let reply = array(
                        query(
                            &mut con,
                            redis::cmd(command)
                                .arg(key)
                                .arg(cursor)
                                .arg("COUNT")
                                .arg(100),
                        )
                        .await?,
                    )?;
                    if reply.len() != 2 {
                        return Err(DbError::query("Invalid scan response"));
                    }
                    cursor = redis::from_redis_value(reply[0].clone())
                        .map_err(|_| DbError::query("Invalid scan cursor"))?;
                    let batch = array(reply[1].clone())?;
                    let rows = if kind == "set" {
                        batch.into_iter().map(|v| vec![map_value(v)]).collect()
                    } else {
                        pairs(batch)?
                    };
                    for row in rows {
                        let identity = serde_json::to_string(&row[0])
                            .map_err(|_| DbError::internal("Invalid Redis value"))?;
                        if seen.insert(identity) {
                            all.push(row);
                        }
                        if all.len() > offset + limit {
                            break;
                        }
                    }
                    if cursor == 0 || all.len() > offset + limit {
                        break;
                    }
                }
                (
                    if kind == "set" {
                        vec!["value"]
                    } else if kind == "hash" {
                        vec!["field", "value"]
                    } else {
                        vec!["member", "score"]
                    },
                    all.into_iter().skip(offset).collect(),
                )
            }
            "stream" => {
                let entries = array(
                    query(
                        &mut con,
                        redis::cmd("XRANGE")
                            .arg(key)
                            .arg("-")
                            .arg("+")
                            .arg("COUNT")
                            .arg(offset + limit + 1),
                    )
                    .await?,
                )?;
                (
                    vec!["id", "fields"],
                    stream_rows(entries)?.into_iter().skip(offset).collect(),
                )
            }
            _ => return Err(DbError::unsupported("Redis key type is not supported")),
        };
        let more = rows.len() > limit;
        rows.truncate(limit);
        let mut result = table(&columns, rows, more);
        result.title = "Redis key".into();
        result.message = Some(format!(
            "TYPE: {kind}; TTL: {ttl} seconds (-1 persistent, -2 missing)"
        ));
        result.page = Some(PageInfo {
            offset: page.offset,
            limit: limit as u32,
            has_more: more,
        });
        Ok(result)
    }
}
#[async_trait]
impl Session for RedisSession {
    async fn roles(&self)->Result<RolesSnapshot,DbError>{self.native_roles().await}
    async fn plan_grants(&self,changes:&[GrantChange])->Result<GrantPlan,DbError>{self.native_grants(changes)}
    async fn import_rows(&self,req:&EditRequest,preview:bool,cancel:CancelToken)->Result<ImportOutput,DbError>{self.import_batch(req,preview,cancel).await}

    async fn ping(&self) -> Result<(), DbError> {
        self.guard(30_000, CancelToken::new(), async {
            query(&mut self.connection().await?, &mut redis::cmd("PING"))
                .await
                .map(|_| ())
        })
        .await
    }
    async fn server_info(&self) -> Result<String, DbError> {
        self.guard(30_000, CancelToken::new(), async {
            let reply = query(
                &mut self.connection().await?,
                redis::cmd("INFO").arg("server"),
            )
            .await?;
            let text: String = redis::from_redis_value(reply)
                .map_err(|_| DbError::query("Invalid Redis server info"))?;
            Ok(text
                .lines()
                .find(|s| s.starts_with("redis_version:"))
                .unwrap_or("Redis")
                .trim()
                .into())
        })
        .await
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        let path = match parent {
            Some(p) => parse_path(p)?,
            None => serde_json::json!(["scan", [], "*", 0]),
        };
        if path[0] == "key" {
            return Ok(vec![]);
        }
        let (prefix, pattern, offset) = scan_path(&path)?;
        self.guard(30_000, CancelToken::new(), async {
            let (keys, more) = self.scan(&prefix, pattern.as_bytes(), offset, 200).await?;
            let mut nodes = Vec::new();
            let mut folders = BTreeSet::new();
            for key in keys {
                if let Some(i) = key[prefix.len()..].iter().position(|b| *b == b':') {
                    let folder = key[..prefix.len() + i + 1].to_vec();
                    if folders.insert(folder.clone()) {
                        nodes.push(node(
                            serde_json::json!(["scan", folder, pattern, 0]),
                            &display(&folder[prefix.len()..]),
                            true,
                            Icon::Folder,
                            vec![],
                        ));
                    }
                }
                if !key[prefix.len()..].contains(&b':') {
                    nodes.push(node(
                        serde_json::json!(["key", key]),
                        &display(if key.len() == prefix.len() {
                            &key
                        } else {
                            &key[prefix.len()..]
                        }),
                        false,
                        Icon::Key,
                        vec![NodeAction {
                            id: "browse".into(),
                            label: "값 보기 (TYPE / TTL)".into(),
                            kind: NodeActionKind::Browse,
                        }],
                    ));
                }
            }
            if more {
                nodes.push(node(
                    serde_json::json!(["scan", prefix, pattern, offset + 200]),
                    "More keys…",
                    true,
                    Icon::Folder,
                    vec![],
                ));
            }
            Ok(nodes)
        })
        .await
    }
    async fn browse(&self, id: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        if !page.filter.is_empty() {
            return Err(DbError::unsupported(
                "Redis filtered browse is not supported",
            ));
        }
        let path = parse_path(id)?;
        self.guard(30_000, CancelToken::new(), async {
            if path[0] == "key" {
                let key: Vec<u8> = serde_json::from_value(path[1].clone())
                    .map_err(|_| DbError::query("Invalid key node"))?;
                return self.key_result(&key, &page).await;
            }
            let (prefix, pattern, _) = scan_path(&path)?;
            let limit = clamp_row_limit(page.limit);
            let (keys, more) = self
                .scan(&prefix, pattern.as_bytes(), page.offset, limit as usize)
                .await?;
            let mut result = table(
                &["key"],
                keys.into_iter().map(|k| vec![bytes(k)]).collect(),
                more,
            );
            result.page = Some(PageInfo {
                offset: page.offset,
                limit,
                has_more: more,
            });
            Ok(result)
        })
        .await
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        let args = tokenize(&req.text, &req.params)?;
        let command = String::from_utf8(args[0].clone())
            .map_err(|_| DbError::query("Command must be ASCII"))?
            .to_ascii_uppercase();
        if command == "KEYS" {
            return Err(DbError::unsupported("Use SCAN instead of KEYS"));
        }
        if req.read_only && !read_command(&command) {
            return Err(DbError::unsupported(
                "Redis read-only execution permits only whitelisted read commands",
            ));
        }
        if matches!(
            command.as_str(),
            "AUTH"
                | "HELLO"
                | "SELECT"
                | "MULTI"
                | "EXEC"
                | "WATCH"
                | "SUBSCRIBE"
                | "PSUBSCRIBE"
                | "SSUBSCRIBE"
                | "MONITOR"
                | "QUIT"
                | "RESET"
                | "CLIENT"
        ) {
            return Err(DbError::unsupported(
                "Connection state commands are not supported",
            ));
        }
        let started = Instant::now();
        self.guard(req.timeout_ms, cancel, async {
            // Dedicated canonical connection: dropping it aborts its task/socket, including blocking reads.
            let mut con = self.connection().await?;
            let mut cmd = redis::cmd(&command);
            for arg in &args[1..] {
                cmd.arg(arg);
            }
            let reply = query(&mut con, &mut cmd).await?;
            let limit = clamp_row_limit(req.row_limit) as usize;
            let mut message = None;
            let (columns, mut rows) = match (command.as_str(), reply) {
                ("HGETALL", Resp::Array(v)) => (vec!["field", "value"], pairs(v)?),
                ("ZRANGE" | "ZREVRANGE" | "ZRANGEBYSCORE" | "ZREVRANGEBYSCORE", Resp::Array(v))
                    if args.iter().skip(4).any(|a| a.eq_ignore_ascii_case(b"WITHSCORES")) =>
                {
                    (vec!["member", "score"], pairs(v)?)
                }
                ("XRANGE" | "XREVRANGE", Resp::Array(v)) => (vec!["id", "fields"], stream_rows(v)?),
                ("SCAN" | "HSCAN" | "SSCAN" | "ZSCAN", Resp::Array(v)) if v.len() == 2 => {
                    let mut parts = v.into_iter();
                    let cursor = map_value(parts.next().expect("two scan elements"));
                    let values = array(parts.next().expect("two scan elements"))?;
                    message = Some(format!(
                        "SCAN cursor: {} (0 means complete); truncation may omit members, increase row limit or reduce COUNT",
                        display_value(&cursor)
                    ));
                    if matches!(command.as_str(), "HSCAN" | "ZSCAN") {
                        (if command == "HSCAN" { vec!["field", "value"] } else { vec!["member", "score"] }, pairs(values)?)
                    } else {
                        (vec!["key"], values.into_iter().map(|v| vec![map_value(v)]).collect())
                    }
                }
                (_, Resp::Array(v)) => (vec!["value"], v.into_iter().map(|v| vec![map_value(v)]).collect()),
                (_, v) => (vec!["value"], vec![vec![map_value(v)]]),
            };
            let more = rows.len() > limit;
            rows.truncate(limit);
            let mut result = table(&columns, rows, more);
            result.title = command;
            result.message = message;
            result.elapsed_ms = started.elapsed().as_millis() as u64;
            Ok(vec![result])
        }).await
    }

    async fn expand(&self, _: &str, _: u32) -> Result<ResultSet, DbError> {
        Err(DbError::unsupported("Redis has no graph expansion"))
    }
    async fn close(&self) {
        self.closed.cancel();
    }
}
fn error(e: redis::RedisError) -> DbError {
    let kind = if e.kind() == redis::ErrorKind::AuthenticationFailed {
        ErrorKind::Auth
    } else if e.is_io_error() {
        ErrorKind::Connection
    } else {
        ErrorKind::Query
    };
    // Server errors may echo arguments or credentials; never forward their text.
    DbError::new(
        kind,
        "Redis request failed; check connection, credentials, command and argument types",
    )
}
async fn query(con: &mut MultiplexedConnection, cmd: &mut redis::Cmd) -> Result<Resp, DbError> {
    cmd.query_async(con).await.map_err(error)
}
fn read_command(c: &str) -> bool {
    matches!(
        c,
        "PING"
            | "GET"
            | "MGET"
            | "GETRANGE"
            | "STRLEN"
            | "EXISTS"
            | "TYPE"
            | "TTL"
            | "PTTL"
            | "SCAN"
            | "HGET"
            | "HMGET"
            | "HGETALL"
            | "HLEN"
            | "HKEYS"
            | "HVALS"
            | "HEXISTS"
            | "HSCAN"
            | "LINDEX"
            | "LLEN"
            | "LRANGE"
            | "SCARD"
            | "SISMEMBER"
            | "SMISMEMBER"
            | "SMEMBERS"
            | "SSCAN"
            | "SRANDMEMBER"
            | "SDIFF"
            | "SINTER"
            | "SUNION"
            | "ZCARD"
            | "ZCOUNT"
            | "ZSCORE"
            | "ZMSCORE"
            | "ZRANK"
            | "ZREVRANK"
            | "ZRANGE"
            | "ZREVRANGE"
            | "ZRANGEBYSCORE"
            | "ZREVRANGEBYSCORE"
            | "ZSCAN"
            | "XLEN"
            | "XRANGE"
            | "XREVRANGE"
            | "DBSIZE"
    )
}
// shortcut: offset pages rescan mutable data up to 100000; use native cursors for larger navigation.
fn checked_offset(offset: u64) -> Result<usize, DbError> {
    if offset > 100_000 {
        Err(DbError::query("Redis browse offset must be at most 100000"))
    } else {
        Ok(offset as usize)
    }
}
fn array(v: Resp) -> Result<Vec<Resp>, DbError> {
    if let Resp::Array(v) = v {
        Ok(v)
    } else {
        Err(DbError::query("Expected Redis array"))
    }
}
fn pairs(v: Vec<Resp>) -> Result<Vec<Vec<Value>>, DbError> {
    if !v.len().is_multiple_of(2) {
        return Err(DbError::query("Expected Redis pairs"));
    }
    Ok(v.as_chunks::<2>()
        .0
        .iter()
        .map(|p| vec![map_value(p[0].clone()), map_value(p[1].clone())])
        .collect())
}
fn stream_rows(v: Vec<Resp>) -> Result<Vec<Vec<Value>>, DbError> {
    v.into_iter()
        .map(|v| {
            let pair = array(v)?;
            if pair.len() != 2 {
                return Err(DbError::query("Invalid stream entry"));
            }
            Ok(vec![
                map_value(pair[0].clone()),
                Value::List(
                    pairs(array(pair[1].clone())?)?
                        .into_iter()
                        .map(Value::List)
                        .collect(),
                ),
            ])
        })
        .collect()
}
fn bytes(v: Vec<u8>) -> Value {
    match String::from_utf8(v) {
        Ok(s) => Value::Text(s),
        Err(e) => Value::Bytes {
            len: e.as_bytes().len() as u64,
            hex: e
                .as_bytes()
                .iter()
                .take(64)
                .map(|b| format!("{b:02x}"))
                .collect(),
        },
    }
}
fn map_value(v: Resp) -> Value {
    match v {
        Resp::Nil => Value::Null,
        Resp::Int(n) => Value::Int(n.to_string()),
        Resp::BulkString(v) => bytes(v),
        Resp::SimpleString(s) => Value::Text(s),
        Resp::Okay => Value::Text("OK".into()),
        Resp::Array(v) => Value::List(v.into_iter().map(map_value).collect()),
        _ => Value::Other {
            type_name: "RESP".into(),
            text: "Unsupported RESP3 value".into(),
        },
    }
}
fn table(columns: &[&str], rows: Vec<Vec<Value>>, truncated: bool) -> ResultSet {
    ResultSet {
        row_count: rows.len() as u64,
        table: Some(Table {
            columns: columns
                .iter()
                .map(|n| Column {
                    name: (*n).into(),
                    type_name: "Redis".into(),
                })
                .collect(),
            rows,
        }),
        truncated,
        ..Default::default()
    }
}
fn display(v: &[u8]) -> String {
    match std::str::from_utf8(v) {
        Ok(s) => s.into(),
        Err(_) => format!(
            "0x{}",
            v.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ),
    }
}
fn display_value(v: &Value) -> String {
    match v {
        Value::Text(s) | Value::Int(s) => s.clone(),
        _ => "unknown".into(),
    }
}
fn node(
    id: serde_json::Value,
    label: &str,
    children: bool,
    icon: Icon,
    actions: Vec<NodeAction>,
) -> ExplorerNode {
    ExplorerNode {
        id: id.to_string(),
        label: label.into(),
        kind: if children { "folder" } else { "key" }.into(),
        icon,
        has_children: children,
        detail: None,
        actions,
    }
}
fn parse_path(id: &str) -> Result<serde_json::Value, DbError> {
    let v: serde_json::Value =
        serde_json::from_str(id).map_err(|_| DbError::query("Invalid Redis node"))?;
    if v.is_array() && matches!(v[0].as_str(), Some("key" | "scan")) {
        Ok(v)
    } else {
        Err(DbError::query("Invalid Redis node"))
    }
}
fn scan_path(v: &serde_json::Value) -> Result<(Vec<u8>, String, u64), DbError> {
    if v[0] != "scan" {
        return Err(DbError::query("Invalid scan node"));
    }
    Ok((
        serde_json::from_value(v[1].clone()).map_err(|_| DbError::query("Invalid key prefix"))?,
        v[2].as_str()
            .ok_or_else(|| DbError::query("Invalid key pattern"))?
            .into(),
        v[3].as_u64()
            .ok_or_else(|| DbError::query("Invalid scan offset"))?,
    ))
}

// Parameters occupy a whole unquoted $name token and never change command boundaries.
fn tokenize(
    text: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<Vec<u8>>, DbError> {
    if text.contains(['\r', '\n']) {
        return Err(DbError::query("Enter one Redis command line"));
    }
    let mut args = Vec::new();
    let mut chars = text.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut token = Vec::new();
        let mut quote = None;
        let mut literal = false;
        while let Some(&c) = chars.peek() {
            if quote.is_none() && c.is_whitespace() {
                break;
            }
            chars.next();
            if c == '\\' {
                literal = true;
                let c = chars
                    .next()
                    .ok_or_else(|| DbError::query("Incomplete Redis escape"))?;
                if c == 'x' {
                    let hex: String = chars.by_ref().take(2).collect();
                    if hex.len() != 2 {
                        return Err(DbError::query("Invalid hex escape"));
                    }
                    token.push(
                        u8::from_str_radix(&hex, 16)
                            .map_err(|_| DbError::query("Invalid hex escape"))?,
                    );
                } else {
                    let c = match c {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '0' => '\0',
                        c => c,
                    };
                    let mut b = [0; 4];
                    token.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                }
            } else if matches!(c, '\'' | '"') && (quote.is_none() || quote == Some(c)) {
                literal = true;
                quote = if quote.is_some() { None } else { Some(c) };
            } else {
                let mut b = [0; 4];
                token.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            }
        }
        if quote.is_some() {
            return Err(DbError::query("Unclosed Redis quote"));
        }
        if !literal && token.starts_with(b"$") {
            if args.is_empty() {
                return Err(DbError::query("Redis command cannot be a parameter"));
            }
            let name = std::str::from_utf8(&token[1..])
                .map_err(|_| DbError::query("Invalid parameter name"))?;
            let v = params
                .get(name)
                .ok_or_else(|| DbError::query("Missing Redis parameter"))?;
            token = match v {
                serde_json::Value::String(s) => s.as_bytes().to_vec(),
                serde_json::Value::Number(n) => n.to_string().into_bytes(),
                serde_json::Value::Bool(b) => b.to_string().into_bytes(),
                _ => {
                    return Err(DbError::query(
                        "Redis parameters must be strings, numbers or booleans",
                    ))
                }
            };
        }
        args.push(token);
    }
    if args.is_empty() {
        return Err(DbError::query("Redis command is empty"));
    }
    Ok(args)
}
#[cfg(test)]
mod tests;
