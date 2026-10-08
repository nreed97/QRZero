//! The watch list: entries over the API, alerts from cluster spots and FTx decodes.

use std::time::Duration;

use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;

const CTY: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
VP8,Falkland Islands,141,SA,13,16,-51.63,58.72,4.0,VP8 VQ8;
";

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    dir: tempfile::TempDir,
}

impl Api {
    async fn new(dir: tempfile::TempDir) -> Self {
        let mut cfg = Config::local(dir.path().to_path_buf());
        cfg.secret_service = format!("QRZero-test-{}", std::process::id());
        cfg.update_cty = false;
        let running = start(cfg).await.unwrap();
        Api { base: format!("http://{}/api", running.addr), token: running.token, http: reqwest::Client::new(), dir }
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>, raw: Option<&str>) -> (u16, Value) {
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
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn ok(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Value {
        let (status, v) = self.send(method, path, body, None).await;
        assert_eq!(status, 200, "{path}: {v}");
        v
    }

    async fn get(&self, path: &str) -> Value {
        self.ok(reqwest::Method::GET, path, None).await
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

    async fn events(&self) -> mpsc::UnboundedReceiver<Value> {
        let mut resp = self.http.get(format!("{}/events", self.base)).header("x-qrzero-token", &self.token).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut buf = String::new();
            while let Ok(Some(chunk)) = resp.chunk().await {
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(end) = buf.find("\n\n") {
                    let event: String = buf.drain(..end + 2).collect();
                    for line in event.lines().filter_map(|l| l.strip_prefix("data:")) {
                        if let Ok(v) = serde_json::from_str(line.trim()) {
                            let _ = tx.send(v);
                        }
                    }
                }
            }
        });
        rx
    }
}

async fn next_hit(rx: &mut mpsc::UnboundedReceiver<Value>) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let v = rx.recv().await.expect("event stream ended");
            if v["type"] == "watch_hit" {
                return v["hit"].clone();
            }
        }
    })
    .await
    .expect("no watch_hit event")
}

/// A cluster node that sends the given spot lines after login.
async fn mock_node(spots: &'static [&'static str]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let Ok((sock, _)) = listener.accept().await else { return };
        let (r, mut w) = sock.into_split();
        let mut lines = BufReader::new(r).lines();
        w.write_all(b"login: ").await.unwrap();
        let _ = lines.next_line().await;
        w.write_all(b"Hello N0CALL\r\n").await.unwrap();
        for s in spots {
            w.write_all(format!("{s}\r\n").as_bytes()).await.unwrap();
        }
        while let Ok(Some(_)) = lines.next_line().await {}
    });
    port
}

fn packet(kind: u32, id: &str) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend(0xadbc_cbdau32.to_be_bytes());
    p.extend(2u32.to_be_bytes());
    p.extend(kind.to_be_bytes());
    str_(&mut p, id);
    p
}

fn str_(p: &mut Vec<u8>, s: &str) {
    p.extend((s.len() as u32).to_be_bytes());
    p.extend(s.as_bytes());
}

fn status(dial: u64, mode: &str) -> Vec<u8> {
    let mut p = packet(1, "WSJT-X");
    p.extend(dial.to_be_bytes());
    for s in [mode, "", "", mode] {
        str_(&mut p, s);
    }
    p.extend([0, 0, 1]); // tx enabled, transmitting, decoding
    p.extend(1500u32.to_be_bytes());
    p.extend(1500u32.to_be_bytes());
    for s in ["N0CALL", "EN34", ""] {
        str_(&mut p, s);
    }
    p
}

fn decode(time_ms: u32, message: &str) -> Vec<u8> {
    let mut p = packet(2, "WSJT-X");
    p.push(1);
    p.extend(time_ms.to_be_bytes());
    p.extend((-10i32).to_be_bytes());
    p.extend(0.1f64.to_be_bytes());
    p.extend(1200u32.to_be_bytes());
    str_(&mut p, "~");
    str_(&mut p, message);
    p.extend([0, 0]);
    p
}

#[tokio::test]
async fn watch_list_routes_and_alerts() {
    let api = Api::new(tempfile::tempdir().unwrap()).await;
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    api.ok(reqwest::Method::POST, &format!("/logs/{log}/callsigns"), Some(json!({"callsign": "N0CALL"}))).await;
    assert_eq!(api.send(reqwest::Method::POST, "/cty", None, Some(CTY)).await.0, 200);
    api.ok(reqwest::Method::POST, "/station/active", Some(json!({"log_id": log, "location_id": null, "station_callsign": "N0CALL"}))).await;

    let entities = api.get("/cty/entities").await;
    let names: Vec<&str> = entities.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Falkland Islands", "Japan", "United States"]);
    assert_eq!(entities[1]["prefix"], "JA");

    assert_eq!(api.get("/watch").await, json!([]));
    let saved = api
        .ok(
            reqwest::Method::PUT,
            "/watch",
            Some(json!([
                {"kind": "call", "value": "w1aw", "note": "the club", "enabled": true},
                {"kind": "prefix", "value": "VP8", "bands": ["20m"], "enabled": true},
                {"kind": "entity", "value": "JA", "name": "Japan", "modes": ["DIGITAL"], "enabled": true},
                {"kind": "call", "value": "K9OFF", "enabled": false},
            ])),
        )
        .await;
    assert_eq!(saved[0]["id"], 1);
    assert_eq!(saved[0]["value"], "W1AW");
    assert_eq!(saved[3]["id"], 4);
    assert_eq!(api.get("/watch").await, saved);
    assert_eq!(api.get("/prefs/watch").await, saved, "kept as a preference");

    let mut rx = api.events().await;
    static SPOTS: &[&str] = &[
        "DX de K1TTT:     14025.0  W1AW/P       CW 25 dB                       1234Z",
        "DX de K1TTT:     14025.0  W1AW/P       CW again                       1234Z",
        "DX de W3LPL:     18080.0  VP8LP        CW                             1235Z",
        "DX de W3LPL:     14010.0  VP8LP        CW                             1236Z",
        "DX de W3LPL:     14012.0  JA1XYZ       CW                             1237Z",
        "DX de W3LPL:     14013.0  K9OFF        CW                             1238Z",
    ];
    let port = mock_node(SPOTS).await;
    api.ok(
        reqwest::Method::PUT,
        "/cluster",
        Some(json!({"nodes": [{"name": "Test", "host": "127.0.0.1", "port": port, "login": "", "password": "", "commands": []}], "auto_connect": false})),
    )
    .await;
    api.ok(reqwest::Method::POST, "/cluster/connect", Some(json!({"connect": true}))).await;

    let hit = next_hit(&mut rx).await;
    assert_eq!(hit["call"], "W1AW/P");
    assert_eq!(hit["entry_id"], 1);
    assert_eq!(hit["source"], "cluster");
    assert_eq!(hit["band"], "20m");
    assert_eq!(hit["mode"], "CW");
    assert_eq!(hit["freq_hz"], 14_025_000);
    assert_eq!(hit["note"], "the club");
    assert_eq!(hit["country"], "United States");
    let hit = next_hit(&mut rx).await;
    assert_eq!(hit["call"], "VP8LP", "the repeat W1AW/P spot is quiet, 17m VP8 is not watched");
    assert_eq!(hit["band"], "20m");

    let c = api.wait_for("/cluster", |v| v["spots"].as_array().is_some_and(|s| s.len() == SPOTS.len())).await;
    let watched: Vec<Value> = c["spots"].as_array().unwrap().iter().map(|s| s["watched"].clone()).collect();
    assert_eq!(watched, [json!(1), json!(1), Value::Null, json!(2), Value::Null, Value::Null], "repeats still marked; CW Japan and disabled entries are not");

    // FT8 from Japan matches the entity entry; CW didn't.
    let port = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut cfg = api.get("/integrations").await["config"].clone();
    cfg["wsjtx_listen"] = json!(format!("127.0.0.1:{port}"));
    api.ok(reqwest::Method::PUT, "/integrations", Some(cfg)).await;
    let wsjtx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server = format!("127.0.0.1:{port}");
    wsjtx.send_to(&status(14_074_000, "FT8"), &server).await.unwrap();
    wsjtx.send_to(&decode(45_015_000, "CQ K1ABC FN42"), &server).await.unwrap();
    wsjtx.send_to(&decode(45_015_000, "CQ JA1XYZ PM95"), &server).await.unwrap();
    let hit = next_hit(&mut rx).await;
    assert_eq!(hit["call"], "JA1XYZ");
    assert_eq!(hit["source"], "ftx");
    assert_eq!(hit["label"], "Japan");
    assert_eq!(hit["grid"], "PM95");
    assert_eq!(hit["freq_hz"], 14_075_200);
    let ftx = api.wait_for("/ftx", |v| v["decodes"].as_array().is_some_and(|d| d.len() == 2)).await;
    assert_eq!(ftx["decodes"][0]["watched"], Value::Null);
    assert_eq!(ftx["decodes"][1]["watched"], 3);

    let hits = api.get("/watch/hits").await;
    let calls: Vec<&str> = hits.as_array().unwrap().iter().map(|h| h["call"].as_str().unwrap()).collect();
    assert_eq!(calls, ["JA1XYZ", "VP8LP", "W1AW/P"], "newest first");

    // Bad lists are cleaned up; the list survives a restart.
    let saved = api.ok(reqwest::Method::PUT, "/watch", Some(json!([{"kind": "prefix", "value": " 3y0 "}, {"kind": "call", "value": ""}]))).await;
    assert_eq!(saved, json!([{"id": 1, "kind": "prefix", "value": "3Y0", "name": "", "bands": [], "modes": [], "note": "", "enabled": true}]));
    let (status, _) = api.send(reqwest::Method::PUT, "/watch", Some(json!({"nope": 1})), None).await;
    assert!(status >= 400);
    let Api { dir, .. } = api;
    let again = Api::new(dir).await;
    assert_eq!(again.get("/watch").await, saved);
}
