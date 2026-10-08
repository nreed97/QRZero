//! The user's own UDP connections (QSO logged, rotator, lookup, relay) and startup programs.

use std::time::Duration;

use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::net::UdpSocket;

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

    async fn call(&self, method: reqwest::Method, path: &str, body: Value) -> (u16, Value) {
        let resp = self.http.request(method, format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token).json(&body).send().await.unwrap();
        let status = resp.status().as_u16();
        (status, serde_json::from_str(&resp.text().await.unwrap()).unwrap_or(Value::Null))
    }

    async fn ok(&self, method: reqwest::Method, path: &str, body: Value) -> Value {
        let (status, v) = self.call(method, path, body).await;
        assert_eq!(status, 200, "{path}: {v}");
        v
    }

    async fn get(&self, path: &str) -> Value {
        let resp = self.http.get(format!("{}{}", self.base, path)).header("x-qrzero-token", &self.token).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        resp.json().await.unwrap()
    }
}

async fn listener() -> (UdpSocket, u16) {
    let s = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = s.local_addr().unwrap().port();
    (s, port)
}

async fn recv(s: &UdpSocket) -> Vec<u8> {
    let mut buf = vec![0u8; 65536];
    let (n, _) = tokio::time::timeout(Duration::from_secs(3), s.recv_from(&mut buf)).await.expect("nothing arrived").unwrap();
    buf.truncate(n);
    buf
}

#[tokio::test]
async fn connections_send_on_events() {
    let api = Api::new().await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    let loc = api.ok(reqwest::Method::POST, &format!("/logs/{log}/locations"), json!({"name": "Home", "fields": {"MY_GRIDSQUARE": "FN31"}})).await["id"].as_i64().unwrap();
    api.ok(reqwest::Method::POST, "/station/active", json!({"log_id": log, "location_id": loc, "station_callsign": "N0CALL"})).await;
    let (qso_rx, qso_port) = listener().await;
    let (rot_rx, rot_port) = listener().await;
    let (look_rx, look_port) = listener().await;
    let (off_rx, off_port) = listener().await;
    let saved = api
        .ok(
            reqwest::Method::PUT,
            "/udp-connections",
            json!([
                {"name": "Logged", "host": "127.0.0.1", "port": qso_port, "event": "qso_logged", "format": "template", "template": "{CALL} {BAND} {MODE} {RST_SENT}\\r\\n"},
                {"name": "Rotor", "host": "localhost", "port": rot_port, "event": "rotator", "format": "pst"},
                {"name": "Entered", "host": "127.0.0.1", "port": look_port, "event": "lookup", "format": "json"},
                {"name": "Off", "enabled": false, "host": "127.0.0.1", "port": off_port, "event": "qso_logged", "format": "adif"},
            ]),
        )
        .await;
    let conns = saved["connections"].as_array().unwrap();
    assert_eq!(conns.len(), 4);
    assert!(conns.iter().all(|c| c["id"].as_u64().unwrap() > 0), "{saved}");
    assert_eq!(api.get("/udp-connections").await["connections"], saved["connections"], "kept");

    // Logging a QSO from the QSO panel.
    let fields = json!({"CALL": "DL1ABC", "QSO_DATE": "20240101", "TIME_ON": "1200", "BAND": "20m", "MODE": "SSB", "RST_SENT": "59"});
    api.ok(reqwest::Method::POST, &format!("/logs/{log}/qsos"), json!({"location_id": loc, "fields": fields})).await;
    assert_eq!(recv(&qso_rx).await, b"DL1ABC 20m SSB 59\r\n");
    let mut buf = [0u8; 64];
    assert!(tokio::time::timeout(Duration::from_millis(300), off_rx.recv_from(&mut buf)).await.is_err(), "disabled connection sent");

    // Turning the rotator works with only a UDP connection set up (PstRotatorAz itself off).
    api.ok(reqwest::Method::POST, "/rotator", json!({"azimuth": 271.6})).await;
    assert_eq!(recv(&rot_rx).await, b"<PST><AZIMUTH>272</AZIMUTH></PST>");

    // Entering a call: the heading comes from the location's grid to the station's country.
    let cty = "JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;\n";
    let resp = api.http.post(format!("{}/cty", api.base)).header("x-qrzero-token", &api.token).body(cty).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    api.get(&format!("/logs/{log}/lookup/JA1XYZ")).await;
    let msg: Value = serde_json::from_slice(&recv(&look_rx).await).unwrap();
    assert_eq!((msg["event"].as_str(), msg["call"].as_str()), (Some("lookup"), Some("JA1XYZ")));
    let az = msg["azimuth"].as_u64().unwrap();
    assert!((320..=345).contains(&az), "New England to Japan is north-west: {az}");

    let status = api.get("/udp-connections").await["status"].clone();
    let id = conns[0]["id"].to_string();
    assert_eq!(status[&id]["ok"], true, "{status}");
    assert!(status[&id]["text"].as_str().unwrap().contains("last sent"));
}

#[tokio::test]
async fn send_test_and_errors() {
    let api = Api::new().await;
    let (rx, port) = listener().await;
    let conn = json!({"name": "Switch", "host": "127.0.0.1", "port": port, "event": "radio", "format": "n1mm_radio"});
    let r = api.ok(reqwest::Method::POST, "/udp-connections/test", conn).await;
    let got = String::from_utf8(recv(&rx).await).unwrap();
    assert_eq!(r["sent"].as_str().unwrap(), got);
    assert!(got.contains("<RadioInfo>") && got.contains("<Freq>1407400</Freq>"), "{got}");

    let conn = json!({"host": "127.0.0.1", "port": port, "event": "qso_logged", "format": "n1mm_contact"});
    api.ok(reqwest::Method::POST, "/udp-connections/test", conn).await;
    assert!(String::from_utf8(recv(&rx).await).unwrap().contains("<call>W1AW</call>"));

    // PstRotatorAz format on an event without a heading, and an empty template, are refused.
    for bad in [
        json!({"host": "127.0.0.1", "port": port, "event": "qso_logged", "format": "pst"}),
        json!({"host": "127.0.0.1", "port": port, "event": "radio", "format": "template", "template": ""}),
        json!({"host": "", "port": port, "event": "radio", "format": "json"}),
    ] {
        let (status, v) = api.call(reqwest::Method::POST, "/udp-connections/test", bad).await;
        assert_eq!(status, 400, "{v}");
        assert!(v["error"].is_string());
    }
}

#[tokio::test]
async fn relay_passes_wsjtx_packets_on_unchanged() {
    let api = Api::new().await;
    let wsjtx_port = UdpSocket::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();
    let mut cfg = api.get("/integrations").await["config"].clone();
    cfg["wsjtx_listen"] = json!(format!("127.0.0.1:{wsjtx_port}"));
    api.ok(reqwest::Method::PUT, "/integrations", cfg).await;
    let (a, a_port) = listener().await;
    let (b, b_port) = listener().await;
    let saved = api
        .ok(
            reqwest::Method::PUT,
            "/udp-connections",
            json!([
                {"name": "GridTracker", "host": "127.0.0.1", "port": a_port, "event": "relay_wsjtx"},
                {"name": "JTAlert", "host": "127.0.0.1", "port": b_port, "event": "relay_wsjtx"},
                // Pointing a relay back at QRZero's own port is ignored rather than looping.
                {"name": "Loop", "host": "127.0.0.1", "port": wsjtx_port, "event": "relay_wsjtx"},
            ]),
        )
        .await;
    let id = saved["connections"][0]["id"].to_string();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let wsjtx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    // A WSJT-X heartbeat: magic, schema 2, type 0, id "WSJT-X", max schema 3, version, revision.
    let mut packet = vec![0xAD, 0xBC, 0xCB, 0xDA, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 6];
    packet.extend(b"WSJT-X");
    packet.extend([0, 0, 0, 3, 0, 0, 0, 5]);
    packet.extend(b"2.7.0");
    packet.extend([0, 0, 0, 0]);
    wsjtx.send_to(&packet, ("127.0.0.1", wsjtx_port)).await.unwrap();
    assert_eq!(recv(&a).await, packet);
    assert_eq!(recv(&b).await, packet);
    let status = api.get("/udp-connections").await["status"].clone();
    assert!(status[&id]["text"].as_str().unwrap().contains("1 packet passed on"), "{status}");
}

#[cfg(unix)]
#[tokio::test]
async fn startup_programs_save_and_launch() {
    let api = Api::new().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("ran.txt");
    let saved = api
        .ok(
            reqwest::Method::PUT,
            "/startup-apps",
            json!([
                {"path": "/bin/sh", "args": format!("-c \"echo hi > '{}'\"", out.display()), "enabled": true},
                {"path": "   "},
            ]),
        )
        .await;
    let apps = saved["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 1, "blank paths are dropped: {saved}");
    assert_eq!(apps[0]["skip_if_running"], true);
    let st = api.ok(reqwest::Method::POST, "/startup-apps/launch", apps[0].clone()).await;
    assert_eq!(st["ok"], true, "{st}");
    for _ in 0..100 {
        if std::fs::read_to_string(&out).is_ok_and(|t| t.trim() == "hi") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "hi");
    let id = apps[0]["id"].to_string();
    assert_eq!(api.get("/startup-apps").await["status"][&id]["ok"], true);

    let st = api.ok(reqwest::Method::POST, "/startup-apps/launch", json!({"id": 9, "path": "/no/such/qrzero-program"})).await;
    assert_eq!(st["ok"], false);
    assert!(st["text"].as_str().unwrap().contains("couldn't start"));

    // Only with the session token.
    let resp = api.http.post(format!("{}/startup-apps/launch", api.base)).json(&apps[0]).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}
