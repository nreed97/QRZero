//! The POTA spot list: read from a stand-in for api.pota.app, newest first, kept briefly.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{routing::get, Router};
use qrzero_server::{start, Config};
use serde_json::Value;
use tokio::net::TcpListener;

#[tokio::test]
async fn spots_come_from_the_list() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let feed = r#"[
      {"spotId":1,"activator":"K1ABC","frequency":"14244","mode":"SSB","reference":"US-0001","spotTime":"2026-10-31T22:00:00","name":"Acadia","locationDesc":"US-ME","grid4":"FN54","spotter":"W1XYZ","comments":"loud"},
      {"spotId":2,"activator":"N2DEF/P","frequency":"7032.5","mode":"CW","reference":"US-0002","spotTime":"2026-10-31T22:10:00","name":"Other","locationDesc":"US-NY","spotter":"W2XYZ","comments":""},
      {"spotId":3,"activator":"BAD","frequency":"x","mode":"CW","reference":"US-0003","spotTime":"2026-10-31T22:20:00"}
    ]"#;
    let app = Router::new()
        .route("/pota", get(move || { counted.fetch_add(1, Ordering::SeqCst); async move { feed } }))
        .route("/down", get(|| async { (axum::http::StatusCode::BAD_GATEWAY, "no") }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
    cfg.pota_url = format!("http://{addr}/pota");
    let running = start(cfg).await.unwrap();
    let get = |path: &str| {
        let req = reqwest::Client::new().get(format!("http://{}/api{path}", running.addr)).header("x-qrzero-token", &running.token);
        async move { req.send().await.unwrap().json::<Value>().await.unwrap() }
    };

    let list = get("/pota").await;
    let spots = list["spots"].as_array().unwrap();
    assert_eq!(spots.len(), 2, "the entry without a frequency is skipped");
    assert_eq!(spots[0]["call"], "N2DEF/P", "newest first");
    assert_eq!(spots[0]["band"], "40m");
    assert_eq!(spots[1]["reference"], "US-0001");
    assert_eq!(spots[1]["freq_hz"], 14_244_000);
    assert!(list["error"].is_null());

    get("/pota").await;
    assert_eq!(hits.load(Ordering::SeqCst), 1, "a second request within the keep time reuses the list");
    get("/pota?force=1").await;
    assert_eq!(hits.load(Ordering::SeqCst), 2, "a forced one reads it again");
}
