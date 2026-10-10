//! Confirmation downloads (LoTW, eQSL), eQSL uploads, paper cards and awards.

use std::sync::{Arc, Mutex};

use axum::extract::Query;
use axum::routing::{get, post};
use axum::Router;
use qrzero_server::{start, Config, QslEndpoints};
use serde_json::{json, Value};
use tokio::net::TcpListener;

const CTY: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
DL,Germany,230,EU,14,28,51.00,-10.00,-1.0,DA DB DC DD DE DF DG DH DI DJ DK DL DM DN DO DP DQ DR;
";

fn adif_field(k: &str, v: &str) -> String {
    format!("<{k}:{}>{v}", v.len())
}

fn record(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| adif_field(k, v)).collect::<Vec<_>>().join(" ") + " <EOR>\n"
}

/// LoTW report and eQSL inbox/upload stand-ins. Returns the endpoints and the upload log.
async fn mock(uploads: Arc<Mutex<Vec<String>>>) -> QslEndpoints {
    let lotw = get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
        if q.get("password").map(String::as_str) != Some("lotwpw") {
            return "<html>Username/password incorrect</html>".to_string();
        }
        let mut out = String::from("ARRL Logbook of the World Status Report\n<PROGRAMID:4>LoTW\n<APP_LoTW_LASTQSL:19>2026-01-07 10:00:00\n<eoh>\n");
        out += &record(&[("CALL", "JA1XYZ"), ("BAND", "20M"), ("MODE", "CW"), ("QSO_DATE", "20260101"), ("TIME_ON", "120500"), ("QSL_RCVD", "Y"), ("QSLRDATE", "20260105"), ("DXCC", "339"), ("CQZ", "25")]);
        out += &record(&[("CALL", "W1AW"), ("BAND", "20M"), ("MODE", "CW"), ("QSO_DATE", "20260101"), ("TIME_ON", "1158"), ("QSL_RCVD", "Y"), ("QSLRDATE", "20260106"), ("DXCC", "291"), ("STATE", "CT"), ("CQZ", "5")]);
        out += &record(&[("CALL", "VK2XYZ"), ("BAND", "40M"), ("MODE", "SSB"), ("QSO_DATE", "20260102"), ("TIME_ON", "0800"), ("QSL_RCVD", "Y")]);
        out
    });
    let inbox = get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
        if q.get("Password").map(String::as_str) != Some("eqslpw") {
            return "<HTML>Error: No such Username/Password found</HTML>".to_string();
        }
        r#"<HTML><BODY>Your ADIF log file has been built. <A HREF="../downloadedfiles/abc.adi">.ADI file</A></BODY></HTML>"#.to_string()
    });
    let file = get(|| async { format!("eQSL inbox\n<EOH>\n{}", record(&[("CALL", "DL1BAD"), ("BAND", "20M"), ("MODE", "CW"), ("QSO_DATE", "20260101"), ("TIME_ON", "1210"), ("QSL_RCVD", "Y"), ("GRIDSQUARE", "JO62")])) });
    let upload = post(move |body: String| {
        let uploads = uploads.clone();
        async move {
            uploads.lock().unwrap().push(body);
            "<HTML>Result: 1 out of 1 records added<BR></HTML>".to_string()
        }
    });
    // QRZ Logbook: the confirmed QSOs, with the ADIF angle brackets as entities like the real service.
    let qrz = post(|body: String| async move {
        if !body.contains("KEY=QRZ-KEY") {
            return "RESULT=AUTH&REASON=invalid+api+key".to_string();
        }
        assert!(body.contains("STATUS%3ACONFIRMED") && body.contains("AFTERLOGID%3A0"), "{body}");
        let adi = record(&[("CALL", "JA1XYZ"), ("BAND", "20m"), ("MODE", "CW"), ("QSO_DATE", "20260101"), ("TIME_ON", "120500"), ("APP_QRZLOG_LOGID", "77")]);
        format!("RESULT=OK&COUNT=1&LOGIDS=77&ADIF={}", adi.replace('<', "&lt;").replace('>', "&gt;"))
    });
    // Club Log matches: rows of [call, dxcc, date, band id, mode].
    let matches = get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
        if q.get("password").map(String::as_str) != Some("clpw") || q.get("api").map(String::as_str) != Some("appkey") {
            return (axum::http::StatusCode::FORBIDDEN, "Invalid login".to_string());
        }
        (axum::http::StatusCode::OK, r#"[["W1AW","291","2026-01-01 11:58:00","20","CW"],["VK2XYZ","150","2026-01-02 08:00:00","40",false],["ZZ9ZZ","1","2026-01-03 08:00:00","40","SSB"]]"#.to_string())
    });
    let app = Router::new()
        .route("/qrz", qrz)
        .route("/getmatches.php", matches)
        .route("/lotwuser/lotwreport.adi", lotw)
        .route("/qslcard/DownloadInBox.cfm", inbox)
        .route("/downloadedfiles/abc.adi", file)
        .route("/qslcard/ImportADIF.cfm", upload);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    QslEndpoints {
        lotw_report: format!("http://{addr}/lotwuser/lotwreport.adi"),
        eqsl_inbox: format!("http://{addr}/qslcard/DownloadInBox.cfm"),
        eqsl_upload: format!("http://{addr}/qslcard/ImportADIF.cfm"),
        qrz: format!("http://{addr}/qrz"),
        clublog_matches: format!("http://{addr}/getmatches.php"),
        ..QslEndpoints::default()
    }
}

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Api {
    async fn new(endpoints: QslEndpoints) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_path_buf());
        cfg.secret_service = format!("QRZero-test-{}", std::process::id());
        cfg.update_cty = false;
        cfg.qsl_endpoints = endpoints;
        let running = start(cfg).await.unwrap();
        Api { base: format!("http://{}/api", running.addr), token: running.token, http: reqwest::Client::new(), _dir: dir }
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

    async fn put(&self, path: &str, body: Value) -> Value {
        self.send(reqwest::Method::PUT, path, Some(body), None).await
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


fn has_cell(table: &Value, key: &str, col: &str, status: &str) -> bool {
    table["rows"].as_array().unwrap().iter().any(|r| r["key"] == key && r["cells"][col] == status)
}

#[tokio::test]
async fn confirmations_awards_and_paper_cards() {
    let uploads = Arc::new(Mutex::new(Vec::new()));
    let api = Api::new(mock(uploads.clone()).await).await;
    let (log, loc) = api.setup().await;
    for call in ["JA1XYZ", "W1AW", "DL1BAD"] {
        api.qso(log, loc, call).await;
    }

    // Worked, nothing confirmed yet.
    let dxcc = api.get(&format!("/logs/{log}/awards/dxcc?lotw=true&paper=true&unworked=true")).await;
    assert_eq!(dxcc["total"], 3, "{dxcc}");
    assert!(has_cell(&dxcc, "339", "20m", "worked"), "{dxcc}");
    assert_eq!(dxcc["columns"][0]["worked"], 3);
    assert_eq!(dxcc["columns"][0]["confirmed"], 0);

    // LoTW: wrong password, then the real download.
    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["lotw_username"] = json!("n0call");
    api.put("/qsl", json!({"config": cfg, "secrets": {"lotw_password": "nope"}})).await;
    let d = api.post("/qsl/download/lotw", json!(null)).await;
    assert!(d["error"].as_str().unwrap().contains("password"), "{d}");
    api.put("/qsl", json!({"config": cfg, "secrets": {"lotw_password": "lotwpw"}})).await;
    let d = api.post("/qsl/download/lotw", json!(null)).await;
    assert_eq!((d["received"].as_u64(), d["confirmed"].as_u64(), d["unmatched_count"].as_u64()), (Some(3), Some(2), Some(1)), "{d}");
    // The summary lists the cells LoTW filled: Japan and the US, mixed/mode/band each, with WAS for CT.
    let new: Vec<(String, String, String)> = d["new_awards"].as_array().unwrap().iter().map(|n| (n["award"].as_str().unwrap().into(), n["name"].as_str().unwrap().into(), n["column"].as_str().unwrap().into())).collect();
    assert!(new.contains(&("dxcc".into(), "Japan".into(), "mixed".into())), "{d}");
    assert!(new.contains(&("dxcc".into(), "United States".into(), "20m".into())), "{d}");
    assert!(new.contains(&("was".into(), "Connecticut".into(), "mixed".into())), "{d}");
    assert!(d["new_awards"].as_array().unwrap().iter().all(|n| n["before"]["lotw"] == false), "{d}");
    assert_eq!(d["auto"], false);
    let ja = api.fields(log, "JA1XYZ").await;
    assert_eq!((ja["LOTW_QSL_RCVD"].as_str(), ja["LOTW_QSLRDATE"].as_str()), (Some("Y"), Some("20260105")));
    assert_eq!(api.fields(log, "W1AW").await["STATE"], "CT", "blank state filled from LoTW");
    assert_eq!(api.get("/qsl").await["config"]["lotw_rcvd_since"], "2026-01-07");
    // Downloading again changes nothing.
    let again = api.post("/qsl/download/lotw", json!(null)).await;
    assert_eq!((again["confirmed"].as_u64(), again["new_awards"].as_array().unwrap().len()), (Some(0), 0));

    let dxcc = api.get(&format!("/logs/{log}/awards/dxcc?lotw=true&paper=true&unworked=true")).await;
    assert!(has_cell(&dxcc, "339", "mixed", "confirmed") && has_cell(&dxcc, "339", "cw", "confirmed"), "{dxcc}");
    assert!(has_cell(&dxcc, "230", "mixed", "worked"), "{dxcc}");
    assert_eq!(dxcc["columns"][0]["confirmed"], 2);
    let was = api.get(&format!("/logs/{log}/awards/was?lotw=true")).await;
    assert!(has_cell(&was, "CT", "20m", "confirmed"), "{was}");
    assert_eq!(was["total"], 50);
    // Not counting LoTW leaves them worked.
    let dxcc = api.get(&format!("/logs/{log}/awards/dxcc?paper=true")).await;
    assert_eq!(dxcc["columns"][0]["confirmed"], 0);

    // eQSL: upload one QSO, then download the inbox.
    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["eqsl_username"] = json!("N0CALL");
    cfg["eqsl_calls"] = json!(["N0CALL"]);
    cfg["eqsl_since"] = json!("2025-01-01");
    api.put("/qsl", json!({"config": cfg, "secrets": {"eqsl_password": "eqslpw"}})).await;
    let run = api.post("/qsl/upload/eqsl", json!(null)).await;
    assert_eq!(run["uploaded"], 3, "{run}");
    assert!(uploads.lock().unwrap()[0].contains("EQSL_USER"));
    assert_eq!(api.fields(log, "DL1BAD").await["EQSL_QSL_SENT"], "Y");
    let d = api.post("/qsl/download/eqsl", json!(null)).await;
    assert_eq!(d["confirmed"], 1, "{d}");
    let germany = d["new_awards"].as_array().unwrap().iter().find(|n| n["award"] == "dxcc" && n["column"] == "mixed").unwrap();
    assert_eq!((germany["name"].as_str(), germany["before"]["lotw"].as_bool()), (Some("Germany"), Some(false)), "{d}");
    let dl = api.fields(log, "DL1BAD").await;
    assert_eq!((dl["EQSL_QSL_RCVD"].as_str(), dl["GRIDSQUARE"].as_str()), (Some("Y"), Some("JO62")));
    let dxcc = api.get(&format!("/logs/{log}/awards/dxcc?eqsl=true")).await;
    assert!(has_cell(&dxcc, "230", "mixed", "confirmed"), "{dxcc}");

    // Paper cards: queue, list, mark received.
    let id = api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"call": "DL1BAD"}, "offset": 0, "limit": 1})).await["rows"][0]["id"].as_i64().unwrap();
    api.post("/qsos/mark", json!({"ids": [id], "fields": {"QSL_SENT": "Q"}})).await;
    let queue = api.get(&format!("/logs/{log}/paper-queue")).await;
    assert_eq!(queue.as_array().unwrap().len(), 1);
    assert_eq!(queue[0]["fields"]["CALL"], "DL1BAD");
    api.post("/qsos/mark", json!({"ids": [id], "fields": {"QSL_SENT": "Y", "QSL_RCVD": "Y"}})).await;
    assert_eq!(api.get(&format!("/logs/{log}/paper-queue")).await.as_array().unwrap().len(), 0);
    let dxcc = api.get(&format!("/logs/{log}/awards/dxcc?paper=true")).await;
    assert!(has_cell(&dxcc, "230", "mixed", "confirmed"), "{dxcc}");
    // Only QSL fields can be set in bulk.
    let resp = api.http.post(format!("{}/qsos/mark", api.base)).header("x-qrzero-token", &api.token).json(&json!({"ids": [id], "fields": {"CALL": "X"}})).send().await.unwrap();
    assert_ne!(resp.status(), 200);
}

/// [(column, status)] of one award in an award-hints answer.
fn hint_cells(hints: &Value, award: &str) -> Vec<(String, String)> {
    let h = hints.as_array().unwrap().iter().find(|h| h["award"] == award).unwrap_or_else(|| panic!("no {award} in {hints}"));
    h["cells"].as_array().unwrap().iter().map(|c| (c["column"].as_str().unwrap().to_string(), c["status"].as_str().unwrap().to_string())).collect()
}

fn cells(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[tokio::test]
async fn award_hints_follow_the_log() {
    let api = Api::new(QslEndpoints::default()).await;
    let (log, loc) = api.setup().await;
    let hints = |q: &str| {
        let path = format!("/logs/{log}/award-hints?lotw=true&paper=true&{q}");
        let api = &api;
        async move { api.get(&path).await }
    };

    // Empty log: everything is new; DXCC and zone come from the country file.
    let h = hints("call=W8ABC&band=20m&mode=CW&state=OH").await;
    let keys: Vec<(&str, &str)> = h.as_array().unwrap().iter().map(|h| (h["award"].as_str().unwrap(), h["key"].as_str().unwrap())).collect();
    assert_eq!(keys, [("dxcc", "291"), ("was", "OH"), ("waz", "5"), ("wpx", "W8"), ("wac", "NA"), ("itu", "8")], "{h}");
    assert_eq!(h[0]["name"], "United States");
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "new"), ("cw", "new"), ("20m", "new")]));

    // After a QSO on 20m CW: worked there, new on 40m and in phone.
    let id = api.qso(log, loc, "JA1XYZ").await;
    let h = hints("call=JA2AAA&band=20m&mode=CW").await;
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "worked"), ("cw", "worked"), ("20m", "worked")]), "{h}");
    assert_eq!(hint_cells(&h, "wpx"), cells(&[("mixed", "new"), ("cw", "new"), ("20m", "new")]));
    let h = hints("call=JA1XYZ&band=40m&mode=USB").await;
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "worked"), ("phone", "new"), ("40m", "new")]));
    assert_eq!(hint_cells(&h, "waz"), cells(&[("mixed", "worked"), ("phone", "new"), ("40m", "new")]));

    // A paper card received confirms it, but only when cards are counted.
    api.post("/qsos/mark", json!({"ids": [id], "fields": {"QSL_RCVD": "Y"}})).await;
    let h = hints("call=JA1XYZ&band=20m&mode=CW").await;
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "confirmed"), ("cw", "confirmed"), ("20m", "confirmed")]), "{h}");
    let h = api.get(&format!("/logs/{log}/award-hints?lotw=true&call=JA1XYZ&band=20m&mode=CW")).await;
    assert_eq!(hint_cells(&h, "dxcc")[0].1, "worked");

    // Edits count: the QSO moved to 15m frees up 20m.
    let mut f = api.fields(log, "JA1XYZ").await;
    f["BAND"] = json!("15m");
    f["FREQ"] = json!("21.025");
    api.put(&format!("/qsos/{id}"), json!({"location_id": loc, "fields": f})).await;
    let h = hints("call=JA1XYZ&band=20m&mode=CW").await;
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "confirmed"), ("cw", "confirmed"), ("20m", "new")]), "{h}");

    // Logging keeps the kept cells current.
    api.qso(log, loc, "DL1ABC").await;
    let h = hints("call=DL2XX&band=20m&mode=CW").await;
    assert_eq!(h[0]["name"], "Germany");
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "worked"), ("cw", "worked"), ("20m", "worked")]), "{h}");
    assert_eq!(hint_cells(&h, "waz")[0].1, "worked");
    // No band column (1.25m) and no mode: just the mixed cell.
    let h = hints("call=DL2XX&band=1.25m").await;
    assert_eq!(hint_cells(&h, "dxcc"), cells(&[("mixed", "worked")]));
}

#[tokio::test]
async fn qrz_and_clublog_confirmations() {
    let api = Api::new(mock(Arc::default()).await).await;
    let (log, loc) = api.setup().await;
    api.qso(log, loc, "JA1XYZ").await;
    api.qso(log, loc, "W1AW").await;
    api.qso(log, loc, "DL1BAD").await;

    // Nothing to download from until the service is set up.
    let d = api.post("/qsl/download/qrz", json!(null)).await;
    assert!(d["error"].as_str().unwrap().contains("API key"), "{d}");

    let mut cfg = api.get("/qsl").await["config"].clone();
    cfg["qrz_calls"] = json!(["N0CALL"]);
    cfg["clublog_calls"] = json!(["N0CALL"]);
    cfg["clublog_email"] = json!("op@example.com");
    api.put("/qsl", json!({"config": cfg, "secrets": {"qrz_keys": {"N0CALL": "QRZ-KEY"}, "clublog_password": "clpw", "clublog_app_key": "appkey"}})).await;

    let d = api.post("/qsl/download/qrz", json!(null)).await;
    assert_eq!((d["received"].as_u64(), d["confirmed"].as_u64()), (Some(1), Some(1)), "{d}");
    let ja = api.fields(log, "JA1XYZ").await;
    assert_eq!(ja["QRZCOM_QSO_DOWNLOAD_STATUS"], "Y");
    assert_eq!(ja["QRZCOM_QSO_UPLOAD_STATUS"], "Y", "a confirmed QSO is on QRZ");
    assert_eq!(ja["QRZCOM_QSO_DOWNLOAD_DATE"].as_str().unwrap().len(), 8);
    assert!(api.fields(log, "W1AW").await.get("QRZCOM_QSO_DOWNLOAD_STATUS").is_none());

    // The match for VK2XYZ has no mode and is dropped; ZZ9ZZ isn't in the log.
    let d = api.post("/qsl/download/clublog", json!(null)).await;
    assert_eq!((d["received"].as_u64(), d["confirmed"].as_u64(), d["unmatched_count"].as_u64()), (Some(2), Some(1), Some(1)), "{d}");
    let w = api.fields(log, "W1AW").await;
    assert_eq!(w["APP_QRZERO_CLUBLOG_RCVD"], "Y");
    assert_eq!(w["CLUBLOG_QSO_UPLOAD_STATUS"], "Y");
    assert!(api.fields(log, "DL1BAD").await.get("APP_QRZERO_CLUBLOG_RCVD").is_none());
    assert!(d["new_awards"].as_array().unwrap().is_empty(), "only LoTW and eQSL count toward awards");

    // A wrong password is reported, not swallowed.
    api.put("/qsl", json!({"config": api.get("/qsl").await["config"].clone(), "secrets": {"clublog_password": "wrong"}})).await;
    let d = api.post("/qsl/download/clublog", json!(null)).await;
    assert!(d["error"].as_str().unwrap().contains("application password"), "{d}");
}
