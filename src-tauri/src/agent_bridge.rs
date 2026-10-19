#[path = "agent_results.rs"]
pub mod results;
use crate::{storage, permissions, DbError, Host};
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    Extension,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    time::Duration,
};
use tauri::{Emitter, Manager};
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;

type Forward = Arc<dyn Fn(BridgeRequest) -> Result<(), DbError> + Send + Sync>;
#[derive(Clone, Serialize)]
struct BridgeRequest {
    request_id: String,
    method: String,
    params: Value,
    timeout_ms: u64,
}
pub(crate) struct Shared {
    pub(crate) app: Option<tauri::AppHandle>,
    config: std::path::PathBuf,
    permits: tokio::sync::Semaphore,
    pub(crate) recycling: AtomicBool,
    token: String,
    ready: watch::Receiver<bool>,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    forward: Forward,
    cancel: Arc<dyn Fn(String) + Send + Sync>,
}
impl Shared {
    pub(crate) async fn prepared(&self, timeout: Duration) -> bool {
        let mut ready=self.ready.clone();
        tokio::time::timeout(timeout, ready.wait_for(|value| *value)).await.is_ok_and(|value| value.is_ok())
    }
}
struct Running {
    shared: Arc<Shared>,
    stop: CancellationToken,
    discovery: Value,
}
pub struct Bridge {
    running: tokio::sync::Mutex<Option<Running>>,
    ready: watch::Sender<bool>,
}
impl Default for Bridge {
    fn default() -> Self {
        Self {
            running: Default::default(),
            ready: watch::channel(false).0,
        }
    }
}
impl Bridge {
    pub(crate) async fn pause_for_recycle(&self) -> bool {
        let running = self.running.lock().await;
        if let Some(running) = running.as_ref() {
            running.shared.recycling.store(true, Ordering::SeqCst);
            if running.shared.permits.available_permits() != 64 || !running.shared.pending.lock().unwrap().is_empty() {
                running.shared.recycling.store(false, Ordering::SeqCst);
                return false;
            }
        }
        self.ready.send_replace(false);
        true
    }
    pub(crate) async fn resume_after_recycle(&self, config: &std::path::Path) -> Result<(), DbError> {
        self.mark_ready(config).await
    }

    pub(crate) async fn client_config(&self) -> Result<Value, DbError> {
        let running = self.running.lock().await;
        let running = running.as_ref().ok_or_else(|| DbError::query("먼저 MCP 브리지를 켜세요"))?;
        let token = running.discovery["token"].as_str().ok_or_else(|| DbError::internal("Bridge token unavailable"))?;
        if permissions::authenticate(&running.shared.config, token)?.is_none() {
            return Err(DbError::query("로컬 토큰이 만료되었거나 폐기되었습니다. 클라이언트 패널에서 토큰을 발급하세요"));
        }
        Ok(json!({"mcpServers":{"axdb":{"type":"http","url":running.discovery["url"],"headers":{"Authorization":format!("Bearer {token}")}}}}))
    }
    pub async fn update_local_token(&self,config:&std::path::Path,raw:&str)->Result<(),DbError> {
        if let Some(running)=self.running.lock().await.as_mut(){running.discovery["token"]=json!(raw);if *self.ready.borrow(){publish(config,&running.discovery)?;}}Ok(())
    }
    pub fn loading(&self, config: &std::path::Path) {
        self.ready.send_replace(false);
        let _ = std::fs::remove_file(config.join("agent-bridge.json"));
    }
    async fn mark_ready(&self, config: &std::path::Path) -> Result<(), DbError> {
        let running = self.running.lock().await;
        self.ready.send_replace(true);
        if let Some(running) = running.as_ref() {
            running.shared.recycling.store(false, Ordering::SeqCst);
            publish(config, &running.discovery)?;
        }
        Ok(())
    }
    pub fn configured(config: &std::path::Path) -> Result<bool, DbError> {
        let settings = read_settings(config)?;
        Ok(std::env::var("AXDB_AGENT_BRIDGE").as_deref() == Ok("1")
            || settings["agent_bridge"].as_bool().unwrap_or(false))
    }
    pub async fn stop(&self, config: &std::path::Path) {
        if let Some(running) = self.running.lock().await.take() {
            if let Some(app) = &running.shared.app { app.state::<Host>().agent_results.clear(); }
            running.stop.cancel();
            let requests = std::mem::take(&mut *running.shared.pending.lock().unwrap());
            for (id, reply) in requests {
                (running.shared.cancel)(id);
                let _ = reply.send(unavailable("Agent bridge stopped"));
            }
        }
        let _ = std::fs::remove_file(config.join("agent-bridge.json"));
    }
    pub async fn start(
        &self,
        app: tauri::AppHandle,
        config: &std::path::Path,
    ) -> Result<(), DbError> {
        let mut running = self.running.lock().await;
        if running.is_some() {
            return Ok(());
        }
        let send = app.clone();
        let forward: Forward = Arc::new(move |request| {
            send.emit_to("main", "agent-bridge-request", request)
                .map_err(|_| DbError::internal("Cannot reach app webview"))
        });
        let cancel_app = app.clone();
        let cancel = Arc::new(move |id| {
            let _ = cancel_app.emit_to("main", "agent-bridge-cancel", json!({"request_id": id}));
        });
        let shared = Arc::new(Shared {
            app: Some(app),
            config: config.to_owned(),
            permits: tokio::sync::Semaphore::new(64),
            recycling: AtomicBool::new(false),
            token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            pending: Mutex::new(HashMap::new()),
            ready: self.ready.subscribe(),
            forward,
            cancel,
        });
        {
            let host=shared.app.as_ref().unwrap().state::<Host>();
            let _guard=host.agent_storage.lock().map_err(|_|DbError::internal("Agent storage lock unavailable"))?;
            permissions::migrate_local(config, &shared.token)?;
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| DbError::internal("Cannot bind agent bridge"))?;
        let address = listener
            .local_addr()
            .map_err(|_| DbError::internal("Cannot read agent bridge address"))?;
        let discovery = json!({"url": format!("http://{address}/mcp"), "token": shared.token});
        if *self.ready.borrow() {
            publish(config, &discovery)?;
        } else {
            let _ = std::fs::remove_file(config.join("agent-bridge.json"));
        }
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let router = router(shared.clone());
        tauri::async_runtime::spawn(async move {
            if axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .is_err()
            {
                tracing::error!("Agent bridge stopped unexpectedly");
            }
        });
        *running = Some(Running {
            shared,
            stop,
            discovery,
        });
        Ok(())
    }
}
fn publish(config: &std::path::Path, discovery: &Value) -> Result<(), DbError> {
    storage::write_private(
        &config.join("agent-bridge.json"),
        &serde_json::to_vec(discovery)
            .map_err(|_| DbError::internal("Cannot encode agent bridge configuration"))?,
    )
}
#[tauri::command]
pub async fn agent_bridge_ready(
    bridge: tauri::State<'_, Bridge>,
    host: tauri::State<'_, Host>,
) -> Result<(), DbError> {
    bridge.mark_ready(&host.paths.config).await
}
pub(crate) fn read_settings(config: &std::path::Path) -> Result<Value, DbError> {
    match std::fs::read(config.join("settings.json")) {
        Ok(data) => {
            let value: Value = serde_json::from_slice(&data)
                .map_err(|_| DbError::internal("Invalid settings file"))?;
            if !value.is_object() {
                return Err(DbError::internal("Invalid settings file"));
            }
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err(DbError::internal("Cannot read settings file")),
    }
}
#[tauri::command]
pub async fn agent_bridge_set(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, Bridge>,
    host: tauri::State<'_, Host>,
    enabled: bool,
) -> Result<(), DbError> {
    let _guard = host.storage.lock().await;
    let mut settings = read_settings(&host.paths.config)?;
    if enabled {
        bridge.start(app, &host.paths.config).await?;
    } else {
        bridge.stop(&host.paths.config).await;
    }
    settings["agent_bridge"] = json!(enabled);
    storage::write_private(
        &host.paths.config.join("settings.json"),
        &serde_json::to_vec(&settings).map_err(|_| DbError::internal("Cannot encode settings"))?,
    )
}
#[tauri::command]
pub async fn agent_bridge_status(bridge: tauri::State<'_, Bridge>) -> Result<bool, DbError> {
    Ok(bridge.running.lock().await.is_some())
}
#[tauri::command]
pub async fn agent_bridge_reply(
    bridge: tauri::State<'_, Bridge>,
    request_id: String,
    result: Value,
) -> Result<(), DbError> {
    if let Some(running) = bridge.running.lock().await.as_ref() {
        if let Some(reply) = running
            .shared
            .pending
            .lock()
            .map_err(|_| DbError::internal("Bridge lock unavailable"))?
            .remove(&request_id)
        {
            let _ = reply.send(result);
        }
    }
    Ok(())
}
fn unavailable(message: &str) -> Value {
    json!({"error":{"kind":"Unavailable", "message":message}})
}
fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/mcp", post(rpc))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn_with_state(shared.clone(), authorize))
        .with_state(shared)
}
async fn authorize(State(shared): State<Arc<Shared>>, mut request: Request, next: Next) -> Response {
    if request.headers().contains_key(header::ORIGIN) { return StatusCode::FORBIDDEN.into_response(); }
    let raw=request.headers().get(header::AUTHORIZATION).and_then(|h|h.to_str().ok()).and_then(|s|s.strip_prefix("Bearer "));
    let client=match raw.map(|raw|permissions::authenticate(&shared.config,raw)) {
        Some(Ok(Some(client)))=>client,
        Some(Err(_))=>return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        _=>return StatusCode::UNAUTHORIZED.into_response(),
    };
    request.extensions_mut().insert(client);
    if let Some(version) = request.headers().get("mcp-protocol-version") {
        if !matches!(
            version.to_str(),
            Ok("2025-11-25" | "2025-06-18" | "2025-03-26")
        ) {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }
    next.run(request).await
}
async fn rpc(
    State(shared): State<Arc<Shared>>,
    Extension(client): Extension<permissions::AgentClient>,
    body: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let request = match body { Ok(Json(value)) => value, Err(_) => return Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON request"}})).into_response() };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request["jsonrpc"] != "2.0"
        || !request["method"].is_string()
        || (!id.is_null() && !id.is_string() && !id.is_number())
    {
        return Json(
            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid request"}}),
        )
        .into_response();
    }
    let method = request["method"].as_str().unwrap();
    if request.get("id").is_none() {
        return if method == "notifications/initialized" {
            StatusCode::ACCEPTED.into_response()
        } else {
            StatusCode::BAD_REQUEST.into_response()
        };
    }
    let result = match method {
        "initialize" => {
            let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
            let version = if matches!(requested, "2025-03-26" | "2025-06-18" | "2025-11-25") {
                requested
            } else {
                "2025-11-25"
            };
            json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"axdb","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" | "tools/call" => {
            let _permit = match shared.permits.try_acquire() { Ok(p)=>p,Err(_)=>return Json(json!({"jsonrpc":"2.0","id":id,"result":{"error":{"kind":"Busy","message":"Too many pending agent requests"}}})).into_response() };
            if shared.recycling.load(Ordering::SeqCst) {
                if method == "tools/list" {
                    return Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Workbench is restoring; retry this request","data":{"retryable":true}}})).into_response();
                }
                let error = json!({"error":{"kind":"Busy","message":"Workbench is restoring; retry this request","retryable":true}});
                let result = if method == "tools/call" { json!({"content":[{"type":"text","text":error.to_string()}],"structuredContent":error,"isError":true}) } else { error };
                return Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response();
            }
            let value = crate::agent_dispatch::dispatch(&shared,&client,method,request.get("params").cloned().unwrap_or(json!({}))).await;
            if method == "tools/call" {
                json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value.get("error").is_some()})
            } else {
                value
            }
        }
        _ => return Json(
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
        )
        .into_response(),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}
pub(crate) async fn forward(shared: &Shared, method: &str, params: Value) -> Value {
    forward_with_ready_timeout(shared, method, params, Duration::from_secs(10)).await
}
async fn forward_with_ready_timeout(
    shared: &Shared,
    method: &str,
    params: Value,
    ready_timeout: Duration,
) -> Value {
    let timeout_ms = params
        .get("arguments")
        .and_then(|a| a.get("timeout_ms"))
        .and_then(Value::as_u64)
        .map(|ms| ms.clamp(1, 3_600_000) + 30_000)
        .unwrap_or(60_000);
    let request_id = uuid::Uuid::new_v4().to_string();
    let (send, mut receive) = oneshot::channel();
    {
        let mut pending = shared.pending.lock().unwrap();
        if pending.len() >= 64 {
            return json!({"error":{"kind":"Busy","message":"Too many pending agent requests"}});
        }
        pending.insert(request_id.clone(), send);
    }
    let mut ready = shared.ready.clone();
    let prepared = tokio::select! {
        ready = tokio::time::timeout(ready_timeout, ready.wait_for(|value| *value)) => ready.is_ok_and(|value| value.is_ok()),
        reply = &mut receive => {
            shared.pending.lock().unwrap().remove(&request_id);
            return reply.unwrap_or_else(|_| unavailable("Agent bridge stopped"));
        }
    };
    if !prepared {
        shared.pending.lock().unwrap().remove(&request_id);
        return unavailable("App webview is not ready");
    }
    let sent = (shared.forward)(BridgeRequest {
        request_id: request_id.clone(),
        method: method.into(),
        params,
        timeout_ms,
    });
    let result = if sent.is_ok() {
        match tokio::time::timeout(Duration::from_millis(timeout_ms), receive).await {
            Ok(Ok(value)) => value,
            _ => {
                (shared.cancel)(request_id.clone());
                unavailable("App webview response timed out")
            }
        }
    } else {
        unavailable("App webview is unavailable")
    };
    shared.pending.lock().unwrap().remove(&request_id);
    result
}
// Remove discovery data even when the window closes via the normal application exit path.
pub fn cleanup(app: &tauri::AppHandle) {
    let host = app.state::<Host>();
    let bridge = app.state::<Bridge>();
    tauri::async_runtime::block_on(bridge.stop(&host.paths.config));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn test_config(raw:&str)->std::path::PathBuf {
        let dir=std::env::temp_dir().join(format!("axdb-a1-auth-{}",uuid::Uuid::new_v4()));std::fs::create_dir_all(&dir).unwrap();permissions::migrate_local(&dir,raw).unwrap();dir
    }
    async fn request(address: std::net::SocketAddr, headers: &str, body: &str) -> String {
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let text = format!("POST /mcp HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}", body.len());
        socket.write_all(text.as_bytes()).await.unwrap();
        let mut reply = String::new();
        socket.read_to_string(&mut reply).await.unwrap();
        reply
    }
    #[tokio::test]
    async fn http_auth_origin_protocol_and_webview_forwarding() {
        let (sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            app: None, config: test_config("test-token"), permits: tokio::sync::Semaphore::new(64),
            recycling: AtomicBool::new(false),
            token: "test-token".into(),
            ready: watch::channel(true).1,
            pending: Mutex::new(HashMap::new()),
            forward: Arc::new(move |r| {
                sender
                    .send(r)
                    .map_err(|_| DbError::internal("test channel closed"))
            }),
            cancel: Arc::new(|_| {}),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = router(shared.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let payload = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#;
        assert!(request(address, "", payload)
            .await
            .starts_with("HTTP/1.1 401"));
        assert!(request(address, "Authorization: Bearer wrong\r\n", payload)
            .await
            .starts_with("HTTP/1.1 401"));
        let auth = "Authorization: Bearer test-token\r\n";
        assert!(request(
            address,
            &format!("{auth}Origin: https://evil.example\r\n"),
            payload
        )
        .await
        .starts_with("HTTP/1.1 403"));
        shared.recycling.store(true, Ordering::SeqCst);
        let paused = request(address, auth, r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"run_query"}}"#).await;
        assert!(paused.contains("retryable") && paused.contains("Busy") && paused.contains("structuredContent"));
        let listing = request(address, auth, r#"{"jsonrpc":"2.0","id":8,"method":"tools/list"}"#).await;
        assert!(listing.contains("\"code\":-32000") && listing.contains("retryable") && !listing.contains("\"result\":"));
        assert!(requests.try_recv().is_err(), "A paused request must never be forwarded or executed");
        shared.recycling.store(false, Ordering::SeqCst);
        let response = request(address, auth, payload).await;
        assert!(response.contains("application/json"));
        assert!(response.contains("2025-11-25"));
        assert!(request(
            address,
            auth,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .await
        .starts_with("HTTP/1.1 202"));
        assert!(request(
            address,
            &format!("{auth}MCP-Protocol-Version: wrong\r\n"),
            payload
        )
        .await
        .starts_with("HTTP/1.1 400"));
        let pending = request(
            address,
            auth,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        );
        let respond = async {
            let event = requests.recv().await.unwrap();
            assert_eq!(event.method, "tools/list");
            let reply = shared
                .pending
                .lock()
                .unwrap()
                .remove(&event.request_id)
                .unwrap();
            reply.send(json!({"tools":[{"name":"fixture"}]})).unwrap();
        };
        let (response, _) = tokio::join!(pending, respond);
        assert!(response.contains("fixture"));
        assert!(shared.pending.lock().unwrap().is_empty());
        server.abort();
    }
    fn waiting_shared(
        ready: watch::Receiver<bool>,
    ) -> (
        Arc<Shared>,
        tokio::sync::mpsc::UnboundedReceiver<BridgeRequest>,
    ) {
        let (sender, requests) = tokio::sync::mpsc::unbounded_channel();
        (
            Arc::new(Shared {
                app: None, config: test_config("test"), permits: tokio::sync::Semaphore::new(64),
            recycling: AtomicBool::new(false),
                token: "test".into(),
                ready,
                pending: Mutex::new(HashMap::new()),
                forward: Arc::new(move |r| sender.send(r).map_err(|_| DbError::internal("closed"))),
                cancel: Arc::new(|_| {}),
            }),
            requests,
        )
    }
    #[tokio::test]
    async fn queues_until_ready_and_waits_again_after_reload() {
        let (ready, receive) = watch::channel(false);
        let (shared, mut requests) = waiting_shared(receive);
        for _ in 0..2 {
            let host = shared.clone();
            let task = tokio::spawn(async move { forward(&host, "tools/list", json!({})).await });
            while shared.pending.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
            assert!(requests.try_recv().is_err());
            ready.send_replace(true);
            assert!(shared.prepared(Duration::from_millis(5)).await);
            let event = requests.recv().await.unwrap();
            shared
                .pending
                .lock()
                .unwrap()
                .remove(&event.request_id)
                .unwrap()
                .send(json!({"tools":[]}))
                .unwrap();
            assert_eq!(task.await.unwrap(), json!({"tools":[]}));
            assert!(shared.pending.lock().unwrap().is_empty());
            ready.send_replace(false);
        }
    }
    #[tokio::test]
    async fn missing_ready_is_unavailable_and_does_not_forward() {
        let (_ready, receive) = watch::channel(false);
        let (shared, mut requests) = waiting_shared(receive);
        let value =
            forward_with_ready_timeout(&shared, "tools/list", json!({}), Duration::from_millis(5))
                .await;
        assert!(!shared.prepared(Duration::from_millis(1)).await);
        assert_eq!(value["error"]["kind"], "Unavailable");
        assert!(shared.pending.lock().unwrap().is_empty());
        assert!(requests.try_recv().is_err());
    }
    #[tokio::test]
    async fn transport_failure_is_unavailable_but_explicit_denial_is_preserved() {
        let (_ready, receive) = watch::channel(true);
        let (shared, mut requests) = waiting_shared(receive);
        let host = shared.clone();
        let task = tokio::spawn(async move { forward(&host, "tools/call", json!({})).await });
        let event = requests.recv().await.unwrap();
        shared
            .pending
            .lock()
            .unwrap()
            .remove(&event.request_id)
            .unwrap()
            .send(json!({"error":{"kind":"UserDenied"}}))
            .unwrap();
        assert_eq!(task.await.unwrap()["error"]["kind"], "UserDenied");
        let host = shared.clone();
        let task = tokio::spawn(async move { forward(&host, "tools/call", json!({})).await });
        let event = requests.recv().await.unwrap();
        shared.pending.lock().unwrap().remove(&event.request_id);
        assert_eq!(task.await.unwrap()["error"]["kind"], "Unavailable");
        drop(requests);
        assert_eq!(
            forward(&shared, "tools/list", json!({})).await["error"]["kind"],
            "Unavailable"
        );
        assert!(shared.pending.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn discovery_is_published_on_ready_and_removed_on_reload_and_stop() {
        let directory =
            std::env::temp_dir().join(format!("axdb-bridge-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let bridge = Bridge::default();
        let (shared, _requests) = waiting_shared(bridge.ready.subscribe());
        *bridge.running.lock().await = Some(Running {
            shared,
            stop: CancellationToken::new(),
            discovery: json!({"url":"http://127.0.0.1:1/mcp","token":"test"}),
        });
        let file = directory.join("agent-bridge.json");
        assert!(!file.exists());
        bridge.mark_ready(&directory).await.unwrap();
        assert!(file.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        bridge.loading(&directory);
        assert!(!*bridge.ready.borrow());
        assert!(!file.exists());
        bridge.mark_ready(&directory).await.unwrap();
        assert!(file.exists());
        bridge.stop(&directory).await;
        assert!(!file.exists());
        std::fs::remove_dir(directory).unwrap();
    }
    #[tokio::test]
    async fn stopping_releases_requests_waiting_for_ready() {
        let directory =
            std::env::temp_dir().join(format!("axdb-bridge-{}", uuid::Uuid::new_v4()));
        let bridge = Bridge::default();
        let (shared, mut requests) = waiting_shared(bridge.ready.subscribe());
        *bridge.running.lock().await = Some(Running {
            shared: shared.clone(),
            stop: CancellationToken::new(),
            discovery: json!({}),
        });
        let host = shared.clone();
        let task = tokio::spawn(async move { forward(&host, "tools/list", json!({})).await });
        while shared.pending.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        bridge.stop(&directory).await;
        assert_eq!(task.await.unwrap()["error"]["kind"], "Unavailable");
        assert!(requests.try_recv().is_err());
        assert!(shared.pending.lock().unwrap().is_empty());
    }
    #[test]
    fn old_contract_defaults_remain_compatible() {
        let history: storage::HistoryEntry = serde_json::from_value(json!({"timestamp":"now","profile_name":"test","text":"SELECT 1","elapsed_ms":1,"row_count":1,"error":null})).unwrap();
        assert_eq!(serde_json::to_value(history).unwrap()["source"], "ui");
        let request: dbcore::ExecRequest =
            serde_json::from_value(json!({"text":"SELECT 1"})).unwrap();
        assert!(!request.read_only);
    }
}

#[cfg(test)]
#[path = "agent_results_e2e.rs"]
mod results_e2e;
