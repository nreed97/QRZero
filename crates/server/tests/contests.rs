//! The contest list: a stand-in for the WA7BNM feed, now and upcoming, finished ones dropped.

use axum::routing::get;
use axum::Router;
use chrono::{Duration, Utc};
use qrzero_server::{start, Config};
use serde_json::Value;
use tokio::net::TcpListener;

fn span(from_hours: i64, to_hours: i64) -> String {
    let (a, b) = (Utc::now() + Duration::hours(from_hours), Utc::now() + Duration::hours(to_hours));
    format!("{}Z, {} to {}Z, {}", a.format("%H%M"), a.format("%b %-d"), b.format("%H%M"), b.format("%b %-d"))
}

#[tokio::test]
async fn contests_now_and_upcoming() {
    let item = |title: &str, hours: (i64, i64)| format!("<item><title>{title}</title><link>https://example.test/{}</link><description>{}</description></item>", title.len(), span(hours.0, hours.1));
    let feed = format!(
        "<?xml version=\"1.0\"?><rss><channel>{}{}{}{}</channel></rss>",
        item("Test CW Sprint, CW", (-3, 10)),
        item("Test RTTY Party", (30, 40)),
        item("Test Mixed Contest", (50, 60)),
        item("Old Phone Contest, SSB", (-100, -90)),
    );
    let app = Router::new().route("/feed", get(move || async move { feed.clone() }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/feed", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
    cfg.contests_url = url;
    let running = start(cfg).await.unwrap();
    let base = format!("http://{}/api", running.addr);
    let http = reqwest::Client::new();
    let call = |m: reqwest::Method, path: &str| {
        let req = http.request(m, format!("{base}{path}")).header("x-qrzero-token", &running.token);
        async move {
            let r = req.send().await.unwrap();
            assert_eq!(r.status(), 200);
            r.json::<Value>().await.unwrap()
        }
    };

    assert_eq!(call(reqwest::Method::GET, "/contests").await["items"].as_array().unwrap().len(), 0, "nothing fetched yet");
    let list = call(reqwest::Method::POST, "/contests/refresh").await;
    let items = list["items"].as_array().unwrap();
    let titles: Vec<&str> = items.iter().map(|i| i["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Test CW Sprint, CW", "Test RTTY Party", "Test Mixed Contest"], "the finished one is dropped, the rest are in date order");
    assert_eq!(items[0]["active"], true);
    assert_eq!(items[1]["active"], false);
    assert_eq!(items[0]["modes"], serde_json::json!(["CW"]));
    assert_eq!(items[1]["modes"], serde_json::json!(["DIGITAL"]));
    assert_eq!(items[2]["modes"], serde_json::json!([]));
    assert!(list["error"].is_null());
}
