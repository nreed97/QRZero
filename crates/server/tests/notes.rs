use qrzero_server::{start, Config};
use reqwest::Method;
use serde_json::{json, Value};

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Api {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_path_buf());
        cfg.secret_service = format!("QRZero-test-{}", std::process::id());
        cfg.update_cty = false;
        let running = start(cfg).await.unwrap();
        Api { base: format!("http://{}/api", running.addr), token: running.token, http: reqwest::Client::new(), _dir: dir }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut req = self.http.request(method, format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        (status, resp.json().await.unwrap_or(Value::Null))
    }

    async fn ok(&self, method: Method, path: &str, body: Option<Value>) -> Value {
        let (s, v) = self.call(method, path, body).await;
        assert_eq!(s, 200, "{path}: {v}");
        v
    }
}

#[tokio::test]
async fn notes_round_trip_and_show_in_lookup() {
    let api = Api::new().await;
    let log = api.ok(Method::GET, "/logs", None).await[0]["id"].as_i64().unwrap();

    assert!(api.ok(Method::GET, &format!("/logs/{log}/notes/DL1ABC"), None).await.is_null());
    let r = api.ok(Method::GET, &format!("/logs/{log}/lookup/DL1ABC"), None).await;
    assert!(r["note"].is_null());

    // A portable form saves under the base call.
    let n = api.ok(Method::PUT, &format!("/logs/{log}/notes/dl1abc%2Fp"), Some(json!({"text": "Hans\nQSL direct"}))).await;
    assert_eq!(n["call"], "DL1ABC");
    assert_eq!(n["text"], "Hans\nQSL direct");
    let r = api.ok(Method::GET, &format!("/logs/{log}/lookup/EA8%2FDL1ABC"), None).await;
    assert_eq!(r["note"], "Hans\nQSL direct");

    api.ok(Method::PUT, &format!("/logs/{log}/notes/G4XYZ"), Some(json!({"text": "contest regular"}))).await;
    let page = api.ok(Method::GET, &format!("/logs/{log}/notes?q=dl&limit=10"), None).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["rows"][0]["call"], "DL1ABC");
    let page = api.ok(Method::GET, &format!("/logs/{log}/notes"), None).await;
    assert_eq!(page["total"], 2);

    // Blank text deletes; DELETE says whether there was a note.
    assert!(api.ok(Method::PUT, &format!("/logs/{log}/notes/G4XYZ"), Some(json!({"text": "  "}))).await.is_null());
    assert_eq!(api.ok(Method::DELETE, &format!("/logs/{log}/notes/DL1ABC"), None).await, true);
    assert_eq!(api.ok(Method::DELETE, &format!("/logs/{log}/notes/DL1ABC"), None).await, false);
    assert_eq!(api.ok(Method::GET, &format!("/logs/{log}/notes"), None).await["total"], 0);

    let (status, _) = api.call(Method::PUT, &format!("/logs/{log}/notes/%2F"), Some(json!({"text": "x"}))).await;
    assert_eq!(status, 400);
}
