use super::*;
use axdb_driver_package::{Platform, SigningKey};

fn fixture(binary: Vec<u8>, version: &str, key: Option<&SigningKey>) -> Vec<u8> {
    let target = host_target().to_string();
    let path = format!("bin/{target}/axdb-driver-example-kv");
    let manifest = Manifest {
        id: "example-kv".into(),
        name: "Example KV (memory)".into(),
        version: version.into(),
        protocol: 1,
        description: "Integration fixture".into(),
        paradigm: "KeyValue".into(),
        homepage: String::new(),
        license: "MIT".into(),
        platforms: [(
            target.clone(),
            Platform {
                path,
                sha256: package::sha256(&binary),
            },
        )]
        .into(),
    };
    package::write_package(&manifest, &[(target, binary)].into(), key).unwrap()
}
#[test]
fn transport_and_markers_reject_untrusted_paths() {
    let installer = Installer::default();
    for url in [
        "http://example.org/package",
        "file:///tmp/package",
        "https://user:password@example.org/package",
    ] {
        assert!(installer.validate_url(url).is_err());
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("example-kv")).unwrap();
    std::fs::write(
        root.path().join("example-kv/current.json"),
        br#"{"version":"1.0.0","executable":"../escape"}"#,
    )
    .unwrap();
    assert!(marker(root.path(), "example-kv").is_err());
    assert!(marker(root.path(), "../escape").is_err());
    assert!(installed_executables(root.path()).unwrap().is_empty());
}
#[tokio::test]
async fn unsigned_requires_consent_and_bad_handshake_never_registers() {
    let root = tempfile::tempdir().unwrap();
    let package_path = root.path().join("local.axdriver");
    std::fs::write(
        &package_path,
        fixture(b"invalid executable".to_vec(), "1.0.0", None),
    )
    .unwrap();
    let installer = Installer::default();
    let registry = Registry::new();
    let source = package_path.to_str().unwrap();
    assert!(installer
        .install(root.path(), &registry, source, false, |_| {})
        .await
        .unwrap_err()
        .message
        .contains("explicit consent required"));
    assert!(installer
        .install(root.path(), &registry, source, true, |_| {})
        .await
        .is_err());
    assert!(registry.manifests().is_empty());
    assert!(marker(root.path(), "example-kv").unwrap().is_none());
    assert!(!root.path().join("example-kv/1.0.0").exists());
}

/// Requires the real P1 SDK executable; no database connection is opened.
#[tokio::test]
#[ignore = "set AXDB_PACKAGE_TEST_BINARY to a built axdb-driver-example-kv executable"]
async fn http_catalog_install_handshake_rollback_update_remove() {
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use std::sync::atomic::AtomicUsize;
    #[derive(Clone)]
    struct Server {
        index: Arc<Mutex<Vec<u8>>>,
        package: Arc<Mutex<Vec<u8>>>,
        requests: Arc<AtomicUsize>,
        etags: Arc<AtomicUsize>,
    }
    async fn index(State(state): State<Server>, headers: HeaderMap) -> impl IntoResponse {
        state.requests.fetch_add(1, Ordering::Relaxed);
        if headers
            .get("if-none-match")
            .is_some_and(|value| value == "\"test-v1\"")
        {
            state.etags.fetch_add(1, Ordering::Relaxed);
            return (
                StatusCode::NOT_MODIFIED,
                [("etag", "\"test-v1\"")],
                Vec::new(),
            );
        }
        (
            StatusCode::OK,
            [("etag", "\"test-v1\"")],
            state.index.lock().unwrap().clone(),
        )
    }
    async fn download(State(state): State<Server>) -> Vec<u8> {
        state.package.lock().unwrap().clone()
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}");
    let key = SigningKey::from_bytes(&[11; 32]);
    let binary = std::fs::read(
        std::env::var_os("AXDB_PACKAGE_TEST_BINARY")
            .expect("AXDB_PACKAGE_TEST_BINARY must identify the real SDK fixture"),
    )
    .unwrap();
    let bytes = fixture(binary.clone(), "1.0.0", Some(&key));
    let entry = CatalogEntry {
        id: "example-kv".into(),
        name: "Example KV (memory)".into(),
        version: "1.0.0".into(),
        description: "fixture".into(),
        paradigm: "KeyValue".into(),
        homepage: String::new(),
        license: "MIT".into(),
        platforms: vec![host_target().into()],
        url: format!("{base}/driver.axdriver"),
        size: bytes.len() as u64,
        sha256: package::sha256(&bytes),
    };
    let state = Server {
        index: Arc::new(Mutex::new(
            serde_json::to_vec(
                &SignedCatalog::new(
                    Catalog {
                        drivers: vec![entry],
                    },
                    &key,
                )
                .unwrap(),
            )
            .unwrap(),
        )),
        package: Arc::new(Mutex::new(bytes)),
        requests: Arc::new(AtomicUsize::new(0)),
        etags: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/index.json", get(index))
        .route("/driver.axdriver", get(download))
        .with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let test_origin = reqwest::Url::parse(&base).unwrap().origin();
    let installer = Installer {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() > MAX_REDIRECTS || attempt.url().origin() != test_origin
                {
                    attempt.error("Unsafe redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .unwrap(),
        keys: vec![TrustedKey {
            public_key: key.verifying_key().to_bytes(),
        }],
        index_url: format!("{base}/index.json"),
        allow_loopback: true,
        ..Installer::default()
    };
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::new();
    assert!(installer.snapshot(root.path()).unwrap().drivers.is_empty());
    assert_eq!(
        state.requests.load(Ordering::Relaxed),
        0,
        "Reading cache must not contact network"
    );
    installer.refresh(root.path()).await.unwrap();
    assert_eq!(installer.snapshot(root.path()).unwrap().drivers.len(), 1);
    installer.refresh(root.path()).await.unwrap();
    assert_eq!(state.etags.load(Ordering::Relaxed), 1);
    let cancelled = installer
        .install(root.path(), &registry, "example-kv", false, |event| {
            if event.stage == "download" {
                assert!(event.bytes_per_second > 0.0);
                assert!(installer.cancel().unwrap());
            }
        })
        .await
        .unwrap_err();
    assert!(cancelled.message.contains("cancelled"));
    assert!(registry.manifests().is_empty());
    assert!(marker(root.path(), "example-kv").unwrap().is_none());
    let stages = Mutex::new(Vec::new());
    let install_started = Instant::now();
    installer
        .install(root.path(), &registry, "example-kv", false, |progress| {
            stages.lock().unwrap().push(progress.stage)
        })
        .await
        .unwrap();
    println!(
        "P2 install_total_ms={:.3}",
        install_started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(
        !installer.cancel().unwrap(),
        "Committed operations cannot be cancelled"
    );
    assert_eq!(registry.manifests()[0].id, "example-kv");
    assert_eq!(
        installer.snapshot(root.path()).unwrap().installed[0].version,
        "1.0.0"
    );
    assert!(stages.lock().unwrap().ends_with(&[
        "verify",
        "signature",
        "extract",
        "handshake",
        "register",
        "complete"
    ]));
    let local = root.path().join("update.axdriver");
    std::fs::write(&local, fixture(binary.clone(), "2.0.0", Some(&key))).unwrap();
    for cancel_stage in ["verify", "signature", "extract", "handshake", "register"] {
        let requested = Mutex::new(None);
        let result = installer
            .install(
                root.path(),
                &registry,
                local.to_str().unwrap(),
                false,
                |event| {
                    if event.stage == cancel_stage {
                        *requested.lock().unwrap() = Some(Instant::now());
                        assert!(installer.cancel().unwrap());
                    }
                },
            )
            .await;
        assert!(
            result.unwrap_err().message.contains("cancelled"),
            "{cancel_stage}"
        );
        println!(
            "P2 cancellation_{cancel_stage}_ms={:.3}",
            requested.lock().unwrap().unwrap().elapsed().as_secs_f64() * 1000.0
        );
        assert_eq!(
            marker(root.path(), "example-kv").unwrap().unwrap().version,
            "1.0.0"
        );
        assert!(registry.get("example-kv").is_some());
        assert!(!root.path().join("example-kv/2.0.0").exists());
        assert!(std::fs::read_dir(root.path().join("example-kv"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".install-")));
        assert!(!installer.cancel().unwrap());
    }
    std::fs::write(
        &local,
        fixture(b"broken executable".to_vec(), "2.0.0", Some(&key)),
    )
    .unwrap();
    assert!(installer
        .install(
            root.path(),
            &registry,
            local.to_str().unwrap(),
            false,
            |_| {}
        )
        .await
        .is_err());
    assert_eq!(
        marker(root.path(), "example-kv").unwrap().unwrap().version,
        "1.0.0"
    );
    assert!(!root.path().join("example-kv/2.0.0").exists());
    for version in ["2.0.0", "3.0.0"] {
        std::fs::write(&local, fixture(binary.clone(), version, Some(&key))).unwrap();
        installer
            .install(
                root.path(),
                &registry,
                local.to_str().unwrap(),
                false,
                |_| {},
            )
            .await
            .unwrap();
    }
    assert!(!root.path().join("example-kv/1.0.0").exists());
    assert!(root.path().join("example-kv/2.0.0").exists());
    assert!(root.path().join("example-kv/3.0.0").exists());
    // Corrupt remote bytes: the signed index digest must reject before activation.
    {
        let mut bytes = state.package.lock().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
    }
    assert!(installer
        .install(root.path(), &registry, "example-kv", false, |_| {})
        .await
        .unwrap_err()
        .message
        .contains("SHA-256 mismatch"));
    assert_eq!(
        marker(root.path(), "example-kv").unwrap().unwrap().version,
        "3.0.0"
    );
    installer
        .remove(root.path(), &registry, "example-kv")
        .await
        .unwrap();
    assert!(registry.manifests().is_empty());
    assert!(installed(root.path()).unwrap().is_empty());
    assert!(!root.path().join("example-kv").exists());
    std::fs::write(&local, fixture(binary, "1.0.0", None)).unwrap();
    assert!(installer
        .install(
            root.path(),
            &registry,
            local.to_str().unwrap(),
            false,
            |_| {}
        )
        .await
        .is_err());
    installer
        .install(
            root.path(),
            &registry,
            local.to_str().unwrap(),
            true,
            |_| {},
        )
        .await
        .unwrap();
    let installed_paths = installed_executables(root.path()).unwrap();
    assert_eq!(
        installed_paths.len(),
        1,
        "Explicit local consent must survive restart"
    );
    std::fs::write(&installed_paths[0], b"tampered on disk").unwrap();
    assert!(
        installed_executables(root.path()).unwrap().is_empty(),
        "Startup must recheck executable integrity"
    );
    installer
        .remove(root.path(), &registry, "example-kv")
        .await
        .unwrap();
    server.abort();
}

#[tokio::test]
async fn download_limit_checks_headers_and_streamed_bytes() {
    use axum::{body::Body, routing::get, Router};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/sized", get(|| async { "1234" }))
        .route(
            "/chunked",
            get(|| async {
                Body::from_stream(futures_util::stream::iter([
                    Ok::<_, std::io::Error>("12"),
                    Ok("34"),
                ]))
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let installer = Installer::default();
    for endpoint in ["sized", "chunked"] {
        let response = client
            .get(format!("http://{address}/{endpoint}"))
            .send()
            .await
            .unwrap();
        if endpoint == "chunked" {
            assert!(response.content_length().is_none());
        }
        assert!(installer
            .response_bytes(response, 3, &|_| {})
            .await
            .unwrap_err()
            .message
            .contains("size limit"));
    }
    server.abort();
}

#[cfg(unix)]
#[test]
fn local_artifacts_reject_named_pipes_before_opening() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pipe.axdriver");
    assert!(std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    assert!(read_limited(&path, 1024)
        .unwrap_err()
        .message
        .contains("regular files"));
}

#[tokio::test]
async fn progress_channel_coalesces_and_never_exceeds_ten_hz() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let channel = Channel::<Progress>::new(move |body| {
        let tauri::ipc::InvokeResponseBody::Json(json) = body else {
            panic!("Expected progress JSON")
        };
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        observed.lock().unwrap().push((Instant::now(), value));
        Ok(())
    });
    let (sender, task) = progress_channel(channel);
    sender.send_replace(Progress::new("download", 1, Some(100)));
    tokio::time::sleep(Duration::from_millis(20)).await;
    for value in 2..100 {
        sender.send_replace(Progress::new("download", value, Some(100)));
    }
    sender.send_replace(Progress::new("complete", 100, Some(100)));
    drop(sender);
    task.await.unwrap();
    let events = events.lock().unwrap();
    assert!(!events.is_empty() && events.len() <= 2);
    assert_eq!(events.last().unwrap().1["stage"], "complete");
    assert_eq!(events.last().unwrap().1["bytes"], 100);
    for pair in events.windows(2) {
        assert!(pair[1].0.duration_since(pair[0].0) >= Duration::from_millis(100));
    }
}

#[test]
fn catalog_cache_latency_and_disk_tamper_invalidation() {
    let key = SigningKey::from_bytes(&[11; 32]);
    let installer = Installer {
        keys: vec![TrustedKey {
            public_key: key.verifying_key().to_bytes(),
        }],
        ..Installer::default()
    };
    let catalog = Catalog {
        drivers: ["postgres", "neo4j", "sqlite", "mongodb", "redis"]
            .into_iter()
            .map(|id| CatalogEntry {
                id: id.into(),
                name: id.into(),
                version: "1.0.0".into(),
                description: "fixture catalog entry".into(),
                paradigm: "Relational".into(),
                homepage: String::new(),
                license: "MIT".into(),
                platforms: vec![host_target().into()],
                url: format!("https://example.org/{id}.axdriver"),
                size: 1,
                sha256: package::sha256(b"x"),
            })
            .collect(),
    };
    let cached = CachedCatalog {
        index: SignedCatalog::new(catalog, &key).unwrap(),
        etag: Some("fixture".into()),
    };
    let bundled = serde_json::to_vec(&cached.index).unwrap();
    assert_eq!(
        installer.bundled_catalog(&bundled).unwrap().drivers.len(),
        5
    );
    assert!(installer.bundled_catalog(&[]).unwrap().drivers.is_empty());
    assert!(installer.bundled_catalog(b"invalid json").is_err());
    let untrusted = Installer {
        keys: vec![],
        ..Installer::default()
    };
    assert!(untrusted.bundled_catalog(&bundled).is_err());
    let tampered = String::from_utf8(bundled)
        .unwrap()
        .replace("fixture catalog entry", "tampered catalog entry");
    assert!(installer.bundled_catalog(tampered.as_bytes()).is_err());
    let bytes = serde_json::to_vec(&cached).unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("catalog-cache.json");
    std::fs::write(&path, &bytes).unwrap();
    let started = Instant::now();
    assert_eq!(installer.snapshot(root.path()).unwrap().drivers.len(), 5);
    let cold = started.elapsed().as_secs_f64() * 1000.0;
    let mut samples = Vec::new();
    for _ in 0..100 {
        let started = Instant::now();
        assert_eq!(installer.snapshot(root.path()).unwrap().drivers.len(), 5);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "P2 catalog_5_drivers_disk_ms cold={cold:.3} p50={:.3} p95={:.3}",
        samples[50], samples[95]
    );
    let mut corrupt: CachedCatalog = serde_json::from_slice(&bytes).unwrap();
    corrupt.index.catalog.drivers[0].name = "tampered".into();
    std::fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    assert!(installer.snapshot(root.path()).unwrap().drivers.is_empty());
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(installer.snapshot(root.path()).unwrap().drivers.len(), 5);
}
