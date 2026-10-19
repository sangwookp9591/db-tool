//! Headless HTTP MCP integration: real PG and production execution/cache helpers; UI transport is a fixture.
use super::*;
use dbcore::{ConnectConfig, Driver, ExecRequest};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_pg_http_caps_pages_scope_and_revocation() {
    if std::env::var_os("AXDB_TEST_PG").is_none() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().to_path_buf();
    permissions::migrate_local(&config, "l2-http-test").unwrap();
    let client = permissions::authenticate(&config, "l2-http-test")
        .unwrap()
        .unwrap();
    let mut other = client.clone();
    other.id = "other".into();
    other
        .connections
        .insert("p".into(), permissions::Permission::Read);
    other.token_hash = permissions::hash(&format!("{}:l2-http-other", other.token_salt));
    storage::write_private(
        &config.join("agent_clients.json"),
        &serde_json::to_vec(&vec![client.clone(), other.clone()]).unwrap(),
    )
    .unwrap();
    let host = Arc::new(Host::new(crate::storage::Paths {
        config: config.clone(),
        data: config.clone(),
        log: config.clone(),
    }));
    host.registry
        .register(Arc::new(driver_postgres::PostgresDriver::new()));
    let profile = storage::Profile {
        id: "p".into(),
        name: "isolated test".into(),
        driver_id: "postgres".into(),
        fields: BTreeMap::new(),
        save_secret: false,
        network: None,
        auth: Default::default(),
        access_mode: Default::default(),
    };
    storage::write_private(
        &config.join("profiles.json"),
        &serde_json::to_vec(&vec![profile.clone()]).unwrap(),
    )
    .unwrap();
    let cfg = ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "postgres".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testpw".into()),
        connect_timeout_ms: 3000,
    };
    let session = driver_postgres::PostgresDriver::new()
        .connect(&cfg)
        .await
        .unwrap();
    host.sessions.lock().unwrap().insert(
        "s".into(),
        Arc::new(crate::SessionRecord {
            session: Arc::from(session),
            profile,
            secret: cfg.secret,
            _tunnel: None,
            dead: AtomicBool::new(false),
        }),
    );
    let (send, mut receive) = tokio::sync::mpsc::channel::<BridgeRequest>(8);
    let shared = Arc::new(Shared {
        app: None,
        config: config.clone(),
        permits: tokio::sync::Semaphore::new(64),
        recycling: std::sync::atomic::AtomicBool::new(false),
        token: String::new(),
        ready: watch::channel(true).1,
        pending: Mutex::new(HashMap::new()),
        forward: Arc::new(move |r| {
            send.try_send(r)
                .map_err(|_| DbError::internal("test UI queue closed"))
        }),
        cancel: Arc::new(|_| {}),
    });
    let fixture = shared.clone();
    let backend = host.clone();
    let local = client.clone();
    let worker = tokio::spawn(async move {
        while let Some(r) = receive.recv().await {
            let args = &r.params["arguments"];
            // The HTTP middleware authenticates the request; the fixture selects the same client by an explicit test-only marker.
            let client = if args["result_id"]
                .as_str()
                .is_some_and(|s| s.starts_with("other:"))
            {
                &other
            } else {
                &local
            };
            let mut args = args.clone();
            if let Some(id) = args["result_id"]
                .as_str()
                .and_then(|s| s.strip_prefix("other:"))
            {
                let id = id.to_owned();
                args["result_id"] = json!(id);
            }
            let result = if r.params["name"] == "run_query" {
                let req: ExecRequest = serde_json::from_value(
                    json!({"text":args["text"],"row_limit":1000,"read_only":true}),
                )
                .unwrap();
                let results = crate::execute_impl(
                    &backend,
                    "s".into(),
                    uuid::Uuid::new_v4().to_string(),
                    req,
                    None,
                    storage::HistorySource::Agent,
                )
                .await
                .unwrap();
                crate::agent_dispatch::limit_result(
                    &backend,
                    client,
                    "run_query",
                    &args,
                    Some("p"),
                    json!({"results":results}),
                )
            } else {
                crate::agent_dispatch::read_result_page(&backend, client, &args)
            };
            if let Some(reply) = fixture.pending.lock().unwrap().remove(&r.request_id) {
                let _ = reply.send(result);
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(shared);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let http = reqwest::Client::new();
    async fn call(
        http: &reqwest::Client,
        address: std::net::SocketAddr,
        token: &str,
        name: &str,
        args: Value,
    ) -> Value {
        let value:Value=http.post(format!("http://{address}/mcp")).bearer_auth(token).json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).send().await.unwrap().json().await.unwrap();
        let result = value["result"]["structuredContent"].clone();
        assert!(results::size(&result, results::DEFAULT_BYTES).is_some());
        assert_eq!(
            serde_json::from_str::<Value>(value["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap(),
            result
        );
        result
    }
    let first = call(
        &http,
        address,
        "l2-http-test",
        "run_query",
        json!({"session_id":"s","text":"SELECT i FROM generate_series(1,500) i"}),
    )
    .await;
    assert_eq!(
        first["results"][0]["table"]["rows"]
            .as_array()
            .unwrap()
            .len(),
        200
    );
    assert_eq!(first["truncated"], true);
    let id = first["result_id"].as_str().unwrap();
    let second = call(
        &http,
        address,
        "l2-http-test",
        "get_result_page",
        json!({"session_id":"s","result_id":id,"offset":200}),
    )
    .await;
    assert_eq!(second["results"][0]["table"]["rows"][0][0]["v"], "201");
    let denied = call(
        &http,
        address,
        "l2-http-other",
        "get_result_page",
        json!({"session_id":"s","result_id":format!("other:{id}")}),
    )
    .await;
    assert!(denied.get("error").is_some());
    let huge = call(
        &http,
        address,
        "l2-http-test",
        "run_query",
        json!({"session_id":"s","text":"SELECT repeat('한',120000)"}),
    )
    .await;
    assert_eq!(huge["truncated"], true);
    assert!(huge["omitted_items"].is_array());
    let huge_id = huge["result_id"].as_str().unwrap();
    let mut text = String::new();
    let mut offset = 0;
    loop {
        let page = call(
            &http,
            address,
            "l2-http-test",
            "get_result_page",
            json!({"session_id":"s","result_id":huge_id,"byte_offset":offset}),
        )
        .await;
        text.push_str(page["data"].as_str().unwrap());
        if let Some(next) = page["next_byte_offset"].as_u64() {
            offset = next;
        } else {
            break;
        }
    }
    let raw: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        raw["results"][0]["table"]["rows"][0][0]["v"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        120000
    );
    let mut revoked = client;
    revoked.allowed_tools.retain(|t| t != "get_result_page");
    storage::write_private(
        &config.join("agent_clients.json"),
        &serde_json::to_vec(&vec![revoked]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        call(
            &http,
            address,
            "l2-http-test",
            "get_result_page",
            json!({"session_id":"s","result_id":id})
        )
        .await["error"]["kind"],
        "Forbidden"
    );
    let audit = std::fs::read_to_string(config.join("agent-audit.jsonl")).unwrap();
    assert!(audit.contains("\"truncated\":true"));
    assert!(!audit.contains("한"));
    assert!(!audit.contains("testpw"));
    host.session("s").unwrap().session.close().await;
    server.abort();
    worker.abort();
}
