//! Native plugin + loopback HTTP smoke check, without opening a GUI or installing anything.
use axum::{routing::get, Json, Router};
use serde_json::json;
use tauri_plugin_updater::UpdaterExt;

#[tokio::test]
async fn older_runtime_detects_local_feed_without_downloading_or_installing() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}/latest.json");
    let release = json!({
        "version": "0.2.0-test", "notes": "# 로컬 업데이트\n- 감지만 검증", "pub_date": "2026-10-19T00:00:00Z",
        "platforms": {"darwin-aarch64": {"url": format!("http://{address}/never-download"), "signature": "detection-only"}}
    });
    let router = Router::new().route("/latest.json", get(move || async { Json(release) }));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.package_info_mut().version = semver::Version::parse("0.1.9").unwrap();
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    context
        .config_mut()
        .plugins
        .0
        .insert("updater".into(), config["plugins"]["updater"].clone());
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    let updater = app
        .updater_builder()
        .target("darwin-aarch64")
        .endpoints(vec![url.parse().unwrap()])
        .unwrap()
        .timeout(std::time::Duration::from_secs(5))
        .no_proxy()
        .build()
        .unwrap();
    let update = updater
        .check()
        .await
        .unwrap()
        .expect("newer prerelease detected from 0.1.9");
    assert_eq!(update.version, "0.2.0-test");
    assert_eq!(update.current_version, "0.1.9");
    assert!(update.body.unwrap().contains("감지만 검증"));
    server.abort();
}
