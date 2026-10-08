//! The live station: WSJT-X and N1MM over UDP, a Hamlib rig, and the event stream.

use std::time::Duration;

use qrzero_server::{start, Config};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;

const CTY: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
";

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Api {
    async fn new() -> Self {
        Self::with_qrz(String::new()).await
    }

    async fn with_qrz(qrz_endpoint: String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_path_buf());
        cfg.qrz_endpoint = qrz_endpoint;
        cfg.secret_service = format!("QRZero-test-{}", std::process::id());
        cfg.update_cty = false;
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

    /// Polls until `check` passes (the server handles UDP in the background).
    async fn wait_for(&self, path: &str, body: Option<Value>, check: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..100 {
            let v = match &body {
                Some(b) => self.post(path, b.clone()).await,
                None => self.get(path).await,
            };
            if check(&v) {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!("timed out waiting on {path}");
    }

    /// Reads the event stream into a channel of JSON events.
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

async fn next_matching(rx: &mut mpsc::UnboundedReceiver<Value>, f: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let v = rx.recv().await.expect("event stream ended");
            if f(&v) {
                return v;
            }
        }
    })
    .await
    .expect("no matching event")
}

fn free_udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Just enough of WSJT-X's QDataStream encoding to fake an instance.
struct Packet(Vec<u8>);

impl Packet {
    fn new(kind: u32, id: &str) -> Self {
        let mut p = Packet(Vec::new());
        p.u32(0xADBC_CBDA).u32(2).u32(kind).str(id);
        p
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    fn f64(&mut self, v: f64) -> &mut Self {
        self.0.extend(v.to_be_bytes());
        self
    }
    fn bool(&mut self, v: bool) -> &mut Self {
        self.0.push(v as u8);
        self
    }
    fn str(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.0.extend(s.as_bytes());
        self
    }
}

fn status(dial: u64, mode: &str) -> Vec<u8> {
    status_from("WSJT-X", dial, mode)
}

fn status_from(id: &str, dial: u64, mode: &str) -> Vec<u8> {
    let mut p = Packet::new(1, id);
    p.u64(dial).str(mode).str("").str("").str(mode).bool(false).bool(false).bool(true).u32(1500).u32(1500).str("N0CALL").str("EN34").str("");
    p.0
}

/// A full Status, as WSJT-X 2.x sends it, with a configuration name.
fn status_config(id: &str, dial: u64, mode: &str, config: &str) -> Vec<u8> {
    let mut p = Packet(status_from(id, dial, mode));
    // tx_watchdog, sub_mode, fast_mode, special_op_mode (u8), frequency_tolerance, tr_period, configuration_name, tx_message
    p.bool(false).str("").bool(false).bool(false).u32(0).u32(15).str(config).str("");
    p.0
}

fn decode(time_ms: u32, snr: i32, df: u32, message: &str) -> Vec<u8> {
    decode_from("WSJT-X", time_ms, snr, df, message)
}

fn decode_from(id: &str, time_ms: u32, snr: i32, df: u32, message: &str) -> Vec<u8> {
    let mut p = Packet::new(2, id);
    p.bool(true).u32(time_ms).i32(snr).f64(0.1).u32(df).str("~").str(message).bool(false).bool(false);
    p.0
}

fn logged_adif(adif: &str) -> Vec<u8> {
    let mut p = Packet::new(12, "WSJT-X");
    p.str(adif);
    p.0
}

async fn setup(api: &Api) -> (i64, i64) {
    let log = api.get("/logs").await[0]["id"].as_i64().unwrap();
    api.post(&format!("/logs/{log}/callsigns"), json!({"callsign": "N0CALL"})).await;
    let loc = api.post(&format!("/logs/{log}/locations"), json!({"name": "Home", "fields": {"MY_GRIDSQUARE": "EN34"}})).await["id"].as_i64().unwrap();
    api.send(reqwest::Method::POST, "/cty", None, Some(CTY)).await;
    api.post("/station/active", json!({"log_id": log, "location_id": loc, "station_callsign": "N0CALL"})).await;
    (log, loc)
}

#[tokio::test]
async fn wsjtx_decodes_replies_and_logging() {
    let api = Api::new().await;
    let (log, _) = setup(&api).await;
    let port = free_udp_port();
    let n1mm_port = free_udp_port();
    let mut cfg = api.get("/integrations").await["config"].clone();
    cfg["wsjtx_listen"] = json!(format!("127.0.0.1:{port}"));
    cfg["n1mm_enabled"] = json!(true);
    cfg["n1mm_listen"] = json!(format!("127.0.0.1:{n1mm_port}"));
    let saved = api.send(reqwest::Method::PUT, "/integrations", Some(cfg), None).await;
    assert!(saved["status"]["wsjtx"].as_str().unwrap().starts_with("listening"), "{saved}");

    // A JA station already in the log on 20m FT8.
    api.post(&format!("/logs/{log}/qsos"), json!({"location_id": null, "fields": {"CALL": "JA1AAA", "QSO_DATE": "20240101", "TIME_ON": "1200", "BAND": "20m", "MODE": "FT8"}})).await;

    let wsjtx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server = format!("127.0.0.1:{port}");
    wsjtx.send_to(&status(14_074_000, "FT8"), &server).await.unwrap();
    wsjtx.send_to(&decode(45_015_000, -12, 1200, "CQ K1ABC FN42"), &server).await.unwrap();
    wsjtx.send_to(&decode(45_015_000, -3, 800, "N0CALL JA1XYZ PM95"), &server).await.unwrap();

    let ftx = api.wait_for("/ftx", None, |v| v["decodes"].as_array().is_some_and(|d| d.len() == 2)).await;
    let d = &ftx["decodes"];
    assert_eq!(ftx["instances"][0]["band"], "20m");
    assert_eq!(d[0]["call"], "K1ABC");
    assert_eq!(d[0]["grid"], "FN42");
    assert_eq!(d[0]["time"], "123015");
    assert_eq!(d[0]["freq_hz"], 14_075_200);
    assert_eq!(d[0]["entity"]["dxcc"], 291);
    assert_eq!(d[0]["needed"]["new_dxcc"], true, "{d}");
    assert_eq!(d[1]["to_me"], true);
    assert_eq!(d[1]["needed"]["new_dxcc"], false);
    assert_eq!(d[1]["needed"]["new_call"], true);

    // Double-clicking a decode sends WSJT-X a Reply (type 4) for it.
    api.post("/ftx/reply", json!({"seq": d[0]["seq"]})).await;
    let mut buf = [0u8; 1024];
    let (n, _) = tokio::time::timeout(Duration::from_secs(2), wsjtx.recv_from(&mut buf)).await.unwrap().unwrap();
    assert_eq!(&buf[8..12], &4u32.to_be_bytes());
    assert!(String::from_utf8_lossy(&buf[..n]).contains("CQ K1ABC FN42"));

    // QSOs WSJT-X logs land in the log once, with DXCC from the country file.
    let adif = "<adif_ver:5>3.1.0<eoh><call:5>K1ABC<gridsquare:4>FN42<mode:3>FT8<rst_sent:3>-12<rst_rcvd:3>-09<qso_date:8>20240102<time_on:6>123015<qso_date_off:8>20240102<time_off:6>123145<band:3>20m<freq:9>14.075200<station_callsign:6>N0CALL<my_gridsquare:4>EN34<eor>";
    wsjtx.send_to(&logged_adif(adif), &server).await.unwrap();
    wsjtx.send_to(&logged_adif(adif), &server).await.unwrap();
    let search = json!({"filter": {"call": "K1ABC"}});
    let found = api.wait_for(&format!("/logs/{log}/qsos/search"), Some(search.clone()), |v| v["total"] == 1).await;
    let fields = &found["rows"][0]["fields"];
    assert_eq!(fields["DXCC"], "291");
    assert_eq!(fields["MY_GRIDSQUARE"], "EN34");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(api.post(&format!("/logs/{log}/qsos/search"), search).await["total"], 1, "the repeat was a duplicate");

    // The next decode from K1ABC is no longer a new DXCC.
    wsjtx.send_to(&decode(45_030_000, -10, 1200, "CQ K1ABC FN42"), &server).await.unwrap();
    let ftx = api.wait_for("/ftx", None, |v| v["decodes"].as_array().is_some_and(|d| d.len() == 3)).await;
    assert_eq!(ftx["decodes"][2]["needed"]["new_dxcc"], false);
    assert_eq!(ftx["decodes"][2]["needed"]["new_call_band"], false);

    // N1MM: a contact, an edit of it, then a delete.
    let n1mm = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let n1mm_addr = format!("127.0.0.1:{n1mm_port}");
    let contact = |tag: &str, snt: &str| {
        format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><{tag}><contestname>DXCC</contestname><timestamp>2024-01-03 10:11:12</timestamp><mycall>N0CALL</mycall><band>7</band><rxfreq>702500</rxfreq><txfreq>702500</txfreq><operator>N0CALL</operator><mode>CW</mode><call>W9XYZ</call><snt>{snt}</snt><rcv>599</rcv><ID>abc123</ID></{tag}>")
    };
    n1mm.send_to(contact("contactinfo", "599").as_bytes(), &n1mm_addr).await.unwrap();
    let search = json!({"filter": {"call": "W9XYZ"}});
    let found = api.wait_for(&format!("/logs/{log}/qsos/search"), Some(search.clone()), |v| v["total"] == 1).await;
    assert_eq!(found["rows"][0]["fields"]["BAND"], "40m");
    n1mm.send_to(contact("contactreplace", "579").as_bytes(), &n1mm_addr).await.unwrap();
    api.wait_for(&format!("/logs/{log}/qsos/search"), Some(search.clone()), |v| v["rows"][0]["fields"]["RST_SENT"] == "579").await;
    let delete = "<?xml version=\"1.0\" encoding=\"utf-8\"?><contactdelete><timestamp>2024-01-03 10:11:12</timestamp><call>W9XYZ</call><contestname>DXCC</contestname><StationName>PC</StationName><ID>abc123</ID></contactdelete>";
    n1mm.send_to(delete.as_bytes(), &n1mm_addr).await.unwrap();
    api.wait_for(&format!("/logs/{log}/qsos/search"), Some(search), |v| v["total"] == 0).await;
}

/// A tiny rigctld: answers f, m and t, and reports every set command.
async fn mock_rigctld(seen: mpsc::UnboundedSender<String>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let seen = seen.clone();
            tokio::spawn(async move {
                let (r, mut w) = sock.into_split();
                let mut lines = BufReader::new(r).lines();
                let (mut freq, mut mode) = (14_025_000u64, "CW".to_string());
                while let Ok(Some(line)) = lines.next_line().await {
                    let reply = match line.split_whitespace().collect::<Vec<_>>().as_slice() {
                        ["f"] => format!("{freq}\n"),
                        ["m"] => format!("{mode}\n500\n"),
                        ["t"] => "0\n".to_string(),
                        ["F", hz] => {
                            freq = hz.parse().unwrap();
                            "RPRT 0\n".to_string()
                        }
                        ["M", m, _] => {
                            mode = m.to_string();
                            "RPRT 0\n".to_string()
                        }
                        _ => "RPRT -1\n".to_string(),
                    };
                    if line.starts_with(['F', 'M']) {
                        let _ = seen.send(line.clone());
                    }
                    if w.write_all(reply.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn hamlib_rig_follows_and_tunes() {
    let api = Api::new().await;
    let (_, loc) = setup(&api).await;
    let mut events = api.events().await;
    let (tx, mut seen) = mpsc::unbounded_channel();
    let port = mock_rigctld(tx).await;
    let rig = api
        .post(&format!("/locations/{loc}/equipment"), json!({"kind": "rig", "name": "K3", "fields": {"CONTROL": "hamlib", "PORT": port.to_string()}}))
        .await;
    let key = format!("rig:{}:0", rig["id"]);

    let radios = next_matching(&mut events, |e| e["type"] == "radios" && e["radios"].as_array().is_some_and(|r| r.iter().any(|r| r["connected"] == true))).await;
    let radio = &radios["radios"][0];
    assert_eq!(radio["key"], key.as_str());
    assert_eq!(radio["name"], "K3");
    assert_eq!(radio["freq_hz"], 14_025_000);
    assert_eq!(radio["mode"], "CW");

    api.post("/radios/tune", json!({"key": key, "freq_hz": 7_030_000, "mode": "SSB"})).await;
    let got: Vec<String> = vec![seen.recv().await.unwrap(), seen.recv().await.unwrap()];
    assert_eq!(got, ["F 7030000", "M LSB 0"]);
    next_matching(&mut events, |e| e["type"] == "radios" && e["radios"][0]["freq_hz"] == 7_030_000 && e["radios"][0]["mode"] == "SSB").await;
}

#[tokio::test]
async fn wsjtx_instances_on_separate_ports() {
    let api = Api::new().await;
    setup(&api).await;
    let (a, b) = (free_udp_port(), free_udp_port());
    let mut cfg = api.get("/integrations").await["config"].clone();
    cfg["wsjtx_listen"] = json!(format!("127.0.0.1:{a}, 127.0.0.1:{b}"));
    let saved = api.send(reqwest::Method::PUT, "/integrations", Some(cfg), None).await;
    let st = saved["status"]["wsjtx"].as_str().unwrap();
    assert!(st.contains(&format!("listening on 127.0.0.1:{a}")) && st.contains(&format!("listening on 127.0.0.1:{b}")), "{st}");

    let one = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let two = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    one.send_to(&status_from("WSJT-X - Slice A", 14_074_000, "FT8"), format!("127.0.0.1:{a}")).await.unwrap();
    one.send_to(&decode_from("WSJT-X - Slice A", 45_015_000, -12, 1200, "CQ K1ABC FN42"), format!("127.0.0.1:{a}")).await.unwrap();
    two.send_to(&status_config("JTDX - 40m", 7_074_000, "FT8", "Flex Slice B"), format!("127.0.0.1:{b}")).await.unwrap();
    two.send_to(&decode_from("JTDX - 40m", 45_015_000, -5, 900, "CQ JA1XYZ PM95"), format!("127.0.0.1:{b}")).await.unwrap();

    let ftx = api.wait_for("/ftx", None, |v| v["decodes"].as_array().is_some_and(|d| d.len() == 2) && v["instances"].as_array().is_some_and(|i| i.len() == 2)).await;
    let ja = ftx["decodes"].as_array().unwrap().iter().find(|d| d["call"] == "JA1XYZ").unwrap().clone();
    assert_eq!(ja["band"], "40m");
    // Each decode says where it came from: the slice from the id or the configuration name.
    assert_eq!((&ja["source"], &ja["slice"]), (&json!("Slice B · JTDX"), &json!("B")));
    let k1 = ftx["decodes"].as_array().unwrap().iter().find(|d| d["call"] == "K1ABC").unwrap().clone();
    assert_eq!((&k1["source"], &k1["slice"]), (&json!("Slice A · WSJT-X"), &json!("A")));
    assert_ne!(k1["color_index"], ja["color_index"]);
    let jtdx = ftx["instances"].as_array().unwrap().iter().find(|i| i["id"] == "JTDX - 40m").unwrap().clone();
    assert_eq!((&jtdx["program"], &jtdx["configuration_name"], &jtdx["rig_key"]), (&json!("JTDX"), &json!("Flex Slice B"), &Value::Null));

    // A reply goes to the instance that decoded it, on its own port.
    api.post("/ftx/reply", json!({"seq": ja["seq"]})).await;
    let mut buf = [0u8; 1024];
    let (n, from) = tokio::time::timeout(Duration::from_secs(2), two.recv_from(&mut buf)).await.unwrap().unwrap();
    assert_eq!(from.port(), b);
    assert!(String::from_utf8_lossy(&buf[..n]).contains("CQ JA1XYZ PM95"));
    assert!(tokio::time::timeout(Duration::from_millis(200), one.recv_from(&mut buf)).await.is_err(), "nothing for the other instance");
}

#[tokio::test]
async fn auto_logged_qsos_get_the_antenna_for_their_band() {
    let api = Api::new().await;
    let (log, loc) = setup(&api).await;
    let port = free_udp_port();
    let mut cfg = api.get("/integrations").await["config"].clone();
    cfg["wsjtx_listen"] = json!(format!("127.0.0.1:{port}"));
    api.send(reqwest::Method::PUT, "/integrations", Some(cfg), None).await;
    for (name, bands) in [("Dipole", "80m,40m"), ("Hex beam", "20m,17m,15m"), ("Vertical", "40 20 10")] {
        api.post(&format!("/locations/{loc}/equipment"), json!({"kind": "antenna", "name": name, "fields": {"BANDS": bands}})).await;
    }

    let wsjtx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server = format!("127.0.0.1:{port}");
    let qso = |call: &str, band: &str, extra: &str| {
        format!("<call:{}>{call}<mode:3>FT8<qso_date:8>20240102<time_on:6>123015<band:{}>{band}{extra}<eor>", call.len(), band.len())
    };
    wsjtx.send_to(&logged_adif(&qso("K1ABC", "20m", "")), &server).await.unwrap();
    wsjtx.send_to(&logged_adif(&qso("K2ABC", "10m", "")), &server).await.unwrap();
    wsjtx.send_to(&logged_adif(&qso("K3ABC", "6m", "")), &server).await.unwrap();
    wsjtx.send_to(&logged_adif(&qso("K4ABC", "20m", "<my_antenna:4>Loop")), &server).await.unwrap();

    let all = api.wait_for(&format!("/logs/{log}/qsos/search"), Some(json!({"filter": {}})), |v| v["total"] == 4).await;
    let ant = |call: &str| {
        let row = all["rows"].as_array().unwrap().iter().find(|r| r["fields"]["CALL"] == call).unwrap();
        row["fields"]["MY_ANTENNA"].as_str().map(str::to_string)
    };
    assert_eq!(ant("K1ABC").as_deref(), Some("Hex beam"), "first antenna in tree order for 20m");
    assert_eq!(ant("K2ABC").as_deref(), Some("Vertical"));
    assert_eq!(ant("K3ABC"), None, "no antenna covers 6m");
    assert_eq!(ant("K4ABC").as_deref(), Some("Loop"), "the program's own antenna is kept");
}

/// A stand-in for xmldata.qrz.com that knows one call.
async fn mock_qrz() -> String {
    use axum::{extract::Query, routing::get, Router};
    use std::collections::HashMap;
    let app = Router::new().route(
        "/xml/",
        get(|Query(q): Query<HashMap<String, String>>| async move {
            if q.contains_key("username") {
                return "<QRZDatabase><Session><Key>abc</Key></Session></QRZDatabase>".to_string();
            }
            match q.get("callsign").map(String::as_str) {
                Some("K1ABC") => "<QRZDatabase><Callsign><call>K1ABC</call><fname>Ann</fname><name>Smith</name><addr2>Boston</addr2><state>MA</state><grid>FN42aa</grid></Callsign><Session><Key>abc</Key></Session></QRZDatabase>".to_string(),
                Some(c) => format!("<QRZDatabase><Session><Key>abc</Key><Error>Not found: {c}</Error></Session></QRZDatabase>"),
                None => String::new(),
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/xml/")
}

#[tokio::test]
async fn auto_logged_qsos_are_looked_up_on_qrz() {
    let api = Api::with_qrz(mock_qrz().await).await;
    let (log, _) = setup(&api).await;
    api.send(reqwest::Method::PUT, "/settings", Some(json!({"qrz_enabled": true, "qrz_username": "me", "qrz_password": "pw"})), None).await;
    let port = free_udp_port();
    let mut cfg = api.get("/integrations").await["config"].clone();
    assert_eq!(cfg["auto_log_lookup"], true, "on by default");
    cfg["wsjtx_listen"] = json!(format!("127.0.0.1:{port}"));
    api.send(reqwest::Method::PUT, "/integrations", Some(cfg), None).await;

    let wsjtx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    // WSJT-X sent a grid of its own: that one is kept.
    let adif = "<call:5>K1ABC<gridsquare:4>FN41<mode:3>FT8<qso_date:8>20240102<time_on:6>123015<band:3>20m<eor>";
    wsjtx.send_to(&logged_adif(adif), format!("127.0.0.1:{port}")).await.unwrap();
    let found = api
        .wait_for(&format!("/logs/{log}/qsos/search"), Some(json!({"filter": {}})), |v| v["rows"][0]["fields"]["NAME"].is_string())
        .await;
    let f = &found["rows"][0]["fields"];
    assert_eq!(f["NAME"], "Ann Smith");
    assert_eq!(f["QTH"], "Boston");
    assert_eq!(f["STATE"], "MA");
    assert_eq!(f["GRIDSQUARE"], "FN41", "what WSJT-X sent is not overwritten");
}
