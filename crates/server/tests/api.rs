use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::Query;
use axum::routing::get;
use axum::Router;
use qrzero_server::{start, Config};
use serde_json::{json, Value};

/// A stand-in for xmldata.qrz.com.
async fn mock_qrz(logins: Arc<AtomicUsize>) -> String {
    let app = Router::new().route(
        "/xml/",
        get(move |Query(q): Query<HashMap<String, String>>| {
            let logins = logins.clone();
            async move {
                if q.contains_key("username") {
                    logins.fetch_add(1, Ordering::SeqCst);
                    if q["password"] == "good" {
                        return "<QRZDatabase><Session><Key>abc</Key></Session></QRZDatabase>".to_string();
                    }
                    return "<QRZDatabase><Session><Error>Username/password incorrect</Error></Session></QRZDatabase>".to_string();
                }
                match q.get("callsign").map(String::as_str) {
                    Some("W1AW") => "<QRZDatabase><Callsign><call>W1AW</call><fname>Hiram</fname><name>Maxim</name><addr2>Newington</addr2><dxcc>291</dxcc></Callsign><Session><Key>abc</Key></Session></QRZDatabase>".to_string(),
                    Some(c) => format!("<QRZDatabase><Session><Key>abc</Key><Error>Not found: {c}</Error></Session></QRZDatabase>"),
                    None => String::new(),
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/xml/")
}

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Api {
    async fn new(qrz_endpoint: String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_path_buf());
        cfg.qrz_endpoint = qrz_endpoint;
        // Keep test passwords away from the real credential store entries.
        cfg.secret_service = format!("QRZero-test-{}", std::process::id());
        cfg.update_cty = false;
        let running = start(cfg).await.unwrap();
        Api {
            base: format!("http://{}/api", running.addr),
            token: running.token,
            http: reqwest::Client::new(),
            _dir: dir,
        }
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut req = self.http.request(method, format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    async fn get(&self, path: &str) -> Value {
        let (s, v) = self.call(reqwest::Method::GET, path, None).await;
        assert_eq!(s, 200, "{path}: {v}");
        v
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        let (s, v) = self.call(reqwest::Method::POST, path, Some(body)).await;
        assert_eq!(s, 200, "{path}: {v}");
        v
    }
}

#[tokio::test]
async fn token_is_required() {
    let api = Api::new(String::new()).await;
    let resp = reqwest::get(format!("{}/logs", api.base)).await.unwrap();
    assert_eq!(resp.status(), 401);
    // The UI itself is served without a token.
    let ui = reqwest::get(api.base.replace("/api", "/")).await.unwrap();
    assert_eq!(ui.status(), 200);
}

#[tokio::test]
async fn log_import_search_export() {
    let api = Api::new(String::new()).await;
    let logs = api.get("/logs").await;
    let log = logs[0]["id"].as_i64().unwrap();
    api.post(&format!("/logs/{log}/callsigns"), json!({"callsign": "n0call"})).await;
    let loc = api
        .post(&format!("/logs/{log}/locations"), json!({"name": "Home", "fields": {"MY_GRIDSQUARE": "EN34"}}))
        .await;
    let loc_id = loc["id"].as_i64().unwrap();

    let q = api
        .post(&format!("/logs/{log}/qsos"), json!({"location_id": loc_id, "fields": {"CALL": "W1AW", "QSO_DATE": "20240101", "TIME_ON": "1200", "FREQ": "7.030", "MODE": "CW"}}))
        .await;
    assert_eq!(q["fields"]["BAND"], "40m");
    assert_eq!(q["fields"]["MY_GRIDSQUARE"], "EN34");

    let adi = "<CALL:4>K1AB<QSO_DATE:8>20240102<TIME_ON:4>1300<BAND:3>20m<MODE:3>SSB<EOR>\n<CALL:4>W1AW<QSO_DATE:8>20240101<TIME_ON:4>1200<BAND:3>40m<MODE:2>CW<EOR>";
    let report: Value = api
        .http
        .post(format!("{}/logs/{log}/import?location_id={loc_id}&skip_duplicates=true", api.base))
        .header("x-qrzero-token", &api.token)
        .body(adi)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(report["imported"], 1);
    assert_eq!(report["duplicates"], 1);

    let found = api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"bands": ["20m"]}})).await;
    assert_eq!(found["total"], 1);
    assert_eq!(found["rows"][0]["fields"]["CALL"], "K1AB");
    let exact = api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"exact_call": "k1ab/p"}, "limit": 10000})).await;
    assert_eq!(exact["total"], 1);
    assert_eq!(api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"exact_call": "K1A"}})).await["total"], 0);

    let resp = api
        .http
        .post(format!("{}/logs/{log}/export", api.base))
        .header("x-qrzero-token", &api.token)
        .json(&json!({"filter": {"modes": ["CW"]}, "profile": "standard"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.headers()["x-qso-count"], "1");
    let text = resp.text().await.unwrap();
    assert!(text.contains("<CALL:4>W1AW") && !text.contains("K1AB"));
}

#[tokio::test]
async fn qrz_lookup_and_cache() {
    let logins = Arc::new(AtomicUsize::new(0));
    let api = Api::new(mock_qrz(logins.clone()).await).await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();

    // Lookups are off by default: only the local log is consulted.
    let r = api.get(&format!("/logs/{log}/lookup/W1AW")).await;
    assert!(r["station"].is_null() && r["error"].is_null());

    api.call(reqwest::Method::PUT, "/settings", Some(json!({"qrz_enabled": true, "qrz_username": "me", "qrz_password": "bad"}))).await;
    let (status, _) = api.call(reqwest::Method::POST, "/settings/qrz/test", None).await;
    assert_eq!(status, 502);
    let r = api.get(&format!("/logs/{log}/lookup/W1AW")).await;
    assert!(r["error"].as_str().unwrap().contains("incorrect"));

    let (_, s) = api.call(reqwest::Method::PUT, "/settings", Some(json!({"qrz_password": "good"}))).await;
    assert_eq!(s["qrz_password_set"], true);
    let r = api.get(&format!("/logs/{log}/lookup/w1aw")).await;
    assert_eq!(r["station"]["NAME"], "Hiram Maxim");
    assert_eq!(r["source"], "QRZ");
    assert_eq!(r["worked"]["dxcc"], 291);

    // Portable call falls back to the home call; the second lookup is cached.
    let r = api.get(&format!("/logs/{log}/lookup/W1AW%2FP")).await;
    assert_eq!(r["station"]["QTH"], "Newington");
    let r = api.get(&format!("/logs/{log}/lookup/W1AW")).await;
    assert_eq!(r["source"], "QRZ (cached)");

    let r = api.get(&format!("/logs/{log}/lookup/ZZ9ZZ")).await;
    assert!(r["station"].is_null() && r["error"].is_null());

    // Clearing the password removes it from the credential store.
    let (_, s) = api.call(reqwest::Method::PUT, "/settings", Some(json!({"qrz_password": ""}))).await;
    assert_eq!(s["qrz_password_set"], false);
}

#[tokio::test]
async fn logged_qsos_are_filled_in_from_qrz() {
    let api = Api::new(mock_qrz(Arc::new(AtomicUsize::new(0))).await).await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    let qso = |call: &str, extra: Value| {
        let mut f = json!({"CALL": call, "QSO_DATE": "20240101", "TIME_ON": "1200", "BAND": "20m", "MODE": "CW"});
        f.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({"fields": f})
    };
    let a = api.post(&format!("/logs/{log}/qsos"), qso("W1AW", json!({"QTH": "Hartford"}))).await["id"].as_i64().unwrap();
    let b = api.post(&format!("/logs/{log}/qsos"), qso("ZZ9ZZ", json!({}))).await["id"].as_i64().unwrap();

    // With lookups off it says so rather than doing nothing.
    let r = api.post("/qsos/lookup", json!({"ids": [a]})).await;
    assert_eq!(r["updated"], 0);
    assert!(r["errors"][0].as_str().unwrap().contains("QRZ login"), "{r}");

    api.call(reqwest::Method::PUT, "/settings", Some(json!({"qrz_enabled": true, "qrz_username": "me", "qrz_password": "good"}))).await;
    let r = api.post("/qsos/lookup", json!({"ids": [a, b]})).await;
    assert_eq!(r["updated"], 1, "{r}");
    assert!(r["errors"][0].as_str().unwrap().starts_with("ZZ9ZZ"), "{r}");
    let rows = api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"call": "W1AW"}})).await;
    let f = &rows["rows"][0]["fields"];
    assert_eq!(f["NAME"], "Hiram Maxim");
    assert_eq!(f["QTH"], "Hartford", "a filled field is kept");
    assert_eq!(f["CALL"], "W1AW");
}
