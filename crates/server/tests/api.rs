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
                    Some("K5BOTH") => "<QRZDatabase><Callsign><call>K5BOTH</call><fname>From</fname><name>QRZ</name></Callsign><Session><Key>abc</Key></Session></QRZDatabase>".to_string(),
                    // The same call with the grid HamQTH is missing.
                    Some("K7GAP") => "<QRZDatabase><Callsign><call>K7GAP</call><fname>Patricia</fname><name>Ray</name><grid>DN13</grid><state>ID</state></Callsign><Session><Key>abc</Key></Session></QRZDatabase>".to_string(),
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

/// A stand-in for hamqth.com's xml.php. It knows G0ABC, which the QRZ stand-in doesn't,
/// so a lookup that falls through from QRZ can be told apart.
async fn mock_hamqth(lookups: Arc<AtomicUsize>) -> String {
    let app = Router::new().route(
        "/xml.php",
        get(move |Query(q): Query<HashMap<String, String>>| {
            let lookups = lookups.clone();
            async move {
                if q.contains_key("u") {
                    if q["p"] == "good" {
                        return "<HamQTH><session><session_id>sid</session_id></session></HamQTH>".to_string();
                    }
                    return "<HamQTH><session><error>Wrong user name or password</error></session></HamQTH>".to_string();
                }
                lookups.fetch_add(1, Ordering::SeqCst);
                match q.get("callsign").map(String::as_str) {
                    Some("G0ABC") => "<HamQTH><search><callsign>G0ABC</callsign><nick>Jo</nick><qth>Bath</qth><adif>223</adif><grid>IO81</grid></search></HamQTH>".to_string(),
                    Some("W1AW") => "<HamQTH><search><callsign>W1AW</callsign><nick>HamQTH Hiram</nick></search></HamQTH>".to_string(),
                    Some("K5BOTH") => "<HamQTH><search><callsign>K5BOTH</callsign><nick>From HamQTH</nick></search></HamQTH>".to_string(),
                    // Known to HamQTH with no grid, so QRZ can top it up.
                    Some("K7GAP") => "<HamQTH><search><callsign>K7GAP</callsign><nick>Pat</nick><qth>Boise</qth></search></HamQTH>".to_string(),
                    _ => "<HamQTH><session><error>Callsign not found</error></session></HamQTH>".to_string(),
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/xml.php")
}

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Api {
    async fn new(qrz_endpoint: String) -> Self {
        Api::with_services(qrz_endpoint, None).await
    }

    async fn with_services(qrz_endpoint: String, hamqth_endpoint: Option<String>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_path_buf());
        cfg.qrz_endpoint = qrz_endpoint;
        if let Some(h) = hamqth_endpoint {
            cfg.hamqth_endpoint = h;
        }
        // Keep test passwords away from the real credential store entries.
        // Tests run in parallel and the Windows credential store is shared, so each server gets its own entries.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        cfg.secret_service = format!("QRZero-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
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
    assert_eq!(ui.headers()["x-frame-options"], "DENY");
    assert_eq!(ui.headers()["x-content-type-options"], "nosniff");
}

#[tokio::test]
async fn other_host_names_are_refused() {
    let api = Api::new(String::new()).await;
    let ask = |host: &'static str| {
        let (http, url, token) = (api.http.clone(), format!("{}/logs", api.base), api.token.clone());
        async move { http.get(url).header("host", host).header("x-qrzero-token", token).send().await.unwrap().status().as_u16() }
    };
    assert_eq!(ask("localhost:8073").await, 200);
    assert_eq!(ask("[::1]:8073").await, 200);
    assert_eq!(ask("rebind.example:8073").await, 403);
    assert_eq!(ask("127.0.0.1.example").await, 403);
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
async fn dx_marathon_counts_one_year() {
    let api = Api::new(String::new()).await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    for (call, date, dxcc, cqz) in [("W1AW", "20250601", "291", "5"), ("K1ABC", "20260102", "291", "5"), ("JA1XYZ", "20260103", "339", "25"), ("K2ABC", "20260104", "291", "5")] {
        let fields = json!({"CALL": call, "QSO_DATE": date, "TIME_ON": "1200", "BAND": "20m", "MODE": "CW", "DXCC": dxcc, "CQZ": cqz});
        api.post(&format!("/logs/{log}/qsos"), json!({"fields": fields})).await;
    }
    // The country file may or may not be loaded here, so check counts and calls, not country keys.
    let m = api.get(&format!("/logs/{log}/marathon?year=2026")).await;
    assert_eq!((m["year"].as_i64(), m["score"].as_u64()), (Some(2026), Some(4)), "{m}");
    assert_eq!(m["countries"].as_array().unwrap().iter().map(|h| h["call"].as_str().unwrap()).collect::<Vec<_>>(), ["K1ABC", "JA1XYZ"]);
    assert_eq!(m["zones"][0]["date"], "2026-01-02");
    assert_eq!(m["missing_zones"].as_array().unwrap().len(), 38);
    let years: Vec<i64> = m["years"].as_array().unwrap().iter().filter_map(Value::as_i64).collect();
    assert!(years.contains(&2026) && years.contains(&2025) && years.windows(2).all(|w| w[0] > w[1]), "{years:?}");
    let last = api.get(&format!("/logs/{log}/marathon?year=2025")).await;
    assert_eq!(last["score"], 2);
    assert_eq!(api.get(&format!("/logs/{log}/marathon?year=2025&calls=N0NONE")).await["score"], 0);
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
    assert!(r["error"].as_str().unwrap().contains("Settings > Callsign lookup"));
    // Unchanged credentials aren't sent to QRZ again on every lookup...
    let before = logins.load(Ordering::SeqCst);
    api.get(&format!("/logs/{log}/lookup/K1ABC")).await;
    api.get(&format!("/logs/{log}/lookup/K2ABC")).await;
    assert_eq!(logins.load(Ordering::SeqCst), before);

    // ...but saving a new password retries at once.
    let (_, s) = api.call(reqwest::Method::PUT, "/settings", Some(json!({"qrz_password": "good"}))).await;
    assert_eq!(s["qrz_password_set"], true);
    let r = api.get(&format!("/logs/{log}/lookup/w1aw")).await;
    assert_eq!(r["station"]["NAME"], "Hiram Maxim");
    assert_eq!(r["source"], "QRZ");
    assert_eq!(r["worked"]["dxcc"], 291);

    // Portable call falls back to the home call.
    let r = api.get(&format!("/logs/{log}/lookup/W1AW%2FP")).await;
    assert_eq!(r["station"]["QTH"], "Newington");
    // Nothing is cached: the same call is asked again, and answered again.
    let r = api.get(&format!("/logs/{log}/lookup/W1AW")).await;
    assert_eq!(r["source"], "QRZ");

    let r = api.get(&format!("/logs/{log}/lookup/ZZ9ZZ")).await;
    assert!(r["station"].is_null() && r["error"].is_null());

    // Clearing the password removes it from the credential store.
    let (_, s) = api.call(reqwest::Method::PUT, "/settings", Some(json!({"qrz_password": ""}))).await;
    assert_eq!(s["qrz_password_set"], false);
}

#[tokio::test]
async fn hamqth_lookup_and_order() {
    let hamqth_lookups = Arc::new(AtomicUsize::new(0));
    let api = Api::with_services(
        mock_qrz(Arc::new(AtomicUsize::new(0))).await,
        Some(mock_hamqth(hamqth_lookups.clone()).await),
    )
    .await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();

    // HamQTH alone answers, and says so.
    api.call(
        reqwest::Method::PUT,
        "/settings",
        Some(json!({"hamqth_enabled": true, "hamqth_username": "me", "hamqth_password": "good"})),
    )
    .await;
    let (status, _) = api.call(reqwest::Method::POST, "/settings/hamqth/test", None).await;
    assert_eq!(status, 200);
    let r = api.get(&format!("/logs/{log}/lookup/G0ABC")).await;
    assert_eq!(r["station"]["NAME"], "Jo");
    assert_eq!(r["station"]["GRIDSQUARE"], "IO81");
    assert_eq!(r["source"], "HamQTH");

    // Both on, QRZ first: QRZ answers for W1AW and HamQTH is never asked.
    api.call(
        reqwest::Method::PUT,
        "/settings",
        Some(json!({"qrz_enabled": true, "qrz_username": "me", "qrz_password": "good", "lookup_order": ["qrz", "hamqth"]})),
    )
    .await;
    let before = hamqth_lookups.load(Ordering::SeqCst);
    let r = api.get(&format!("/logs/{log}/lookup/W1AW")).await;
    assert_eq!(r["source"], "QRZ");
    assert_eq!(r["station"]["NAME"], "Hiram Maxim");
    assert_eq!(hamqth_lookups.load(Ordering::SeqCst), before);

    // A call QRZ doesn't know falls through to HamQTH.
    let r = api.get(&format!("/logs/{log}/lookup/G4XYZ")).await;
    assert!(r["station"].is_null() && r["error"].is_null());
    assert!(hamqth_lookups.load(Ordering::SeqCst) > before);

    // Turning the order round makes HamQTH answer for a call both know.
    let (_, s) = api.call(reqwest::Method::PUT, "/settings", Some(json!({"lookup_order": ["hamqth", "qrz"]}))).await;
    assert_eq!(s["lookup_order"][0], "hamqth");
    assert_eq!(s["lookup_order"][1], "qrz");
    let r = api.get(&format!("/logs/{log}/lookup/K5BOTH")).await;
    assert_eq!(r["source"], "HamQTH");
    assert_eq!(r["station"]["NAME"], "From HamQTH");
}

#[tokio::test]
async fn a_broken_service_doesnt_stop_the_next_one() {
    let api = Api::with_services(
        mock_qrz(Arc::new(AtomicUsize::new(0))).await,
        Some(mock_hamqth(Arc::new(AtomicUsize::new(0))).await),
    )
    .await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    // QRZ first but with a password it will refuse; HamQTH second and working.
    api.call(
        reqwest::Method::PUT,
        "/settings",
        Some(json!({
            "qrz_enabled": true, "qrz_username": "me", "qrz_password": "bad",
            "hamqth_enabled": true, "hamqth_username": "me", "hamqth_password": "good",
            "lookup_order": ["qrz", "hamqth"]
        })),
    )
    .await;
    let r = api.get(&format!("/logs/{log}/lookup/G0ABC")).await;
    assert_eq!(r["source"], "HamQTH", "QRZ refusing the login must not stop HamQTH answering");
    assert_eq!(r["station"]["NAME"], "Jo");
    assert!(r["error"].is_null());

    // When nobody can answer, the reason from the first service is reported.
    let r = api.get(&format!("/logs/{log}/lookup/ZZ9ZZ")).await;
    assert!(r["station"].is_null());
    assert!(r["error"].as_str().unwrap().contains("QRZ"));
}

#[tokio::test]
async fn lookup_on_entry_defaults_on_and_saves() {
    let api = Api::new(mock_qrz(Arc::new(AtomicUsize::new(0))).await).await;
    let s = api.get("/settings").await;
    assert_eq!(s["lookup_on_entry"], true);
    assert_eq!(s["lookup_order"][0], "qrz");
    let (_, s) = api.call(reqwest::Method::PUT, "/settings", Some(json!({"lookup_on_entry": false}))).await;
    assert_eq!(s["lookup_on_entry"], false);
    let s = api.get("/settings").await;
    assert_eq!(s["lookup_on_entry"], false);
}

#[tokio::test]
async fn an_explicit_fill_tops_up_from_the_next_service() {
    let api = Api::with_services(
        mock_qrz(Arc::new(AtomicUsize::new(0))).await,
        Some(mock_hamqth(Arc::new(AtomicUsize::new(0))).await),
    )
    .await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    api.call(
        reqwest::Method::PUT,
        "/settings",
        Some(json!({
            "qrz_enabled": true, "qrz_username": "me", "qrz_password": "good",
            "hamqth_enabled": true, "hamqth_username": "me", "hamqth_password": "good",
            "lookup_order": ["hamqth", "qrz"]
        })),
    )
    .await;

    // Typing a call uses the first service that knows it, and stops there.
    let r = api.get(&format!("/logs/{log}/lookup/K7GAP")).await;
    assert_eq!(r["source"], "HamQTH");
    assert_eq!(r["station"]["NAME"], "Pat");
    assert!(r["station"]["GRIDSQUARE"].is_null(), "the automatic lookup must not ask the second service: {r}");

    // Asking for a fill goes on to QRZ for the grid HamQTH didn't have, and keeps
    // HamQTH's answer for everything it did have.
    let r = api.get(&format!("/logs/{log}/lookup/K7GAP?fill=true")).await;
    assert_eq!(r["station"]["GRIDSQUARE"], "DN13");
    assert_eq!(r["station"]["STATE"], "ID");
    assert_eq!(r["station"]["NAME"], "Pat", "the first service's answer wins where it had one");
    assert_eq!(r["source"], "HamQTH + QRZ");

    // A complete first answer still costs one service.
    let r = api.get(&format!("/logs/{log}/lookup/G0ABC?fill=true")).await;
    assert_eq!(r["source"], "HamQTH");
}

#[tokio::test]
async fn hamqth_alone_fills_the_editor_and_right_click_paths() {
    let api = Api::with_services(
        mock_qrz(Arc::new(AtomicUsize::new(0))).await,
        Some(mock_hamqth(Arc::new(AtomicUsize::new(0))).await),
    )
    .await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    // HamQTH only: QRZ is left off entirely.
    api.call(
        reqwest::Method::PUT,
        "/settings",
        Some(json!({"hamqth_enabled": true, "hamqth_username": "me", "hamqth_password": "good"})),
    )
    .await;

    // The editor's Fill button: GET the call, which must answer from HamQTH.
    let r = api.get(&format!("/logs/{log}/lookup/G0ABC?fill=true")).await;
    assert_eq!(r["source"], "HamQTH");
    assert_eq!(r["station"]["NAME"], "Jo");

    // The log's right-click Fill: POST /qsos/lookup, which writes the QSO.
    let qso = json!({"fields": {"CALL": "G0ABC", "QSO_DATE": "20240101", "TIME_ON": "1200", "BAND": "20m", "MODE": "CW"}});
    let id = api.post(&format!("/logs/{log}/qsos"), qso).await["id"].as_i64().unwrap();
    let r = api.post("/qsos/lookup", json!({"ids": [id]})).await;
    assert_eq!(r["updated"], 1, "{r}");
    let rows = api.post(&format!("/logs/{log}/qsos/search"), json!({"filter": {"call": "G0ABC"}})).await;
    let f = &rows["rows"][0]["fields"];
    assert_eq!(f["NAME"], "Jo");
    assert_eq!(f["GRIDSQUARE"], "IO81");

    // A call nobody knows reads as unknown, not as a service being broken.
    let qso = json!({"fields": {"CALL": "ZZ9ZZ", "QSO_DATE": "20240101", "TIME_ON": "1300", "BAND": "20m", "MODE": "CW"}});
    let id = api.post(&format!("/logs/{log}/qsos"), qso).await["id"].as_i64().unwrap();
    let r = api.post("/qsos/lookup", json!({"ids": [id]})).await;
    assert_eq!(r["updated"], 0);
    let err = r["errors"][0].as_str().unwrap();
    assert!(err.contains("knows this call"), "{err}");
    assert!(!err.contains("couldn't ask"), "an unknown call must not read as an unreachable service: {err}");
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

    // With every service off it says so rather than doing nothing.
    let r = api.post("/qsos/lookup", json!({"ids": [a]})).await;
    assert_eq!(r["updated"], 0);
    assert!(r["errors"][0].as_str().unwrap().contains("QRZ or HamQTH"), "{r}");

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

#[tokio::test]
async fn a_second_log_sees_the_same_locations_and_equipment() {
    let api = Api::new(String::new()).await;
    let personal = api.get("/logs").await[0]["id"].as_i64().unwrap();
    let club = api.post("/logs", json!({"name": "Club"})).await["id"].as_i64().unwrap();
    let loc = api.post(&format!("/logs/{personal}/locations"), json!({"name": "Home", "fields": {}})).await["id"].as_i64().unwrap();
    api.post(&format!("/locations/{loc}/equipment"), json!({"kind": "rig", "name": "K3", "fields": {}})).await;

    let locs = api.get(&format!("/logs/{club}/locations")).await;
    assert_eq!(locs[0]["name"], "Home");
    let gear = api.get(&format!("/logs/{club}/equipment")).await;
    assert_eq!(gear[0]["name"], "K3");
}
