use std::path::Path;
use std::time::Duration;

use qrzero_server::{start, Config, Running};
use reqwest::Method;
use serde_json::{json, Value};

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    running: Running,
}

impl Api {
    async fn new(dir: &Path) -> Self {
        let mut cfg = Config::local(dir.to_path_buf());
        cfg.secret_service = format!("QRZero-test-{}", std::process::id());
        cfg.update_cty = false;
        let running = start(cfg).await.unwrap();
        Api { base: format!("http://{}/api", running.addr), token: running.token.clone(), http: reqwest::Client::new(), running }
    }

    fn req(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http.request(method, format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token)
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut req = self.req(method, path);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    async fn ok(&self, method: Method, path: &str, body: Option<Value>) -> Value {
        let (s, v) = self.call(method, path, body).await;
        assert_eq!(s, 200, "{path}: {v}");
        v
    }

    async fn log_qso(&self, call: &str) {
        self.ok(Method::POST, "/logs/1/qsos", Some(json!({"fields": {"CALL": call, "QSO_DATE": "20240101", "TIME_ON": "1200", "BAND": "20m", "MODE": "CW"}}))).await;
    }

    async fn qso_count(&self) -> i64 {
        self.ok(Method::POST, "/logs/1/qsos/search", Some(json!({}))).await["total"].as_i64().unwrap()
    }
}

fn of_kind(overview: &Value, kind: &str) -> Vec<String> {
    overview["backups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["kind"] == kind)
        .map(|b| b["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn backup_download_and_restore_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let api = Api::new(dir.path()).await;

    // The daily automatic backup runs in the background at startup.
    let mut overview = Value::Null;
    for _ in 0..100 {
        overview = api.ok(Method::GET, "/backups", None).await;
        if !of_kind(&overview, "auto").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(of_kind(&overview, "auto").len(), 1, "{overview}");
    assert_eq!(overview["settings"], json!({"auto": true, "keep": 10}));
    assert!(overview["folder"].as_str().unwrap().ends_with("backups"));
    assert!(overview["pending"].is_null());

    api.log_qso("W1AW").await;
    let b = api.ok(Method::POST, "/backups", None).await;
    assert_eq!(b["kind"], "manual");
    let name = b["name"].as_str().unwrap().to_string();
    api.log_qso("K1ABC").await;

    // Download.
    let resp = api.req(Method::GET, &format!("/backups/files/{name}")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers()["content-disposition"].to_str().unwrap().contains(&name));
    let bytes = resp.bytes().await.unwrap();
    assert!(bytes.starts_with(b"SQLite format 3\0"));
    assert_eq!(bytes.len() as u64, b["size"].as_u64().unwrap());
    // Needs the token like everything else, and stays inside the folder.
    let resp = api.http.get(format!("{}/backups/files/{name}", api.base)).send().await.unwrap();
    assert_eq!(resp.status(), 401);
    assert_eq!(api.call(Method::GET, "/backups/files/..%2Fqrzero.db", None).await.0, 400);
    assert_eq!(api.call(Method::GET, "/backups/files/missing.db", None).await.0, 404);

    // Uploads must be QRZero logs.
    let resp = api.req(Method::POST, "/backups/restore?name=notes.txt").body("not a log").send().await.unwrap();
    assert_eq!(resp.status(), 400);
    assert!(api.ok(Method::GET, "/backups", None).await["pending"].is_null());

    // Stage a listed backup, then cancel it.
    let p = api.ok(Method::POST, &format!("/backups/files/{name}/restore"), None).await;
    assert_eq!(p["summary"]["qsos"], 1);
    assert_eq!(api.ok(Method::GET, "/backups", None).await["pending"]["source"], name.as_str());
    api.ok(Method::DELETE, "/backups/pending", None).await;
    assert!(api.ok(Method::GET, "/backups", None).await["pending"].is_null());

    // Stage the downloaded copy as an upload. Nothing changes until restart.
    let resp = api.req(Method::POST, "/backups/restore?name=my%20backup.db").body(bytes.to_vec()).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(api.ok(Method::GET, "/backups", None).await["pending"]["source"], "my backup.db");
    assert_eq!(api.qso_count().await, 2);

    let o = api.ok(Method::PUT, "/backups/settings", Some(json!({"auto": false, "keep": 3}))).await;
    assert_eq!(o["settings"], json!({"auto": false, "keep": 3}));

    // Delete a backup.
    let auto = of_kind(&o, "auto")[0].clone();
    api.ok(Method::DELETE, &format!("/backups/files/{auto}"), None).await;
    assert!(of_kind(&api.ok(Method::GET, "/backups", None).await, "auto").is_empty());

    // Restart: the staged log is swapped in and the old one kept.
    api.running.handle.abort();
    drop(api);
    let api = Api::new(dir.path()).await;
    let o = api.ok(Method::GET, "/backups", None).await;
    assert!(o["pending"].is_null(), "{o}");
    assert_eq!(o["last_restore"]["ok"], true, "{o}");
    assert!(o["last_restore"]["message"].as_str().unwrap().contains("my backup.db"));
    assert_eq!(of_kind(&o, "before_restore").len(), 1);
    assert_eq!(api.qso_count().await, 1);
    // Settings come from the restored log (the backup was made before they changed).
    assert_eq!(o["settings"], json!({"auto": true, "keep": 10}));
}
