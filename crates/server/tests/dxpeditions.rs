//! The DXpedition list: a stand-in for NG3K's calendar, needs from the log, alerts from cluster spots.

use std::time::Duration;

use axum::routing::get;
use axum::Router;
use chrono::{Duration as Days, Utc};
use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const CTY: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
VP8,Falkland Islands,141,SA,13,16,-51.63,58.72,4.0,VP8 VQ8;
";

fn range(from: i64, to: i64) -> String {
    let (a, b) = (Utc::now() + Days::days(from), Utc::now() + Days::days(to));
    format!("{}-{}", a.format("%b %-d"), b.format("%b %-d, %Y"))
}

async fn calendar(page: String) -> String {
    let app = Router::new().route("/adxo", get(move || async move { page.clone() }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/adxo", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

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

#[tokio::test]
async fn dxpedition_list_needs_and_alerts() {
    let entry = |dates: String, entity: &str, call: &str| format!("<p>\n{dates}\n<br>DXCC: {entity}<br>Callsign: <strong><a href=\"https://www.qrz.com/db/{call}\">{call}</a></strong><br>QSL: LoTW<br>Info: test\n</p>\n");
    let page = format!(
        "<html><body><h1>Announced DX Operations</h1><p>\n<strong>October</strong>\n</p>\n{}{}{}{}</body></html>",
        entry(range(-1, 5), "Falkland Is.", "VP8LP"),
        entry(range(-1, 5), "Japan", "JA7XYZ"),
        entry(range(10, 15), "United States", "W2UPC"),
        entry(range(-30, -20), "United States", "K9OLD"),
    );
    let url = calendar(page).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
    cfg.dxped_url = url;
    let running = start(cfg).await.unwrap();
    let base = format!("http://{}/api", running.addr);
    let http = reqwest::Client::new();
    let call = |m: reqwest::Method, path: &str, body: Option<Value>| {
        let mut req = http.request(m, format!("{base}{path}")).header("x-qrzero-token", &running.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        async move {
            let r = req.send().await.unwrap();
            assert_eq!(r.status(), 200);
            r.json::<Value>().await.unwrap_or(Value::Null)
        }
    };
    let get = |p: &str| call(reqwest::Method::GET, p, None);

    let log = get("/logs").await[0]["id"].as_i64().unwrap();
    call(reqwest::Method::POST, &format!("/logs/{log}/callsigns"), Some(json!({"callsign": "N0CALL"}))).await;
    let resp = http.post(format!("{base}/cty")).header("x-qrzero-token", &running.token).body(CTY).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    call(reqwest::Method::POST, "/station/active", Some(json!({"log_id": log, "location_id": null, "station_callsign": "N0CALL"}))).await;
    call(
        reqwest::Method::POST,
        &format!("/logs/{log}/qsos"),
        Some(json!({"location_id": null, "fields": {"CALL": "JA1AAA", "QSO_DATE": "20240101", "TIME_ON": "1200", "BAND": "20m", "MODE": "CW"}})),
    )
    .await;

    assert_eq!(get("/dxpeditions").await["items"], json!([]), "nothing fetched yet");
    let mut list = Value::Null;
    for _ in 0..100 {
        list = call(reqwest::Method::POST, "/dxpeditions/refresh", None).await;
        let ja = list["items"].as_array().and_then(|i| i.iter().find(|x| x["call"] == "JA7XYZ")).cloned();
        if ja.is_some_and(|j| j["need"]["new_dxcc"] == false) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(list["error"], Value::Null, "{list}");
    let calls: Vec<&str> = list["items"].as_array().unwrap().iter().map(|i| i["call"].as_str().unwrap()).collect();
    assert_eq!(calls.len(), 3, "the finished one is left out: {calls:?}");
    let by = |c: &str| list["items"].as_array().unwrap().iter().find(|i| i["call"] == c).unwrap().clone();
    let vp8 = by("VP8LP");
    assert_eq!(vp8["active"], true);
    assert_eq!(vp8["entity"], "Falkland Islands");
    assert_eq!(vp8["need"]["new_dxcc"], true);
    let ja = by("JA7XYZ");
    assert_eq!(ja["need"]["new_dxcc"], false);
    assert!(ja["need"]["bands"].as_array().unwrap().iter().all(|b| b != "20m") && ja["need"]["bands"].as_array().unwrap().iter().any(|b| b == "17m"));
    assert_eq!(ja["need"]["modes"], json!(["PHONE", "DIGITAL"]));
    assert_eq!(by("W2UPC")["active"], false);

    // Hand-added calls join the list.
    let after = call(reqwest::Method::PUT, "/dxpeditions", Some(json!([{"call": " 3y0k ", "note": "Bouvet"}, {"call": ""}]))).await;
    let mine = after["items"].as_array().unwrap().iter().find(|i| i["call"] == "3Y0K").unwrap();
    assert_eq!((mine["manual"].clone(), mine["id"].clone(), mine["active"].clone()), (json!(true), json!(1), json!(true)));

    // Spots: a worked band is quiet; a new band and a new DXCC alert, and the pane remembers the last spot.
    let mut events = http.get(format!("{base}/events")).header("x-qrzero-token", &running.token).send().await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut buf = String::new();
        while let Ok(Some(chunk)) = events.chunk().await {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(end) = buf.find("\n\n") {
                let event: String = buf.drain(..end + 2).collect();
                for line in event.lines().filter_map(|l| l.strip_prefix("data:")) {
                    if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                        let _ = tx.send(v);
                    }
                }
            }
        }
    });
    static SPOTS: &[&str] = &[
        "DX de K1TTT:     14020.0  JA7XYZ       CW                             1234Z",
        "DX de K1TTT:     18080.0  JA7XYZ       CW                             1235Z",
        "DX de W3LPL:     14010.0  VP8LP/P      CW up 1                        1236Z",
        "DX de W3LPL:     14012.0  K9ZZZ        CW                             1237Z",
    ];
    let port = mock_node(SPOTS).await;
    call(
        reqwest::Method::PUT,
        "/cluster",
        Some(json!({"nodes": [{"name": "Test", "host": "127.0.0.1", "port": port, "login": "", "password": "", "commands": []}], "auto_connect": false})),
    )
    .await;
    call(reqwest::Method::POST, "/cluster/connect", Some(json!({"connect": true}))).await;
    let mut hits = Vec::new();
    while hits.len() < 2 {
        let v = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let v = rx.recv().await.expect("event stream ended");
                if v["type"] == "watch_hit" {
                    return v["hit"].clone();
                }
            }
        })
        .await
        .expect("no watch_hit event");
        hits.push(v);
    }
    assert_eq!((hits[0]["call"].clone(), hits[0]["label"].clone(), hits[0]["band"].clone()), (json!("JA7XYZ"), json!("DXpedition: New band 17m"), json!("17m")));
    assert_eq!((hits[1]["call"].clone(), hits[1]["label"].clone()), (json!("VP8LP/P"), json!("DXpedition: New DXCC")));
    assert_eq!(hits[1]["freq_hz"], 14_010_000);

    let list = get("/dxpeditions").await;
    let by = |c: &str| list["items"].as_array().unwrap().iter().find(|i| i["call"] == c).unwrap().clone();
    assert_eq!(by("JA7XYZ")["spot"]["freq_hz"], 18_080_000, "the latest spot");
    assert_eq!(by("VP8LP")["spot"]["call"], "VP8LP/P");
    assert_eq!(by("VP8LP")["spot"]["comment"], "CW up 1");
    assert_eq!(by("W2UPC")["spot"], Value::Null);
    let watch_hits = get("/watch/hits").await;
    assert_eq!(watch_hits.as_array().unwrap().len(), 2, "the repeat 20m spot and the plain K9ZZZ spot are quiet");
}
