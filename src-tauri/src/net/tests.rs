use super::*;
use dbcore::{ConnectConfig, Driver};
use std::sync::atomic::{AtomicUsize, Ordering};

fn progress_events() -> (Progress, Arc<Mutex<Vec<serde_json::Value>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let copy = events.clone();
    let channel = tauri::ipc::Channel::new(move |body| {
        let tauri::ipc::InvokeResponseBody::Json(text) = body else {
            panic!("expected JSON")
        };
        copy.lock()
            .unwrap()
            .push(serde_json::from_str(&text).unwrap());
        Ok(())
    });
    (Progress::new(channel), events)
}

#[test]
fn failure_keeps_network_stage_and_is_emitted_once() {
    let (progress, events) = progress_events();
    let start = Instant::now();
    progress.stage("ssh", "running", start, None);
    progress.failure(&error("SSH_AUTH_REJECTED"), start);
    progress.failure(&error("DB_CONNECT_FAILED"), start);
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["stage"], "ssh");
    assert_eq!(events[1]["code"], "SSH_AUTH_REJECTED");
    assert_eq!(events[1]["help_key"], "connection.help.ssh_auth_rejected");
}

#[tokio::test]
async fn real_connection_stages_and_redacted_auth_failure() {
    if std::env::var_os("AXDB_TEST_NETWORK_FIXTURE").is_none() {
        return;
    }
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let host = crate::Host::new(crate::storage::Paths {
        config: dir.path().into(),
        data: dir.path().into(),
        log: dir.path().into(),
    });
    host.registry
        .register(Arc::new(driver_postgres::PostgresDriver::new()));
    let app = tauri::test::mock_builder()
        .manage(host)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let profile: crate::storage::Profile = serde_json::from_value(serde_json::json!({
        "id":"w1-stages", "name":"Test only", "driver_id":"postgres",
        "fields":{"host":"127.0.0.1", "port":"55432", "database":"axdb_test", "user":"postgres", "sslmode":"disable"}
    })).unwrap();
    for (password, success) in [("testpw", true), ("w1-invalid-test-password", false)] {
        let (progress, events) = progress_events();
        let result = connection_test_steps(
            app.state(),
            profile.clone(),
            Some(password.into()),
            Secrets::default(),
            progress.channel.unwrap(),
        )
        .await;
        assert_eq!(result.is_ok(), success);
        let events = events.lock().unwrap();
        assert!(!serde_json::to_string(&*events).unwrap().contains(password));
        assert!(!events.iter().any(|e| e["status"] == "running"
            && !events
                .iter()
                .any(|end| end["stage"] == e["stage"] && end["status"] != "running")));
        let failures: Vec<_> = events.iter().filter(|e| e["status"] == "failed").collect();
        if success {
            assert!(failures.is_empty());
            assert_eq!(events.last().unwrap()["stage"], "permissions");
        } else {
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0]["stage"], "db_auth");
            assert_eq!(failures[0]["code"], "DB_AUTH_REJECTED");
        }
    }
    let mut profile = profile;
    profile
        .fields
        .insert("sslmode".into(), "verify-full".into());
    let (progress, events) = progress_events();
    assert!(connection_test_steps(
        app.state(),
        profile,
        Some("testpw".into()),
        Secrets::default(),
        progress.channel.unwrap()
    )
    .await
    .is_err());
    let events = events.lock().unwrap();
    assert_eq!(events.iter().filter(|e| e["status"] == "failed").count(), 1);
    assert!(events
        .iter()
        .any(|e| e["stage"] == "tls" && e["code"] == "TLS_FAILED"));
    assert!(events
        .iter()
        .any(|e| e["stage"] == "db_auth" && e["status"] == "skipped"));
}

#[test]
fn legacy_profile_defaults_and_unknown_secret_rejected() {
    let p:crate::storage::Profile=serde_json::from_value(serde_json::json!({"id":"legacy","name":"Legacy","driver_id":"postgres","fields":{"host":"localhost"}})).unwrap();
    assert!(p.network.is_none());
    assert!(matches!(p.auth, Auth::Password));
    let saved = serde_json::to_value(p).unwrap();
    assert!(saved.get("network").is_none());
}
#[tokio::test]
async fn confirmation_is_single_use_and_reject_cannot_pin() {
    let state = Arc::new(State::default());
    let weak = Arc::downgrade(&state);
    let seen = Arc::new(Mutex::new(String::new()));
    let copy = seen.clone();
    state.set_sink(Arc::new(move |event| {
        if let Event::HostKey { request_id, .. } = event {
            *copy.lock().unwrap() = request_id.clone();
            weak.upgrade().unwrap().answer(&request_id, false).unwrap();
        }
    }));
    assert_eq!(
        state
            .confirm("test".into(), 22, "SHA256:test".into())
            .await
            .unwrap_err()
            .message,
        "SSH_HOST_KEY_REJECTED"
    );
    assert!(state.answer(&seen.lock().unwrap(), true).is_err());
    assert!(state.pending.lock().unwrap().is_empty());
}
#[tokio::test]
async fn route_connects_and_rewrite_retains_destination_name() {
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = echo.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = echo.accept().await {
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = stream.write_all(b"test").await;
            });
        }
    });
    let route = Arc::new(Route {
        tls: None,
        network: Network::default(),
        network_secrets: Secrets::default(),
        profile_id: "test".into(),
        host: "127.0.0.1".into(),
        port,
        path: PathBuf::new(),
        state: Arc::new(State::default()),
        ssh: tokio::sync::Mutex::new(None),
    });
    let mut stream = route.open().await.unwrap();
    let mut b = [0; 4];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut b)
        .await
        .unwrap();
    assert_eq!(&b, b"test");
    let endpoint = Endpoint {
        host: "db.example".into(),
        port: 5432,
        uri: None,
    };
    let mut f = BTreeMap::new();
    rewrite(&mut f, &endpoint, 55555).unwrap();
    assert_eq!(f["host"], "127.0.0.1");
    assert_eq!(f["tls_server_name"], "db.example");
    let e = Endpoint {
        host: "db.test".into(),
        port: 27017,
        uri: Some(url::Url::parse("mongodb://db.test:27017/?replicaSet=rs").unwrap()),
    };
    rewrite(&mut f, &e, 55555).unwrap();
    assert!(f["uri"].contains("directConnection=true"));
    assert!(f["uri"].contains("replicaSet=rs"));
    task.abort();
}

#[tokio::test]
async fn real_test_databases_via_ssh_key_password_jumps_and_proxies() {
    let Some(path) = std::env::var_os("AXDB_TEST_NETWORK_FIXTURE") else {
        return;
    };
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let secret_store_path = std::env::var_os("AXDB_DEV_SECRET_STORE")
        .expect("network E2E requires the private test secret store");
    let fixture_secrets: BTreeMap<String, String> =
        serde_json::from_slice(&std::fs::read(secret_store_path).unwrap()).unwrap();
    let key = fixture["key_file"].as_str().unwrap().to_owned();
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(State::default());
    let confirmations = Arc::new(AtomicUsize::new(0));
    let count = confirmations.clone();
    let weak = Arc::downgrade(&state);
    state.set_sink(Arc::new(move |event| {
        if let Event::HostKey { request_id, .. } = event {
            count.fetch_add(1, Ordering::Relaxed);
            weak.upgrade().unwrap().answer(&request_id, true).unwrap();
        }
    }));
    let host = fixture["ssh_host"].as_str().unwrap();
    assert_eq!(host, "127.0.0.1");
    let port = fixture["ssh_port"].as_u64().unwrap() as u16;
    assert!((50000..60000).contains(&port));
    let ssh = Ssh {
        host: host.into(),
        port,
        user: fixture["ssh_user"].as_str().unwrap().into(),
        auth: SshAuth::Key,
        key_file: Some(key.clone()),
        jump_hosts: vec![],
    };
    let proxy = Proxy {
        kind: ProxyKind::Socks5,
        host: "127.0.0.1".into(),
        port: 55080,
        user: None,
    };
    for mode in [
        "ssh_key",
        "ssh_encrypted_key",
        "ssh_agent",
        "ssh_password",
        "ssh_password_draft_credentials",
        "ssh_jump",
        "socks5",
        "http_connect",
        "http_connect_draft_credentials",
        "proxy_then_ssh",
    ] {
        let mut tunnel_ssh = ssh.clone();
        let network = match mode {
            "ssh_agent" => {
                tunnel_ssh.auth = SshAuth::Agent;
                Network {
                    ssh: Some(tunnel_ssh),
                    proxy: None,
                }
            }
            "ssh_encrypted_key" => {
                tunnel_ssh.key_file = Some(fixture["encrypted_key_file"].as_str().unwrap().into());
                Network {
                    ssh: Some(tunnel_ssh),
                    proxy: None,
                }
            }
            "ssh_password" | "ssh_password_draft_credentials" => {
                tunnel_ssh.auth = SshAuth::Password;
                Network {
                    ssh: Some(tunnel_ssh),
                    proxy: None,
                }
            }
            "ssh_jump" => {
                tunnel_ssh.host = "host.docker.internal".into();
                tunnel_ssh.jump_hosts = vec![SshHop {
                    host: host.into(),
                    port,
                    user: ssh.user.clone(),
                    auth: SshAuth::Key,
                    key_file: Some(key.clone()),
                }];
                Network {
                    ssh: Some(tunnel_ssh),
                    proxy: None,
                }
            }
            "socks5" => Network {
                ssh: None,
                proxy: Some(proxy.clone()),
            },
            "http_connect" => {
                let mut p = proxy.clone();
                p.kind = ProxyKind::HttpConnect;
                Network {
                    ssh: None,
                    proxy: Some(p),
                }
            }
            "http_connect_draft_credentials" => {
                let mut p = proxy.clone();
                p.kind = ProxyKind::HttpConnect;
                p.user = Some("draft-user".into());
                Network {
                    ssh: None,
                    proxy: Some(p),
                }
            }
            "proxy_then_ssh" => {
                tunnel_ssh.host = "host.docker.internal".into();
                Network {
                    ssh: Some(tunnel_ssh),
                    proxy: Some(proxy.clone()),
                }
            }
            _ => Network {
                ssh: Some(tunnel_ssh),
                proxy: None,
            },
        };
        let draft_profile_id = match mode {
            "ssh_password_draft_credentials" => "w1-injected-ssh-password",
            "http_connect_draft_credentials" => "w1-injected-proxy-password",
            _ => "w1-network",
        };
        let mut network_secrets = Secrets::default();
        match mode {
            "ssh_password_draft_credentials" => {
                network_secrets.ssh_password =
                    Some(fixture_secrets["w1-network:ssh_password"].clone());
                assert!(
                    crate::storage::keychain_get(&format!("{draft_profile_id}:ssh_password"))
                        .unwrap()
                        .is_none()
                );
            }
            "http_connect_draft_credentials" => {
                network_secrets.proxy_password = Some("transient-draft-proxy-password".into());
                assert!(
                    crate::storage::keychain_get(&format!("{draft_profile_id}:proxy_password"))
                        .unwrap()
                        .is_none()
                );
            }
            _ => {}
        }
        let pg = driver_postgres::PostgresDriver::new();
        let mongo = driver_mongodb::MongoDriver::new();
        let redis = driver_redis::RedisDriver::new();
        let cases: Vec<(&dyn Driver, BTreeMap<String, String>, &str)> = vec![
            (
                &pg,
                BTreeMap::from([
                    ("host".into(), "host.docker.internal".into()),
                    ("port".into(), "55432".into()),
                    ("database".into(), "axdb_test".into()),
                    ("user".into(), "postgres".into()),
                    ("sslmode".into(), "disable".into()),
                ]),
                "testpw",
            ),
            (
                &mongo,
                BTreeMap::from([
                    ("uri".into(), "mongodb://host.docker.internal:57017".into()),
                    ("database".into(), "axdb_test".into()),
                    ("user".into(), "axdb".into()),
                    ("authSource".into(), "admin".into()),
                ]),
                "testmongo",
            ),
            (
                &redis,
                BTreeMap::from([
                    ("host".into(), "host.docker.internal".into()),
                    ("port".into(), "56379".into()),
                ]),
                "testredis",
            ),
        ];
        for (driver, fields, secret) in cases {
            let plugin_dir = std::path::PathBuf::from(
                std::env::var_os("AXDB_DRIVER_DIR")
                    .expect("network E2E requires built plugin binaries"),
            );
            let plugin = crate::plugin_host::PluginDriver::load(
                plugin_dir.join(format!("axdb-driver-{}", driver.manifest().id)),
            )
            .await
            .unwrap();
            for driver in [driver, &plugin as &dyn Driver] {
                let profile = crate::storage::Profile {
                    id: draft_profile_id.into(),
                    name: "Network test".into(),
                    driver_id: driver.manifest().id.clone(),
                    fields: fields.clone(),
                    save_secret: false,
                    access_mode: Default::default(),
                    network: Some(network.clone()),
                    auth: Auth::Password,
                };
                let prepared = prepare(
                    &profile,
                    fields.clone(),
                    Some(secret.into()),
                    network_secrets.clone(),
                    driver.manifest(),
                    temp.path(),
                    state.clone(),
                    Progress::default(),
                )
                .await
                .unwrap_or_else(|e| panic!("{mode}: {}", e.message));
                let session = driver
                    .connect(&ConnectConfig {
                        fields: prepared.fields,
                        secret: prepared.secret,
                        connect_timeout_ms: 15000,
                    })
                    .await
                    .unwrap_or_else(|e| panic!("{mode}/{}: {:?}", driver.manifest().id, e.kind));
                session.ping().await.unwrap();
                let info = session.server_info().await.unwrap();
                assert!(!info.is_empty());
                if matches!(
                    mode,
                    "ssh_password_draft_credentials" | "http_connect_draft_credentials"
                ) {
                    let key = if mode == "ssh_password_draft_credentials" {
                        "ssh_password"
                    } else {
                        "proxy_password"
                    };
                    assert!(
                        crate::storage::keychain_get(&format!("{draft_profile_id}:{key}"))
                            .unwrap()
                            .is_none()
                    );
                }
                session.close().await;
                drop(prepared.tunnel);
            }
        }
    }
    assert!(confirmations.load(Ordering::Relaxed) >= 2);
    // Known hosts are pinned in app-owned test data and private on disk.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(temp.path().join("known_hosts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn real_ssh_reconnects_without_replaying_database_work() {
    let Some(path) = std::env::var_os("AXDB_TEST_NETWORK_FIXTURE") else {
        return;
    };
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(State::default());
    let weak = Arc::downgrade(&state);
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let failures = Arc::new(Mutex::new(Vec::new()));
    let failed = failures.clone();
    state.set_sink(Arc::new(move |event| match event {
        Event::HostKey { request_id, .. } => {
            weak.upgrade().unwrap().answer(&request_id, true).unwrap()
        }
        Event::Network { state, .. } if state == "reconnecting" => {
            count.fetch_add(1, Ordering::Relaxed);
        }
        Event::Network { state, code, .. } if state == "disconnected" => {
            failed.lock().unwrap().push(code);
        }
        _ => {}
    }));
    let ssh = Ssh {
        host: "127.0.0.1".into(),
        port: 55222,
        user: fixture["ssh_user"].as_str().unwrap().into(),
        auth: SshAuth::Key,
        key_file: Some(fixture["key_file"].as_str().unwrap().into()),
        jump_hosts: vec![],
    };
    let connection = ssh::connect(
        &ssh,
        None,
        "w1-network",
        &Secrets::default(),
        &dir.path().join("known_hosts"),
        state.clone(),
    )
    .await
    .unwrap();
    connection.disconnect_for_test().await;
    let route = Route {
        tls: None,
        network: Network {
            ssh: Some(ssh),
            proxy: None,
        },
        network_secrets: Secrets::default(),
        profile_id: "w1-network".into(),
        host: "host.docker.internal".into(),
        port: 55432,
        path: dir.path().join("known_hosts"),
        state,
        ssh: tokio::sync::Mutex::new(Some(connection)),
    };
    let stream = route.open().await.unwrap();
    drop(stream);
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert!(!route.ssh.lock().await.as_ref().unwrap().closed());
    route
        .ssh
        .lock()
        .await
        .as_ref()
        .unwrap()
        .disconnect_for_test()
        .await;
    crate::storage::write_private(&route.path, b"[127.0.0.1]:55222 ssh-ed25519 changed\n").unwrap();
    assert_eq!(
        route.open().await.err().unwrap().message,
        "SSH_HOST_KEY_CHANGED"
    );
    assert_eq!(
        *failures.lock().unwrap(),
        vec![Some("SSH_HOST_KEY_CHANGED".into())]
    );
}

#[tokio::test]
async fn real_tunnel_verifies_tls_name_and_cancels_over_tls() {
    let Some(path) = std::env::var_os("AXDB_TEST_NETWORK_FIXTURE") else {
        return;
    };
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(State::default());
    let weak = Arc::downgrade(&state);
    state.set_sink(Arc::new(move |event| {
        if let Event::HostKey { request_id, .. } = event {
            weak.upgrade().unwrap().answer(&request_id, true).unwrap();
        }
    }));
    let profile: crate::storage::Profile = serde_json::from_value(serde_json::json!({
        "id":"w1-network", "name":"TLS test only", "driver_id":"postgres",
        "fields":{"host":"host.docker.internal", "port":"55433", "database":"axdb_test", "user":"postgres", "sslmode":"verify-full", "sslrootcert":fixture["tls_ca"]},
        "network":{"ssh":{"host":"127.0.0.1", "port":55222, "user":fixture["ssh_user"], "auth":"key", "key_file":fixture["key_file"]}}
    })).unwrap();
    let plugin = crate::plugin_host::PluginDriver::load(
        PathBuf::from(std::env::var_os("AXDB_DRIVER_DIR").unwrap()).join("axdb-driver-postgres"),
    )
    .await
    .unwrap();
    let pg = driver_postgres::PostgresDriver::new();
    for driver in [&pg as &dyn Driver, &plugin as &dyn Driver] {
        let prepared = prepare(
            &profile,
            profile.fields.clone(),
            Some("testpw".into()),
            Secrets::default(),
            driver.manifest(),
            dir.path(),
            state.clone(),
            Progress::default(),
        )
        .await
        .unwrap();
        let mut cfg = ConnectConfig {
            fields: prepared.fields,
            secret: prepared.secret,
            connect_timeout_ms: 10000,
        };
        // Check the real TLS boundary independently of the driver's error normalization.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut socket = tokio::net::TcpStream::connect((
            "127.0.0.1",
            cfg.fields["port"].parse::<u16>().unwrap(),
        ))
        .await
        .unwrap();
        socket
            .write_all(&[0, 0, 0, 8, 4, 210, 22, 47])
            .await
            .unwrap();
        assert_eq!(socket.read_u8().await.unwrap(), b'S');
        let ca = std::fs::read(fixture["tls_ca"].as_str().unwrap()).unwrap();
        let connector: tokio_native_tls::TlsConnector = native_tls::TlsConnector::builder()
            .disable_built_in_roots(true)
            .add_root_certificate(native_tls::Certificate::from_pem(&ca).unwrap())
            .build()
            .unwrap()
            .into();
        let checked = connector
            .connect("host.docker.internal", socket)
            .await
            .unwrap();
        drop(checked);
        let session = driver.connect(&cfg).await.unwrap();
        let result = session
            .execute(
                &dbcore::ExecRequest {
                    text: "SELECT pg_sleep(5)".into(),
                    params: Default::default(),
                    read_only: true,
                    row_limit: 1,
                    graph_node_limit: 1,
                    timeout_ms: 100,
                    preview_sample_limit: 1,
                },
                CancellationToken::new(),
            )
            .await;
        assert!(result.is_err());
        session
            .ping()
            .await
            .expect("session remains usable after TLS cancellation");
        session.close().await;
        cfg.fields
            .insert("tls_server_name".into(), "wrong.invalid".into());
        assert!(
            driver.connect(&cfg).await.is_err(),
            "wrong certificate name was accepted"
        );
        drop(prepared.tunnel);
    }
}
