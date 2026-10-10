//! The update check: a stand-in for the GitHub releases list; only a newer release is offered.

use axum::routing::get;
use axum::Router;
use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::net::TcpListener;

#[tokio::test]
async fn offers_only_a_newer_release() {
    let list = json!([
        {"tag_name": "QRZero-Alpha-v0.9", "html_url": "https://example.test/0.9", "draft": false},
        {"tag_name": "QRZero-Alpha-v0.10", "html_url": "https://example.test/0.10", "draft": false},
        {"tag_name": "Alpha-Release", "html_url": "https://example.test/old", "draft": false},
    ]);
    let app = Router::new().route("/releases", get(move || async move { list.to_string() }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/releases", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
    cfg.updates_url = url;
    let running = start(cfg).await.unwrap();
    let base = format!("http://{}/api/updates", running.addr);
    let http = reqwest::Client::new();
    let put = |on: bool| {
        let req = http.put(&base).header("x-qrzero-token", &running.token).json(&json!({ "enabled": on }));
        async move { req.send().await.unwrap().json::<Value>().await.unwrap() }
    };

    let v = put(true).await;
    assert_eq!(v["enabled"], true);
    // The test build is 0.1.0, so 0.10 is newer.
    assert_eq!(v["update"]["version"], "0.10");
    assert_eq!(v["update"]["url"], "https://example.test/0.10");
    let off = put(false).await;
    assert_eq!(off["enabled"], false);
    assert!(off["update"].is_null(), "nothing is shown while the check is off");
}
