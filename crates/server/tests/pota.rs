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

    for s in spots {
        assert_eq!((s["park_worked"].as_bool(), s["worked_today"].as_bool()), (Some(false), Some(false)), "nothing in the log yet");
    }

    get("/pota").await;
    assert_eq!(hits.load(Ordering::SeqCst), 1, "a second request within the keep time reuses the list");
    get("/pota?force=1").await;
    assert_eq!(hits.load(Ordering::SeqCst), 2, "a forced one reads it again");
}

#[tokio::test]
async fn parks_and_the_day_are_checked_against_the_log() {
    let feed = r#"[
      {"spotId":1,"activator":"K1ABC","frequency":"14244","mode":"SSB","reference":"US-0001","spotTime":"2026-10-31T22:00:00"},
      {"spotId":2,"activator":"K1ABC","frequency":"14244","mode":"SSB","reference":"US-0002","spotTime":"2026-10-31T22:01:00"},
      {"spotId":3,"activator":"N2DEF","frequency":"14062","mode":"FT8","reference":"US-0003","spotTime":"2026-10-31T22:02:00"},
      {"spotId":4,"activator":"W3OLD","frequency":"7032","mode":"CW","reference":"US-0004","spotTime":"2026-10-31T22:03:00"}
    ]"#;
    let app = Router::new().route("/pota", get(move || async move { feed }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}-parks", std::process::id());
    cfg.update_cty = false;
    cfg.pota_url = format!("http://{addr}/pota");
    let running = start(cfg).await.unwrap();
    let http = reqwest::Client::new();
    let base = format!("http://{}/api", running.addr);
    let get = |path: String| {
        let req = http.get(format!("{base}{path}")).header("x-qrzero-token", &running.token);
        async move { req.send().await.unwrap().json::<Value>().await.unwrap() }
    };
    let log = get("/logs".into()).await[0]["id"].as_i64().unwrap();
    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let log_qso = |call: &str, date: &str, band_mhz: &str, mode: &str, park: &str| {
        let body = serde_json::json!({"fields": {"CALL": call, "QSO_DATE": date, "TIME_ON": "0001", "FREQ": band_mhz, "MODE": mode, "POTA_REF": park}});
        let req = http.post(format!("{base}/logs/{log}/qsos")).header("x-qrzero-token", &running.token).json(&body);
        async move { req.send().await.unwrap().error_for_status().unwrap() }
    };
    // K1ABC at US-0001 today on 20m phone; N2DEF at US-0003 a year ago; the n-fer logs both US-0004 and US-0005.
    log_qso("K1ABC", &today, "14.244", "SSB", "US-0001").await;
    log_qso("N2DEF", "20250101", "14.074", "FT8", "US-0003").await;
    log_qso("W3OLD", "20250101", "7.032", "CW", "US-0004, US-0005").await;

    let list = get("/pota?force=1".into()).await;
    let by = |id: i64| list["spots"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap().clone();
    assert_eq!((by(1)["park_worked"].as_bool(), by(1)["worked_today"].as_bool()), (Some(true), Some(true)));
    // The same activator at another park: not a worked park, not worked today.
    assert_eq!((by(2)["park_worked"].as_bool(), by(2)["worked_today"].as_bool()), (Some(false), Some(false)));
    // A park worked long ago counts for unique parks but not for today.
    assert_eq!((by(3)["park_worked"].as_bool(), by(3)["worked_today"].as_bool()), (Some(true), Some(false)));
    assert_eq!(by(4)["park_worked"].as_bool(), Some(true), "one of several parks in the field");
}
