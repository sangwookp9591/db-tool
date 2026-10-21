mod iam;
pub mod model;
pub mod parse;
mod proxy;
mod ssh;
#[cfg(test)]
mod tests;
mod tls;
use dbcore::{DbError, DriverManifest};
use model::*;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Stage {
        stage: String,
        status: String,
        elapsed_ms: u64,
        code: Option<String>,
        help_key: Option<String>,
    },
    HostKey {
        request_id: String,
        host: String,
        port: u16,
        fingerprint: String,
    },
    Network {
        profile_id: String,
        state: String,
        attempt: u32,
        code: Option<String>,
    },
}
type EventSink = Arc<dyn Fn(Event) + Send + Sync>;
#[derive(Default)]
pub struct State {
    pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    sink: Mutex<Option<EventSink>>,
    known_hosts: tokio::sync::Mutex<()>,
    tokens: tokio::sync::Mutex<HashMap<String, iam::Token>>,
}
impl State {
    pub fn set_sink(&self, sink: EventSink) {
        if let Ok(mut slot) = self.sink.lock() {
            *slot = Some(sink);
        }
    }
    pub fn emit(&self, event: Event) {
        let sink = self.sink.lock().ok().and_then(|s| s.clone());
        if let Some(sink) = sink {
            sink(event);
        }
    }
    async fn confirm(&self, host: String, port: u16, fingerprint: String) -> Result<(), DbError> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| error("SSH_CONFIRM_UNAVAILABLE"))?
            .insert(id.clone(), tx);
        struct Pending<'a> {
            id: String,
            state: &'a State,
        }
        impl Drop for Pending<'_> {
            fn drop(&mut self) {
                if let Ok(mut p) = self.state.pending.lock() {
                    p.remove(&self.id);
                }
            }
        }
        let _pending = Pending {
            id: id.clone(),
            state: self,
        };
        self.emit(Event::HostKey {
            request_id: id,
            host,
            port,
            fingerprint,
        });
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(true)) => Ok(()),
            Ok(_) => Err(error("SSH_HOST_KEY_REJECTED")),
            Err(_) => Err(error("SSH_HOST_KEY_CONFIRM_TIMEOUT")),
        }
    }
    pub fn answer(&self, id: &str, accept: bool) -> Result<(), DbError> {
        self.pending
            .lock()
            .map_err(|_| error("SSH_CONFIRM_UNAVAILABLE"))?
            .remove(id)
            .ok_or_else(|| error("SSH_CONFIRM_EXPIRED"))?
            .send(accept)
            .map_err(|_| error("SSH_CONFIRM_EXPIRED"))
    }
    pub fn invalidate(&self, profile_id: &str) {
        if let Ok(mut tokens) = self.tokens.try_lock() {
            tokens.retain(|key, _| !key.starts_with(&format!("{profile_id}:")));
        }
    }
}
pub struct Tunnel {
    cancel: CancellationToken,
}
impl Tunnel {
    pub fn close(&self) { self.cancel.cancel(); }
}
impl Drop for Tunnel {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub struct Prepared {
    pub fields: BTreeMap<String, String>,
    pub secret: Option<String>,
    pub tunnel: Option<Tunnel>,
    pub tls_offloaded: bool,
}
#[derive(Clone, Default)]
pub struct Progress {
    channel: Option<tauri::ipc::Channel<Event>>,
    latest: Arc<Mutex<Option<(String, String)>>>,
}
impl Progress {
    pub(crate) fn new(channel: tauri::ipc::Channel<Event>) -> Self {
        Self {
            channel: Some(channel),
            ..Default::default()
        }
    }
    pub fn stage(&self, stage: &str, status: &str, start: Instant, error: Option<&DbError>) {
        if let Ok(mut latest) = self.latest.lock() {
            *latest = Some((stage.into(), status.into()));
        }
        if let Some(channel) = &self.channel {
            let _ = channel.send(Event::Stage {
                stage: stage.into(),
                status: status.into(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                code: error.map(|e| e.message.clone()),
                help_key: error.and_then(|e| e.detail.clone()),
            });
        }
    }
    pub(crate) fn failure(&self, error: &DbError, start: Instant) {
        let latest = self.latest.lock().ok().and_then(|s| s.clone());
        if latest
            .as_ref()
            .is_some_and(|(_, status)| status == "failed")
        {
            return;
        }
        let stage = latest
            .as_ref()
            .filter(|(_, status)| status == "running")
            .map(|(stage, _)| stage.as_str())
            .unwrap_or("db_auth");
        self.stage(stage, "failed", start, Some(error));
    }
}
struct Endpoint {
    host: String,
    port: u16,
    uri: Option<url::Url>,
}
fn endpoint(
    fields: &BTreeMap<String, String>,
    manifest: &DriverManifest,
) -> Result<Endpoint, DbError> {
    let field = |key: &str| {
        fields.get(key).cloned().or_else(|| {
            manifest
                .connection_fields
                .iter()
                .find(|f| f.key == key)
                .and_then(|f| f.default.clone())
        })
    };
    if let Some(uri) = field("uri") {
        let url = url::Url::parse(&uri).map_err(|_| error("INVALID_CONNECTION_URI"))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(error("USE_SEPARATE_CREDENTIAL_FIELDS"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| error("INVALID_HOST"))?
            .trim_matches(['[', ']'])
            .to_owned();
        valid_host(&host)?;
        let port = url
            .port()
            .or(match url.scheme() {
                "mongodb" => Some(27017),
                "bolt" | "neo4j" | "bolt+s" | "neo4j+s" | "bolt+ssc" | "neo4j+ssc" => Some(7687),
                _ => None,
            })
            .ok_or_else(|| error("NETWORK_PORT_REQUIRED"))?;
        Ok(Endpoint {
            host,
            port,
            uri: Some(url),
        })
    } else {
        let host = field("host").ok_or_else(|| error("NETWORK_REQUIRES_TCP_DRIVER"))?;
        valid_host(&host)?;
        let port = field("port")
            .and_then(|p| p.parse::<u16>().ok())
            .filter(|p| *p > 0)
            .ok_or_else(|| error("INVALID_PORT"))?;
        Ok(Endpoint {
            host,
            port,
            uri: None,
        })
    }
}
fn rewrite(
    fields: &mut BTreeMap<String, String>,
    endpoint: &Endpoint,
    local_port: u16,
) -> Result<(), DbError> {
    if let Some(mut uri) = endpoint.uri.clone() {
        let scheme = uri.scheme().to_owned();
        if scheme.starts_with("neo4j") {
            uri.set_scheme(&scheme.replacen("neo4j", "bolt", 1))
                .map_err(|_| error("INVALID_CONNECTION_URI"))?;
        }
        uri.set_host(Some("127.0.0.1"))
            .map_err(|_| error("INVALID_CONNECTION_URI"))?;
        uri.set_port(Some(local_port))
            .map_err(|_| error("INVALID_PORT"))?;
        if scheme == "mongodb" {
            let query: Vec<(String, String)> = uri
                .query_pairs()
                .filter(|(k, _)| k != "directConnection")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            uri.set_query(None);
            uri.query_pairs_mut()
                .extend_pairs(query)
                .append_pair("directConnection", "true");
        }
        fields.insert("uri".into(), uri.to_string());
    } else {
        fields.insert("host".into(), "127.0.0.1".into());
        fields.insert("port".into(), local_port.to_string());
        fields.insert("tls_server_name".into(), endpoint.host.clone());
    }
    Ok(())
}
// Keep network secrets explicit at this boundary so test credentials stay in memory.
#[allow(clippy::too_many_arguments)]
pub async fn prepare(
    profile: &crate::storage::Profile,
    fields: BTreeMap<String, String>,
    secret: Option<String>,
    network_secrets: Secrets,
    manifest: &DriverManifest,
    path: &Path,
    state: Arc<State>,
    progress: Progress,
) -> Result<Prepared, DbError> {
    let mut prepared = Prepared {
        fields,
        secret,
        tunnel: None,
        tls_offloaded: false,
    };
    let network = profile.network.as_ref();
    if let Some(network) = network {
        network.validate()?;
    }
    let iam = !matches!(profile.auth, Auth::Password);
    let tunneled = network.is_some_and(|n| n.ssh.is_some() || n.proxy.is_some());
    if !iam && !tunneled {
        return Ok(prepared);
    }
    let endpoint = endpoint(&prepared.fields, manifest)?;
    if iam {
        // IAM credentials must never be sent over plaintext or fall back from TLS.
        if !manifest
            .connection_fields
            .iter()
            .any(|f| f.key == "sslmode")
        {
            return Err(error("IAM_REQUIRES_POSTGRES_TLS"));
        }
        prepared
            .fields
            .insert("sslmode".into(), "verify-full".into());
        let user = prepared
            .fields
            .get("user")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| error("IAM_USER_REQUIRED"))?;
        use sha2::Digest;
        let binding = serde_json::to_vec(&(&profile.auth, &endpoint.host, endpoint.port, user))
            .map_err(|_| error("IAM_CREDENTIAL_INVALID"))?;
        let cache_key = format!("{}:{:x}", profile.id, sha2::Sha256::digest(binding));
        let mut cache = state.tokens.lock().await;
        cache.retain(|_, token| token.fresh());
        if !cache.get(&cache_key).is_some_and(iam::Token::fresh) {
            cache.insert(
                cache_key.clone(),
                iam::token(&profile.auth, &endpoint.host, endpoint.port, user).await?,
            );
        }
        prepared.secret = cache.get(&cache_key).map(|t| t.value.clone());
    }
    if !tunneled {
        return Ok(prepared);
    }
    let network = network.cloned().unwrap_or_default();
    let start = Instant::now();
    progress.stage("dns", "running", start, None);
    let first = network
        .proxy
        .as_ref()
        .map(|p| (p.host.as_str(), p.port))
        .or_else(|| {
            network.ssh.as_ref().map(|s| {
                s.jump_hosts
                    .first()
                    .map(|h| (h.host.as_str(), h.port))
                    .unwrap_or((&s.host, s.port))
            })
        })
        .unwrap_or((&endpoint.host, endpoint.port));
    let dns = tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host(first))
        .await
        .map(|r| r.map(|mut addresses| addresses.next().is_some()));
    if !matches!(dns, Ok(Ok(true))) {
        let e = error("DNS_FAILED");
        progress.stage("dns", "failed", start, Some(&e));
        return Err(e);
    }
    progress.stage("dns", "passed", start, None);
    let start = Instant::now();
    progress.stage("ssh", "running", start, None);
    let ssh_connection = if let Some(ssh) = &network.ssh {
        match ssh::connect(
            ssh,
            network.proxy.as_ref(),
            &profile.id,
            &network_secrets,
            &path.join("known_hosts"),
            state.clone(),
        )
        .await
        {
            Ok(c) => Some(c),
            Err(e) => {
                progress.stage("ssh", "failed", start, Some(&e));
                return Err(e);
            }
        }
    } else {
        None
    };
    progress.stage(
        "ssh",
        if ssh_connection.is_some() {
            "passed"
        } else {
            "skipped"
        },
        start,
        None,
    );
    let tls = tls::immediate(&prepared.fields, &endpoint).inspect_err(|e| {
        progress.stage("tls", "failed", Instant::now(), Some(e));
    })?;
    prepared.tls_offloaded = tls.is_some();
    let route = Arc::new(Route {
        tls,
        network,
        network_secrets,
        profile_id: profile.id.clone(),
        host: endpoint.host.clone(),
        port: endpoint.port,
        path: path.join("known_hosts"),
        state,
        ssh: tokio::sync::Mutex::new(ssh_connection),
    });
    let start = Instant::now();
    progress.stage("tunnel", "running", start, None);
    let first_stream = match tokio::time::timeout(Duration::from_secs(15), route.open_raw()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            progress.stage("tunnel", "failed", start, Some(&e));
            return Err(e);
        }
        Err(_) => {
            let e = error("TUNNEL_TIMEOUT");
            progress.stage("tunnel", "failed", start, Some(&e));
            return Err(e);
        }
    };
    progress.stage("tunnel", "passed", start, None);
    let first_stream = if prepared.tls_offloaded {
        let start = Instant::now();
        progress.stage("tls", "running", start, None);
        let stream = tokio::time::timeout(Duration::from_secs(15), route.secure(first_stream))
            .await
            .map_err(|_| error("TLS_TIMEOUT"))??;
        progress.stage("tls", "passed", start, None);
        stream
    } else {
        first_stream
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| error("LOCAL_PORT_UNAVAILABLE"))?;
    rewrite(
        &mut prepared.fields,
        &endpoint,
        listener
            .local_addr()
            .map_err(|_| error("LOCAL_PORT_UNAVAILABLE"))?
            .port(),
    )?;
    if prepared.tls_offloaded {
        tls::local_plaintext(&mut prepared.fields)?;
    }
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    tokio::spawn(async move {
        let mut first_stream = Some(first_stream);
        let mut transfers = tokio::task::JoinSet::new();
        let mut monitor = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
             _=task_cancel.cancelled()=>break,
             _=monitor.tick()=>{
                if route.ssh.try_lock().ok().is_some_and(|ssh|ssh.as_ref().is_some_and(ssh::Connection::closed)) {
                    let reconnect=route.clone();transfers.spawn(async move { let _ = reconnect.open().await; });
                }
             },
             Some(_)=transfers.join_next(),if !transfers.is_empty()=>{},
             accepted=listener.accept(),if transfers.len()<64=>{let Ok((mut local,_))=accepted else{break;};let route=route.clone();let initial=first_stream.take();transfers.spawn(async move{let upstream=match initial{Some(s)=>Ok(s),None=>tokio::time::timeout(Duration::from_secs(120),route.open()).await.unwrap_or_else(|_|Err(error("TUNNEL_TIMEOUT")))};if let Ok(mut upstream)=upstream{let _=tokio::io::copy_bidirectional(&mut local,&mut upstream).await;}});}
            }
        }
        transfers.abort_all();
    });
    prepared.tunnel = Some(Tunnel { cancel });
    Ok(prepared)
}
struct Route {
    tls: Option<tls::Tls>,
    network: Network,
    network_secrets: Secrets,
    profile_id: String,
    host: String,
    port: u16,
    path: PathBuf,
    state: Arc<State>,
    ssh: tokio::sync::Mutex<Option<ssh::Connection>>,
}
impl Route {
    async fn open(&self) -> Result<proxy::BoxStream, DbError> {
        let stream = self.open_raw().await?;
        self.secure(stream).await
    }
    async fn secure(&self, stream: proxy::BoxStream) -> Result<proxy::BoxStream, DbError> {
        if let Some(tls) = &self.tls {
            let stream = tls
                .connector
                .connect(&self.host, stream)
                .await
                .map_err(|_| error("TLS_FAILED"))?;
            Ok(Box::new(stream))
        } else {
            Ok(stream)
        }
    }
    async fn open_raw(&self) -> Result<proxy::BoxStream, DbError> {
        if let Some(config) = &self.network.ssh {
            let mut ssh = self.ssh.lock().await;
            if ssh.as_ref().is_none_or(ssh::Connection::closed) {
                *ssh = None;
                let mut last = error("SSH_TRANSPORT_FAILED");
                for attempt in 0..5u32 {
                    self.state.emit(Event::Network {
                        profile_id: self.profile_id.clone(),
                        state: "reconnecting".into(),
                        attempt: attempt + 1,
                        code: None,
                    });
                    tokio::time::sleep(Duration::from_secs(1u64 << attempt)).await;
                    match ssh::connect(
                        config,
                        self.network.proxy.as_ref(),
                        &self.profile_id,
                        &self.network_secrets,
                        &self.path,
                        self.state.clone(),
                    )
                    .await
                    {
                        Ok(c) => {
                            *ssh = Some(c);
                            break;
                        }
                        Err(e) => {
                            if matches!(
                                e.message.as_str(),
                                "SSH_HOST_KEY_CHANGED"
                                    | "SSH_HOST_KEY_REJECTED"
                                    | "SSH_HOST_KEY_CONFIRM_TIMEOUT"
                                    | "SSH_AUTH_REJECTED"
                                    | "SSH_KEY_PERMISSIONS"
                            ) {
                                self.state.emit(Event::Network {
                                    profile_id: self.profile_id.clone(),
                                    state: "disconnected".into(),
                                    attempt: attempt + 1,
                                    code: Some(e.message.clone()),
                                });
                                return Err(e);
                            }
                            last = e;
                        }
                    }
                }
                if ssh.is_none() {
                    self.state.emit(Event::Network {
                        profile_id: self.profile_id.clone(),
                        state: "disconnected".into(),
                        attempt: 5,
                        code: Some(last.message.clone()),
                    });
                    return Err(last);
                }
            }
            ssh.as_ref()
                .ok_or_else(|| error("SSH_TRANSPORT_FAILED"))?
                .open(&self.host, self.port)
                .await
        } else {
            let password = match self.network_secrets.proxy_password.as_ref() {
                Some(value) => Some(value.clone()),
                None => {
                    crate::storage::keychain_get(&format!("{}:proxy_password", self.profile_id))?
                }
            };
            proxy::connect(
                self.network.proxy.as_ref(),
                password.as_deref(),
                &self.host,
                self.port,
            )
            .await
        }
    }
}

#[tauri::command]
pub fn connection_host_key_confirm(
    host: tauri::State<'_, crate::Host>,
    request_id: String,
    accept: bool,
) -> Result<(), DbError> {
    host.network.answer(&request_id, accept)
}
#[tauri::command]
pub async fn connection_parse(input: String) -> Result<parse::Parsed, DbError> {
    parse::parse(&input)
}
#[tauri::command]
pub fn connection_credentials_save(profile_id: String, secrets: Secrets) -> Result<(), DbError> {
    if profile_id.trim().is_empty() || secrets.jumps.len() > 8 {
        return Err(error("INVALID_PROFILE_ID"));
    }
    for (key, value) in [
        ("ssh_password", secrets.ssh_password),
        ("key_passphrase", secrets.key_passphrase),
        ("proxy_password", secrets.proxy_password),
    ] {
        if let Some(value) = value {
            crate::storage::keychain_set(
                &format!("{profile_id}:{key}"),
                if value.is_empty() { None } else { Some(&value) },
            )?;
        }
    }
    for (i, hop) in secrets.jumps.into_iter().enumerate() {
        for (key, value) in [
            ("ssh_password", hop.ssh_password),
            ("key_passphrase", hop.key_passphrase),
        ] {
            if let Some(value) = value {
                crate::storage::keychain_set(
                    &format!("{profile_id}:jump:{i}:{key}"),
                    if value.is_empty() { None } else { Some(&value) },
                )?;
            }
        }
    }
    Ok(())
}
pub fn delete_credentials(id: &str) -> Result<(), DbError> {
    for key in ["ssh_password", "key_passphrase", "proxy_password"] {
        crate::storage::keychain_set(&format!("{id}:{key}"), None)?;
    }
    for i in 0..8 {
        for key in ["ssh_password", "key_passphrase"] {
            crate::storage::keychain_set(&format!("{id}:jump:{i}:{key}"), None)?;
        }
    }
    Ok(())
}
#[tauri::command]
pub fn iam_profiles() -> Result<iam::Profiles, DbError> {
    iam::profiles()
}

pub(crate) async fn connection_preflight(host: &crate::Host, profile: &crate::storage::Profile, progress: &Progress) -> Result<(), DbError> {
    let network = profile
        .network
        .as_ref()
        .is_some_and(|n| n.ssh.is_some() || n.proxy.is_some());
    if !network {
        let start = Instant::now();
        progress.stage("dns", "running", start, None);
        let driver = host.driver(&profile.driver_id)?;
        if let Ok(endpoint) = endpoint(&profile.fields, driver.manifest()) {
            if !matches!(
                tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::net::lookup_host((endpoint.host.as_str(), endpoint.port))
                )
                .await,
                Ok(Ok(_))
            ) {
                let e = error("DNS_FAILED");
                progress.stage("dns", "failed", start, Some(&e));
                return Err(e);
            }
            progress.stage("dns", "passed", start, None);
        } else {
            progress.stage("dns", "skipped", start, None);
        }
        progress.stage("ssh", "skipped", Instant::now(), None);
        progress.stage("tunnel", "skipped", Instant::now(), None);
    }
    Ok(())
}

#[tauri::command]
pub async fn connection_test_steps(
    host: tauri::State<'_, crate::Host>,
    profile: crate::storage::Profile,
    secret: Option<String>,
    network_secrets: Secrets,
    on_event: tauri::ipc::Channel<Event>,
) -> Result<String, DbError> {
    let progress = Progress::new(on_event);
    connection_preflight(&host, &profile, &progress).await?;
    let start = Instant::now();
    let connected = host
        .connect_progress(&profile, secret, network_secrets, progress.clone())
        .await;
    let (session, _, _tunnel) = match connected {
        Ok(connection) => connection,
        Err(e) => {
            let safe = if e
                .message
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_')
            {
                e
            } else {
                database_error(&e, !matches!(profile.auth, Auth::Password)).1
            };
            progress.failure(&safe, start);
            return Err(safe);
        }
    };
    let start = Instant::now();
    progress.stage("permissions", "running", start, None);
    let result = async {
        session.ping().await?;
        session.list_children(None).await?;
        session.server_info().await
    }
    .await;
    session.close().await;
    match result {
        Ok(info) => {
            progress.stage("permissions", "passed", start, None);
            Ok(info)
        }
        Err(_) => {
            let e = error("PERMISSION_DENIED");
            progress.stage("permissions", "failed", start, Some(&e));
            Err(e)
        }
    }
}

pub fn extract_uri_secret(
    profile: &mut crate::storage::Profile,
) -> Result<Option<String>, DbError> {
    let Some(uri) = profile.fields.get("uri") else {
        return Ok(None);
    };
    let url = url::Url::parse(uri).map_err(|_| error("INVALID_CONNECTION_URI"))?;
    if url.username().is_empty()
        && url.password().is_none()
        && !url
            .query_pairs()
            .any(|(key, _)| matches!(key.as_ref(), "password" | "sslpassword"))
    {
        return Ok(None);
    }
    let parsed = parse::parse(uri)?;
    profile.fields.extend(parsed.fields);
    Ok(parsed.secret)
}

pub fn tls_required(fields: &BTreeMap<String, String>) -> bool {
    fields
        .get("sslmode")
        .is_some_and(|v| matches!(v.as_str(), "require" | "verify-ca" | "verify-full"))
        || fields.get("tls").is_some_and(|v| v == "true")
        || fields
            .get("uri")
            .and_then(|v| url::Url::parse(v).ok())
            .is_some_and(|uri| {
                uri.scheme().ends_with("+s")
                    || uri.scheme().ends_with("+ssc")
                    || uri
                        .query_pairs()
                        .any(|(k, v)| matches!(k.as_ref(), "tls" | "ssl") && v == "true")
            })
}
pub fn database_error(e: &DbError, iam: bool) -> (&'static str, DbError) {
    let text = e.message.to_ascii_lowercase();
    if text.contains("pg_hba") {
        ("db_auth", error("DB_ACCESS_POLICY_REJECTED"))
    } else if text.contains("tls") || text.contains("ssl") || text.contains("certificate") {
        ("tls", error("TLS_FAILED"))
    } else if e.kind == dbcore::ErrorKind::Auth {
        (
            "db_auth",
            error(if iam {
                "IAM_TOKEN_REJECTED"
            } else {
                "DB_AUTH_REJECTED"
            }),
        )
    } else if e.kind == dbcore::ErrorKind::Timeout {
        ("db_auth", error("DB_CONNECT_TIMEOUT"))
    } else {
        ("db_auth", error("DB_CONNECT_FAILED"))
    }
}
