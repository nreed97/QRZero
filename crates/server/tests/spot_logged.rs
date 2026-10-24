//! Spots already on the band map take a newly logged QSO into account.

use std::time::Duration;

use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const CTY: &str = "JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA JE JF;\n";

async fn mock_node(spot: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let Ok((sock, _)) = listener.accept().await else { return };
        let (r, mut w) = sock.into_split();
        let mut lines = BufReader::new(r).lines();
        w.write_all(b"login: ").await.unwrap();
        let _ = lines.next_line().await;
        w.write_all(format!("Hello N0CALL\r\n{spot}\r\n").as_bytes()).await.unwrap();
        while let Ok(Some(_)) = lines.next_line().await {}
    });
    port
}

#[tokio::test]
async fn logging_a_qso_clears_the_needed_flags_but_keeps_the_spot() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
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
    let log = call(reqwest::Method::GET, "/logs", None).await[0]["id"].as_i64().unwrap();
    call(reqwest::Method::POST, &format!("/logs/{log}/callsigns"), Some(json!({"callsign": "N0CALL"}))).await;
    let r = http.post(format!("{base}/cty")).header("x-qrzero-token", &running.token).body(CTY).send().await.unwrap();
    assert_eq!(r.status(), 200);
    call(reqwest::Method::POST, "/station/active", Some(json!({"log_id": log, "location_id": null, "station_callsign": "N0CALL"}))).await;
    // Let the worked index finish before the spot arrives.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let port = mock_node("DX de W3LPL:     14025.0  JA1XYZ       CW                             1234Z").await;
    let node = json!({"name": "Test", "host": "127.0.0.1", "port": port, "login": "", "password": "", "commands": []});
    call(reqwest::Method::PUT, "/cluster", Some(json!({"nodes": [node], "auto_connect": false}))).await;
    call(reqwest::Method::POST, "/cluster/connect", Some(json!({"connect": true}))).await;
    let spot = wait_spot(&call, |s| s["needed"]["new_dxcc"] == true).await;
    assert_eq!(spot["call"], "JA1XYZ");

    let fields = json!({"CALL": "JA1XYZ", "BAND": "20m", "MODE": "CW", "QSO_DATE": "20261024", "TIME_ON": "0100", "DXCC": "339"});
    call(reqwest::Method::POST, &format!("/logs/{log}/qsos"), Some(json!({"location_id": null, "fields": fields}))).await;
    let spot = wait_spot(&call, |s| s["needed"]["new_dxcc"] == false).await;
    assert_eq!(spot["call"], "JA1XYZ", "the spot stays");
    assert_eq!(spot["needed"]["new_call_band"], false);
    assert_eq!(spot["needed"]["new_mode"], false);
}

async fn wait_spot<F, Fut>(call: &F, ok: impl Fn(&Value) -> bool) -> Value
where
    F: Fn(reqwest::Method, &str, Option<Value>) -> Fut,
    Fut: std::future::Future<Output = Value>,
{
    for _ in 0..150 {
        let c = call(reqwest::Method::GET, "/cluster", None).await;
        if let Some(s) = c["spots"].as_array().and_then(|a| a.first()).filter(|s| ok(s)) {
            return s.clone();
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("timed out");
}
