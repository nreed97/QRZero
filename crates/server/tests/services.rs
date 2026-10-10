//! The DX cluster and QSL uploads, against stand-in services.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::post;
use axum::Router;
use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const CTY: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
";

type Seen = Arc<Mutex<Vec<HashMap<String, String>>>>;

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    dir: tempfile::TempDir,
}

impl Api {
    async fn new(qrz: String, clublog: String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_path_buf());
        // Tests run in parallel and the Windows credential store is shared, so each server gets its own entries.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        cfg.secret_service = format!("QRZero-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
        cfg.update_cty = false;
        cfg.qsl_endpoints.qrz = qrz;
        cfg.qsl_endpoints.clublog = clublog;
        let running = start(cfg).await.unwrap();
        Api { base: format!("http://{}/api", running.addr), token: running.token, http: reqwest::Client::new(), dir }
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>, raw: Option<&str>) -> Value {
        let mut req = self.http.request(method, format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        if let Some(r) = raw {
            req = req.body(r.to_string());
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap();
        assert_eq!(status, 200, "{path}: {text}");
        serde_json::from_str(&text).unwrap_or(Value::Null)
    }

    async fn get(&self, path: &str) -> Value {
        self.send(reqwest::Method::GET, path, None, None).await
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        self.send(reqwest::Method::POST, path, Some(body), None).await
    }

    /// POST that returns the HTTP status instead of insisting on 200.
    async fn post_status(&self, path: &str, body: Value) -> u16 {
        let resp = self.http.post(format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token).json(&body).send().await.unwrap();
        resp.status().as_u16()
    }

    async fn put(&self, path: &str, body: Value) -> Value {
        self.send(reqwest::Method::PUT, path, Some(body), None).await
    }

    async fn wait_for(&self, path: &str, check: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..150 {
            let v = self.get(path).await;
            if check(&v) {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!("timed out waiting on {path}: {}", self.get(path).await);
    }

    /// One log with N0CALL at "Home", the country file, and that as the active station.
    async fn setup(&self) -> (i64, i64) {
        let log = self.get("/logs").await[0]["id"].as_i64().unwrap();
        self.post(&format!("/logs/{log}/callsigns"), json!({"callsign": "N0CALL"})).await;
        let loc = self.post(&format!("/logs/{log}/locations"), json!({"name": "Home", "fields": {"MY_GRIDSQUARE": "EN34"}})).await["id"].as_i64().unwrap();
        self.send(reqwest::Method::POST, "/cty", None, Some(CTY)).await;
        self.post("/station/active", json!({"log_id": log, "location_id": loc, "station_callsign": "N0CALL"})).await;
        (log, loc)
    }

    async fn qso(&self, log: i64, loc: i64, call: &str) -> i64 {
        let fields = json!({"CALL": call, "QSO_DATE": "20260101", "TIME_ON": "1200", "BAND": "20m", "FREQ": "14.025", "MODE": "CW", "STATION_CALLSIGN": "N0CALL"});
        self.post(&format!("/logs/{log}/qsos"), json!({"location_id": loc, "fields": fields})).await["id"].as_i64().unwrap()
    }

    async fn fields(&self, log: i64, call: &str) -> Value {
        let found = self.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"call": call}, "offset": 0, "limit": 5})).await;
        found["rows"][0]["fields"].clone()
    }
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn form(body: &str) -> HashMap<String, String> {
    body.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (unescape(k), unescape(v))).collect()
}

/// A stand-in for the QRZ Logbook API: W1AW is already in the logbook.
async fn mock_services(seen: Seen) -> (String, String) {
    let qrz_seen = seen.clone();
    let app = Router::new()
        .route(
            "/qrz",
            post(move |body: String| {
                let seen = qrz_seen.clone();
                async move {
                    let f = form(&body);
                    seen.lock().unwrap().push(f.clone());
                    if f["KEY"] != "GOOD-KEY" {
                        return "RESULT=AUTH&REASON=invalid api key".to_string();
                    }
                    match f["ACTION"].as_str() {
                        "STATUS" => "RESULT=OK&CALLSIGN=N0CALL&COUNT=1".to_string(),
                        _ if f["ADIF"].contains("<CALL:4>W1AW") => "RESULT=FAIL&REASON=Unable to add QSO to database: duplicate".to_string(),
                        _ if f["ADIF"].contains("<CALL:6>DL1BAD") => "RESULT=FAIL&REASON=wrong band".to_string(),
                        _ => "RESULT=OK&LOGID=1001&COUNT=1".to_string(),
                    }
                }
            }),
        )
        .route(
            "/clublog",
            post(move |body: String| {
                let seen = seen.clone();
                async move {
                    let f = form(&body);
                    seen.lock().unwrap().push(f.clone());
                    if f["password"] != "pw" {
                        return (axum::http::StatusCode::FORBIDDEN, "Invalid login".to_string());
                    }
                    (axum::http::StatusCode::OK, "OK".to_string())
                }
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/qrz"), format!("http://{addr}/clublog"))
}

/// A DX cluster node: asks for a login, then sends two spots and echoes commands.
async fn mock_node(logins: Arc<Mutex<Vec<String>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else { return };
            let logins = logins.clone();
            tokio::spawn(async move {
                let (r, mut w) = sock.into_split();
                let mut lines = BufReader::new(r).lines();
                w.write_all(b"Welcome to the test node\r\nlogin: ").await.unwrap();
                let Ok(Some(call)) = lines.next_line().await else { return };
                logins.lock().unwrap().push(call.trim().to_string());
                w.write_all(b"Hello N0CALL\r\nN0CALL de TEST 1200Z >\r\n").await.unwrap();
                w.write_all(b"DX de W3LPL:     14025.0  JA1XYZ       CW 25 dB 28 WPM                1234Z\r\n").await.unwrap();
                w.write_all(b"DX de K1TTT:      7074.0  W1AW         FT8 -12 dB                     1235Z\r\n").await.unwrap();
                while let Ok(Some(line)) = lines.next_line().await {
                    w.write_all(format!("echo: {}\r\n", line.trim()).as_bytes()).await.unwrap();
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn cluster_spots_are_flagged() {
    let seen = Seen::default();
    let (qrz, clublog) = mock_services(seen).await;
    let api = Api::new(qrz, clublog).await;
    let (log, loc) = api.setup().await;
    api.qso(log, loc, "W1AW").await;

    let logins = Arc::new(Mutex::new(Vec::new()));
    let port = mock_node(logins.clone()).await;
    api.put("/cluster", json!({"nodes": [{"name": "Test", "host": "127.0.0.1", "port": port, "login": "", "password": "", "commands": []}], "auto_connect": false})).await;
    api.post("/cluster/connect", json!({"connect": true})).await;

    let c = api.wait_for("/cluster", |v| v["spots"].as_array().is_some_and(|s| s.len() == 2)).await;
    assert_eq!(c["connected"], true);
    assert_eq!(logins.lock().unwrap().as_slice(), ["N0CALL"], "logs in with the active station callsign");
    let spots = c["spots"].as_array().unwrap();
    let ja = spots.iter().find(|s| s["call"] == "JA1XYZ").unwrap();
    assert_eq!(ja["freq_hz"], 14_025_000);
    assert_eq!(ja["band"], "20m");
    assert_eq!(ja["mode"], "CW");
    assert_eq!(ja["entity"]["name"], "Japan");
    assert_eq!(ja["needed"]["new_dxcc"], true);
    let w1aw = spots.iter().find(|s| s["call"] == "W1AW").unwrap();
    assert_eq!(w1aw["mode"], "FT8");
    assert_eq!(w1aw["needed"]["new_dxcc"], false, "the US is worked");
    assert_eq!(w1aw["needed"]["new_mode"], true, "FT8 is new for the US");

    api.post("/cluster/send", json!({"line": "sh/dx 5"})).await;
    api.wait_for("/cluster", |v| v["lines"].as_array().unwrap().iter().any(|l| l == "echo: sh/dx 5")).await;

    assert_eq!(c["config"]["spot_comment"], "spotted with QRZero", "the default spot comment");
    // Spotting: a fresh QSO goes out as a "DX" line, an old one is refused.
    let now = chrono::Utc::now().timestamp();
    api.post("/cluster/spot", json!({"call": "ja1xyz", "freq_khz": 14025.04, "comment": "CW  599\r\nsh/dx", "qso_utc": now - 60})).await;
    api.wait_for("/cluster", |v| v["lines"].as_array().unwrap().iter().any(|l| l == "echo: DX 14025.0 JA1XYZ CW  599sh/dx")).await;
    let old = api.post_status("/cluster/spot", json!({"call": "JA1XYZ", "freq_khz": 14025.0, "comment": "", "qso_utc": now - 3600})).await;
    assert_eq!(old, 400, "an hour-old QSO is too old to spot");

    api.post("/cluster/connect", json!({"connect": false})).await;
    api.wait_for("/cluster", |v| v["connected"] == false).await;
    let off = api.post_status("/cluster/spot", json!({"call": "JA1XYZ", "freq_khz": 14025.0, "comment": "", "qso_utc": now})).await;
    assert_eq!(off, 400, "no spot without a cluster connection");
}

#[tokio::test]
async fn qrz_and_clublog_uploads() {
    let seen = Seen::default();
    let (qrz, clublog) = mock_services(seen.clone()).await;
    let api = Api::new(qrz, clublog).await;
    let (log, loc) = api.setup().await;
    for call in ["JA1XYZ", "W1AW", "DL1BAD"] {
        api.qso(log, loc, call).await;
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["qrz_calls"] = json!(["N0CALL"]);
    cfg["qrz_since"] = json!("2025-01-01");
    cfg["clublog_calls"] = json!(["N0CALL"]);
    cfg["clublog_since"] = json!("2025-01-01");
    cfg["clublog_email"] = json!("op@example.com");
    let o = api.put("/qsl", json!({"config": cfg, "secrets": {"qrz_keys": {"N0CALL": "GOOD-KEY"}, "clublog_password": "pw", "clublog_app_key": "app"}})).await;
    assert_eq!(o["secrets"]["qrz_calls"], json!(["N0CALL"]));
    assert_eq!(o["pending"]["qrz"], 3);
    assert_eq!(o["pending"]["clublog"], 3);
    assert!(!serde_json::to_string(&o).unwrap().contains("GOOD-KEY"), "keys never come back");

    assert_eq!(api.post("/qsl/qrz/test", json!({"callsign": "N0CALL"})).await["callsign"], "N0CALL");

    let run = api.post("/qsl/upload/qrz", json!(null)).await;
    assert_eq!((run["uploaded"].as_u64(), run["duplicates"].as_u64()), (Some(1), Some(1)), "{run}");
    assert_eq!(run["rejected"].as_array().unwrap().len(), 1, "{run}");
    let ja = api.fields(log, "JA1XYZ").await;
    assert_eq!(ja["QRZCOM_QSO_UPLOAD_STATUS"], "Y");
    assert_eq!(ja["QRZCOM_QSO_UPLOAD_DATE"], today.replace('-', ""));
    assert_eq!(api.fields(log, "W1AW").await["QRZCOM_QSO_UPLOAD_STATUS"], "Y", "a duplicate counts as uploaded");
    assert!(api.fields(log, "DL1BAD").await.get("QRZCOM_QSO_UPLOAD_STATUS").is_none());

    // The refused QSO isn't retried, so a second run sends nothing.
    let before = seen.lock().unwrap().len();
    let run = api.post("/qsl/upload/qrz", json!(null)).await;
    assert_eq!(run["uploaded"], 0);
    assert_eq!(seen.lock().unwrap().len(), before);

    // Editing an uploaded QSO sends it again as a replacement.
    let id = api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"call": "JA1XYZ"}, "offset": 0, "limit": 1})).await["rows"][0]["id"].as_i64().unwrap();
    let mut f = ja.clone();
    f["RST_RCVD"] = json!("579");
    api.put(&format!("/qsos/{id}"), json!({"location_id": loc, "fields": f})).await;
    assert_eq!(api.fields(log, "JA1XYZ").await["QRZCOM_QSO_UPLOAD_STATUS"], "M");
    let run = api.post("/qsl/upload/qrz", json!(null)).await;
    assert_eq!(run["uploaded"], 1, "{run}");
    let last = seen.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.get("OPTION").map(String::as_str), Some("REPLACE"));
    assert!(last["ADIF"].contains("<RST_RCVD:3>579"));
    assert!(!last["ADIF"].contains("QRZCOM_QSO_UPLOAD_STATUS"), "upload status isn't sent");

    let run = api.post("/qsl/upload/clublog", json!(null)).await;
    assert_eq!(run["uploaded"], 3, "{run}");
    let last = seen.lock().unwrap().last().cloned().unwrap();
    assert_eq!((last["email"].as_str(), last["callsign"].as_str(), last["api"].as_str()), ("op@example.com", "N0CALL", "app"));
    assert_eq!(api.fields(log, "DL1BAD").await["CLUBLOG_QSO_UPLOAD_STATUS"], "Y");
    assert_eq!(api.get("/qsl").await["pending"]["clublog"], 0);

    // The log's right-click upload: set-up services are offered, and picked QSOs go up even though they're already sent.
    assert_eq!(api.get("/qsl/targets").await, json!(["qrz", "clublog"]));
    let before = seen.lock().unwrap().len();
    let run = api.post("/qsl/upload/clublog/qsos", json!({"ids": [id]})).await;
    assert_eq!(run["uploaded"].as_u64().unwrap() + run["duplicates"].as_u64().unwrap(), 1, "{run}");
    assert_eq!(seen.lock().unwrap().len(), before + 1);

    // A bad key stops the run with a clear error.
    api.put("/qsl", json!({"config": cfg, "secrets": {"qrz_keys": {"N0CALL": "BAD"}}})).await;
    api.qso(log, loc, "K5ABC").await;
    let run = api.post("/qsl/upload/qrz", json!(null)).await;
    assert!(run["error"].as_str().unwrap().contains("login refused"), "{run}");
    assert_eq!(api.fields(log, "K5ABC").await.get("QRZCOM_QSO_UPLOAD_STATUS"), None);
}

#[tokio::test]
async fn live_upload_waits_before_sending() {
    let seen = Seen::default();
    let (qrz, clublog) = mock_services(seen.clone()).await;
    let api = Api::new(qrz, clublog).await;
    let (log, loc) = api.setup().await;
    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["qrz_calls"] = json!(["N0CALL"]);
    cfg["qrz_since"] = json!("2025-01-01");
    cfg["qrz_live"] = json!(true);
    cfg["qrz_live_delay_min"] = json!(0);
    let o = api.put("/qsl", json!({"config": cfg, "secrets": {"qrz_keys": {"N0CALL": "GOOD-KEY"}}})).await;
    assert_eq!(o["config"]["qrz_live_delay_min"], 2, "an unset wait becomes 2 minutes");
    assert_eq!(o["config"]["qrz_enabled"], false, "live upload doesn't need the timer");
    api.qso(log, loc, "JA1XYZ").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(seen.lock().unwrap().is_empty(), "not sent before the wait is over");
}

#[cfg(unix)]
#[tokio::test]
async fn lotw_signs_with_tqsl() {
    use std::os::unix::fs::PermissionsExt;

    let (qrz, clublog) = mock_services(Seen::default()).await;
    let api = Api::new(qrz, clublog).await;
    let (log, loc) = api.setup().await;
    api.qso(log, loc, "JA1XYZ").await;
    api.qso(log, loc, "W1AW").await;

    // A stand-in tqsl that records its arguments and the file it was given.
    let tqsl = api.dir.path().join("tqsl");
    let record = api.dir.path().join("tqsl-args");
    std::fs::write(&tqsl, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {0}\ncat \"$9\" >> {0}\nexit 0\n", record.display())).unwrap();
    std::fs::set_permissions(&tqsl, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["tqsl_path"] = json!(tqsl);
    cfg["lotw_since"] = json!("2025-01-01");
    cfg["lotw"] = json!([{"callsign": "N0CALL", "location_id": loc, "station_location": "Home QTH"}]);
    let o = api.put("/qsl", json!({"config": cfg, "secrets": {"qrz_keys": {}}})).await;
    assert_eq!(o["pending"]["lotw"][0]["pending"], 2);

    let run = api.post("/qsl/upload/lotw", json!(null)).await;
    assert_eq!(run["uploaded"], 2, "{run}");
    let args = std::fs::read_to_string(&record).unwrap();
    assert!(args.starts_with("-d\n-q\n-x\n-a\ncompliant\n-l\nHome QTH\n-u\n"), "{args}");
    assert!(args.contains("<CALL:6>JA1XYZ") && args.contains("<CALL:4>W1AW"), "{args}");
    assert_eq!(api.fields(log, "W1AW").await["LOTW_QSL_SENT"], "Y");
    assert_eq!(api.get("/qsl").await["pending"]["lotw"][0]["pending"], 0);

    // A failing TQSL leaves the QSOs pending.
    std::fs::write(&tqsl, "#!/bin/sh\nexit 11\n").unwrap();
    api.qso(log, loc, "K5ABC").await;
    let run = api.post("/qsl/upload/lotw", json!(null)).await;
    assert!(run["error"].as_str().unwrap().contains("connection"), "{run}");
    assert_eq!(api.get("/qsl").await["pending"]["lotw"][0]["pending"], 1);
}

#[cfg(unix)]
#[tokio::test]
async fn lotw_upload_needed_by_date_range() {
    use std::os::unix::fs::PermissionsExt;

    let (qrz, clublog) = mock_services(Seen::default()).await;
    let api = Api::new(qrz, clublog).await;
    let (log, loc) = api.setup().await;
    for (call, date, sent) in [("A1AA", "20250601", None), ("B2BB", "20250602", Some("Y")), ("C3CC", "20250603", Some("N")), ("D4DD", "20250701", None)] {
        let mut f = json!({"CALL": call, "QSO_DATE": date, "TIME_ON": "2330", "BAND": "20m", "FREQ": "14.025", "MODE": "CW", "STATION_CALLSIGN": "N0CALL"});
        if let Some(s) = sent {
            f["LOTW_QSL_SENT"] = json!(s);
        }
        api.post(&format!("/logs/{log}/qsos"), json!({"location_id": loc, "fields": f})).await;
    }
    let tqsl = api.dir.path().join("tqsl");
    let record = api.dir.path().join("tqsl-args");
    std::fs::write(&tqsl, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {0}\ncat \"$9\" >> {0}\nexit 0\n", record.display())).unwrap();
    std::fs::set_permissions(&tqsl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["tqsl_path"] = json!(tqsl);
    cfg["lotw_since"] = json!("2030-01-01"); // the range overrides this
    cfg["lotw"] = json!([{"callsign": "N0CALL", "location_id": loc, "station_location": "Home QTH"}]);
    api.put("/qsl", json!({"config": cfg, "secrets": {"qrz_keys": {}}})).await;

    let w = api.get("/qsl/lotw/range?from=2025-06-01&to=2025-06-03").await;
    assert_eq!(w["locations"][0]["waiting"], 2, "{w}");
    let run = api.post("/qsl/lotw/range", json!({"from": "2025-06-01", "to": "2025-06-03"})).await;
    assert_eq!(run["uploaded"], 2, "{run}");
    let args = std::fs::read_to_string(&record).unwrap();
    assert!(args.contains("<CALL:4>A1AA") && args.contains("<CALL:4>C3CC"), "{args}");
    assert!(!args.contains("B2BB") && !args.contains("D4DD"), "{args}");
    let a = api.fields(log, "A1AA").await;
    assert_eq!(a["LOTW_QSL_SENT"], "Y");
    assert_eq!(a["LOTW_QSLSDATE"].as_str().unwrap().len(), 8);
    assert!(api.fields(log, "D4DD").await.get("LOTW_QSL_SENT").is_none());

    let bad = api.post("/qsl/lotw/range", json!({"from": "2025-06-03", "to": "2025-06-01"})).await;
    assert!(bad["error"].as_str().unwrap().contains("before"), "{bad}");
}
