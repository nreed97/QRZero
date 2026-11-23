//! The PSK Reporter feed: off until turned on, then reports from a stand-in MQTT broker become spots.

use std::time::Duration;

use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Reads one MQTT packet: its first byte and body.
async fn packet(s: &mut TcpStream) -> (u8, Vec<u8>) {
    let first = s.read_u8().await.unwrap();
    let (mut len, mut mult) = (0usize, 1usize);
    loop {
        let b = s.read_u8().await.unwrap();
        len += usize::from(b & 0x7f) * mult;
        if b & 0x80 == 0 {
            break;
        }
        mult *= 128;
    }
    let mut body = vec![0; len];
    s.read_exact(&mut body).await.unwrap();
    (first, body)
}

fn publish(topic: &str, payload: &str) -> Vec<u8> {
    let mut body = (topic.len() as u16).to_be_bytes().to_vec();
    body.extend_from_slice(topic.as_bytes());
    body.extend_from_slice(payload.as_bytes());
    let mut p = vec![0x30];
    let mut n = body.len();
    loop {
        let mut b = (n % 128) as u8;
        n /= 128;
        if n > 0 {
            b |= 0x80;
        }
        p.push(b);
        if n == 0 {
            break;
        }
    }
    p.extend_from_slice(&body);
    p
}

#[tokio::test]
async fn reports_become_spots_once_turned_on() {
    let broker = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = broker.local_addr().unwrap().port();
    let now = chrono::Utc::now().timestamp();
    tokio::spawn(async move {
        let (mut s, _) = broker.accept().await.unwrap();
        assert_eq!(packet(&mut s).await.0, 0x10);
        s.write_all(&[0x20, 2, 0, 0]).await.unwrap();
        let (first, body) = packet(&mut s).await;
        assert_eq!(first, 0x82);
        assert!(String::from_utf8_lossy(&body).contains("pskr/filter/v2/+/+/VP8PJ/#"));
        s.write_all(&[0x90, 3, 0, 1, 0]).await.unwrap();
        for (rc, f, rp, dt) in [("K1ABC", 14_075_100, -15, 60), ("W2XYZ", 14_075_120, -3, 30), ("DL1AA", 14_024_000, 9, 10)] {
            let md = if f < 14_070_000 { "CW" } else { "FT8" };
            let payload = json!({"f": f, "md": md, "rp": rp, "t": now - dt, "sc": "VP8PJ", "sl": "GD18", "rc": rc, "rl": "FN42", "b": "20m"});
            s.write_all(&publish("pskr/filter/v2/20m/FT8/VP8PJ/K1ABC/GD18/FN42/240/291", &payload.to_string())).await.unwrap();
        }
        let mut buf = [0u8; 64];
        while s.read(&mut buf).await.is_ok_and(|n| n > 0) {}
    });

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-psk-{}", std::process::id());
    cfg.update_cty = false;
    cfg.launch_apps = false;
    let running = start(cfg).await.unwrap();
    let http = reqwest::Client::new();
    let url = format!("http://{}/api/psk", running.addr);
    let get = || {
        let req = http.get(&url).header("x-qrzero-token", &running.token);
        async move { req.send().await.unwrap().json::<Value>().await.unwrap() }
    };

    let first = get().await;
    assert_eq!(first["config"]["enabled"], false, "off by default");
    assert_eq!(first["state"], "off");
    assert_eq!(first["spots"].as_array().unwrap().len(), 0);

    let bad = http.put(&url).header("x-qrzero-token", &running.token).json(&json!({"enabled": true, "calls": ["VP8+"]})).send().await.unwrap();
    assert_eq!(bad.status(), 400, "not a callsign");

    let saved = http
        .put(&url)
        .header("x-qrzero-token", &running.token)
        .json(&json!({"enabled": true, "calls": ["vp8pj"], "minutes": 30, "host": "127.0.0.1", "port": port}))
        .send()
        .await
        .unwrap();
    assert!(saved.status().is_success());
    assert_eq!(saved.json::<Value>().await.unwrap()["config"]["calls"], json!(["VP8PJ"]));

    let mut view = Value::Null;
    for _ in 0..100 {
        view = get().await;
        if view["spots"].as_array().is_some_and(|s| s.len() == 2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(view["connected"], true, "{view}");
    let spots = view["spots"].as_array().unwrap();
    assert_eq!(spots.len(), 2, "{view}");
    assert_eq!((spots[0]["mode"].as_str(), spots[0]["tune_hz"].as_u64()), (Some("CW"), Some(14_024_000)), "newest first");
    let ft8 = &spots[1];
    assert_eq!((ft8["band"].as_str(), ft8["reports"].as_u64(), ft8["receivers"].as_u64()), (Some("20m"), Some(2), Some(2)));
    assert_eq!((ft8["tune_hz"].as_u64(), ft8["best_snr"].as_i64(), ft8["last_receiver"].as_str()), (Some(14_074_000), Some(-3), Some("W2XYZ")));

    // Turning it off disconnects but keeps the list of calls.
    let off = http
        .put(&url)
        .header("x-qrzero-token", &running.token)
        .json(&json!({"enabled": false, "calls": ["VP8PJ"], "minutes": 30, "host": "127.0.0.1", "port": port}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!((off["state"].as_str(), off["connected"].as_bool()), (Some("off"), Some(false)));
    assert_eq!(off["config"]["calls"], json!(["VP8PJ"]));
}
