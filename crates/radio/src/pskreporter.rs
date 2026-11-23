//! PSK Reporter live feed: reception reports of chosen stations from PSK Reporter's MQTT server.
//!
//! [`connect`] runs a background task that subscribes to the reports of the given transmitting
//! callsigns and sends each parsed [`Report`] on a channel, reconnecting after a drop. It speaks
//! just enough MQTT 3.1.1 for that (connect, subscribe at QoS 0, receive, ping), so it needs no
//! MQTT library. The topic ([`topic_for`]) and payload parsing ([`parse_report`]) are pure functions.

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Instant};

pub const DEFAULT_HOST: &str = "mqtt.pskreporter.info";
pub const DEFAULT_PORT: u16 = 1883;
/// Seconds between pings the server is promised (MQTT keep-alive).
const KEEP_ALIVE: u16 = 60;
const ANSWER_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest packet accepted; reports are a few hundred bytes.
const MAX_PACKET: usize = 64 * 1024;

/// One reception report: `receiver` heard `sender`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    pub sender: String,
    pub sender_grid: String,
    pub receiver: String,
    pub receiver_grid: String,
    /// The signal's frequency (for the digital modes, dial plus audio offset).
    pub freq_hz: u64,
    pub mode: String,
    pub snr: Option<i32>,
    /// Unix seconds of the reception.
    pub time: i64,
}

/// What the feed connection reports.
#[derive(Clone, Debug, PartialEq)]
pub enum PskEvent {
    Connecting,
    /// Connected and subscribed.
    Connected,
    /// The connection failed or dropped; it is tried again after a delay.
    Disconnected { reason: String },
    Report(Report),
}

/// Handle to the background feed connection. Dropping it disconnects.
pub struct PskHandle {
    task: JoinHandle<()>,
}

impl Drop for PskHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Whether `call` can be looked for: 3 to 15 letters, digits and '/'.
pub fn valid_call(call: &str) -> bool {
    (3..=15).contains(&call.len()) && call.chars().all(|c| c.is_ascii_alphanumeric() || c == '/') && call.chars().any(|c| c.is_ascii_alphanumeric())
}

/// The topic carrying every report of `call` transmitting, on any band and mode. PSK Reporter
/// writes a '/' in a callsign as '.' in its topics.
pub fn topic_for(call: &str) -> String {
    format!("pskr/filter/v2/+/+/{}/#", call.trim().to_uppercase().replace('/', "."))
}

/// A report from its JSON payload, or `None` without a sender or frequency.
pub fn parse_report(payload: &[u8]) -> Option<Report> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let sender = text("sc").to_uppercase();
    let freq_hz = v.get("f").and_then(|f| f.as_u64().or_else(|| f.as_f64().filter(|x| *x > 0.0).map(|x| x.round() as u64)))?;
    if sender.is_empty() || freq_hz == 0 {
        return None;
    }
    Some(Report {
        sender,
        sender_grid: text("sl"),
        receiver: text("rc").to_uppercase(),
        receiver_grid: text("rl"),
        freq_hz,
        mode: text("md").to_uppercase(),
        snr: v.get("rp").and_then(Value::as_i64).map(|n| n as i32),
        time: v.get("t").and_then(Value::as_i64).unwrap_or(0),
    })
}

/// Connects to `host:port`, subscribes to the reports of `calls` and streams events. After a
/// drop it waits 10 s before trying again, doubling up to 5 minutes while it keeps failing.
/// Dropping the handle (or the receiver) disconnects. Must be called inside a tokio runtime.
pub fn connect(host: String, port: u16, calls: Vec<String>) -> (PskHandle, mpsc::Receiver<PskEvent>) {
    connect_with(host, port, calls, Duration::from_secs(10))
}

fn connect_with(host: String, port: u16, calls: Vec<String>, retry: Duration) -> (PskHandle, mpsc::Receiver<PskEvent>) {
    let (tx, rx) = mpsc::channel(1024);
    let topics: Vec<String> = calls.iter().map(String::as_str).map(topic_for).collect();
    let id = client_id();
    let task = tokio::spawn(async move {
        let mut wait = retry;
        loop {
            if tx.send(PskEvent::Connecting).await.is_err() {
                return;
            }
            let started = Instant::now();
            let reason = match session(&host, port, &id, &topics, &tx).await {
                Ok(()) => return,
                Err(e) => e.to_string(),
            };
            if tx.send(PskEvent::Disconnected { reason }).await.is_err() {
                return;
            }
            // A connection that lasted a while starts the delays over.
            if started.elapsed() > Duration::from_secs(120) {
                wait = retry;
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(Duration::from_secs(300));
        }
    });
    (PskHandle { task }, rx)
}

/// A client id unique enough for one server (MQTT 3.1.1 allows 23 characters).
fn client_id() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    format!("qrzero-{:016x}", nanos ^ (u64::from(std::process::id()) << 32))
}

struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// One connection, until it drops (an error) or nobody listens any more (`Ok`).
async fn session(host: &str, port: u16, client_id: &str, topics: &[String], tx: &mpsc::Sender<PskEvent>) -> Result<()> {
    let mut stream = timeout(ANSWER_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| anyhow!("timed out connecting to {host}"))?
        .map_err(|e| anyhow!("couldn't connect to {host}: {e}"))?;
    let _ = stream.set_nodelay(true);
    stream.write_all(&connect_packet(client_id)).await?;
    let (kind, body) = timeout(ANSWER_TIMEOUT, read_packet(&mut stream)).await.map_err(|_| anyhow!("no answer from {host}"))??;
    if kind >> 4 != 2 || body.len() < 2 {
        bail!("unexpected answer from {host}");
    }
    if body[1] != 0 {
        bail!("{host} refused the connection (code {})", body[1]);
    }
    stream.write_all(&subscribe_packet(1, topics)).await?;
    if tx.send(PskEvent::Connected).await.is_err() {
        return Ok(());
    }

    // Packets are read in their own task, so a ping never interrupts a half-read one.
    let (mut rd, mut wr) = stream.into_split();
    let (ptx, mut prx) = mpsc::channel::<Result<(u8, Vec<u8>)>>(64);
    let _reader = AbortOnDrop(tokio::spawn(async move {
        loop {
            let p = read_packet(&mut rd).await;
            let failed = p.is_err();
            if ptx.send(p).await.is_err() || failed {
                break;
            }
        }
    }));
    let half = Duration::from_secs(u64::from(KEEP_ALIVE) / 2);
    let mut ping = tokio::time::interval_at(Instant::now() + half, half);
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            p = prx.recv() => {
                let (first, body) = p.ok_or_else(|| anyhow!("connection closed"))??;
                heard = Instant::now();
                match first >> 4 {
                    3 => {
                        let Some((id, payload)) = parse_publish(first & 0x0f, &body) else { continue };
                        if let Some(id) = id {
                            let [h, l] = id.to_be_bytes();
                            wr.write_all(&[0x40, 2, h, l]).await?;
                        }
                        if let Some(r) = parse_report(payload) {
                            if tx.send(PskEvent::Report(r)).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                    // SUBACK: 0x80 for a topic means it was refused.
                    9 if body.get(2..).is_some_and(|codes| codes.contains(&0x80)) => bail!("{host} refused the subscription"),
                    _ => {}
                }
            }
            _ = ping.tick() => {
                if heard.elapsed() > Duration::from_secs(u64::from(KEEP_ALIVE) * 2) {
                    bail!("{host} stopped answering");
                }
                wr.write_all(&[0xC0, 0]).await?;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// MQTT 3.1.1 packets

fn put_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u16).to_be_bytes());
    buf.extend_from_slice(s.as_bytes());
}

fn packet(first: u8, body: &[u8]) -> Vec<u8> {
    let mut p = vec![first];
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
    p.extend_from_slice(body);
    p
}

fn connect_packet(client_id: &str) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, "MQTT");
    b.push(4); // protocol level: 3.1.1
    b.push(0x02); // clean session, no will, no login
    b.extend_from_slice(&KEEP_ALIVE.to_be_bytes());
    put_str(&mut b, client_id);
    packet(0x10, &b)
}

fn subscribe_packet(id: u16, topics: &[String]) -> Vec<u8> {
    let mut b = id.to_be_bytes().to_vec();
    for t in topics {
        put_str(&mut b, t);
        b.push(0); // QoS 0
    }
    packet(0x82, &b)
}

/// One packet: its first byte and its body.
async fn read_packet<R: AsyncRead + Unpin>(r: &mut R) -> Result<(u8, Vec<u8>)> {
    let first = r.read_u8().await?;
    let (mut len, mut mult) = (0usize, 1usize);
    for i in 0..4 {
        let b = r.read_u8().await?;
        len += usize::from(b & 0x7f) * mult;
        if b & 0x80 == 0 {
            break;
        }
        if i == 3 {
            bail!("malformed packet length");
        }
        mult *= 128;
    }
    if len > MAX_PACKET {
        bail!("packet too large ({len} bytes)");
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).await?;
    Ok((first, body))
}

/// A PUBLISH body: its packet id (for QoS 1 and 2) and payload.
fn parse_publish(flags: u8, body: &[u8]) -> Option<(Option<u16>, &[u8])> {
    let n = usize::from(u16::from_be_bytes([*body.first()?, *body.get(1)?]));
    let mut rest = body.get(2 + n..)?;
    let id = if (flags >> 1) & 3 > 0 {
        let id = u16::from_be_bytes([*rest.first()?, *rest.get(1)?]);
        rest = &rest[2..];
        Some(id)
    } else {
        None
    };
    Some((id, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    const PAYLOAD: &str = r#"{"sq":30142870791,"f":14075123,"md":"FT8","rp":-12,"t":1793485815,"t_tx":1793485800,
        "sc":"vp8pj","sl":"GD18","rc":"K1ABC","rl":"FN42aa","sa":240,"ra":291,"b":"20m"}"#;

    #[test]
    fn reads_a_report() {
        let r = parse_report(PAYLOAD.as_bytes()).unwrap();
        assert_eq!((r.sender.as_str(), r.sender_grid.as_str(), r.receiver.as_str(), r.receiver_grid.as_str()), ("VP8PJ", "GD18", "K1ABC", "FN42aa"));
        assert_eq!((r.freq_hz, r.mode.as_str(), r.snr, r.time), (14_075_123, "FT8", Some(-12), 1_793_485_815));
        assert!(parse_report(br#"{"sc":"K1ABC"}"#).is_none(), "no frequency");
        assert!(parse_report(br#"{"sc":"","f":7074000}"#).is_none(), "no sender");
        assert!(parse_report(b"not json").is_none());
    }

    #[test]
    fn topics_and_calls() {
        assert_eq!(topic_for(" vp8pj "), "pskr/filter/v2/+/+/VP8PJ/#");
        assert_eq!(topic_for("VP2M/K1ABC"), "pskr/filter/v2/+/+/VP2M.K1ABC/#");
        assert!(valid_call("3Y0J") && valid_call("VP2M/K1ABC"));
        assert!(!valid_call("K1") && !valid_call("K1ABC/#") && !valid_call("K1+B") && !valid_call("///"));
    }

    #[test]
    fn packet_lengths_use_several_bytes_when_long() {
        assert_eq!(&packet(0x30, &[0; 3])[..2], &[0x30, 3]);
        assert_eq!(&packet(0x30, &[0; 200])[..3], &[0x30, 0xC8, 0x01]);
    }

    #[tokio::test]
    async fn reads_packets_back() {
        let p = subscribe_packet(7, &[topic_for("K1ABC")]);
        let (first, body) = read_packet(&mut p.as_slice()).await.unwrap();
        assert_eq!(first, 0x82);
        assert_eq!(&body[..2], &[0, 7]);
        assert!(body.ends_with(b"K1ABC/#\0"));
        let mut publish = Vec::new();
        put_str(&mut publish, "t");
        publish.extend_from_slice(&[0, 9]);
        publish.extend_from_slice(b"hi");
        assert_eq!(parse_publish(0x02, &publish), Some((Some(9), &b"hi"[..])));
        assert_eq!(parse_publish(0x00, &publish), Some((None, &[0, 9, b'h', b'i'][..])));
    }

    /// A stand-in broker: accepts, checks the connect and subscribe, then sends one report.
    #[tokio::test]
    async fn subscribes_and_passes_reports_on() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let broker = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let (first, body) = read_packet(&mut s).await.unwrap();
            assert_eq!(first, 0x10);
            assert_eq!(&body[..6], b"\0\x04MQTT");
            s.write_all(&[0x20, 2, 0, 0]).await.unwrap();
            let (first, body) = read_packet(&mut s).await.unwrap();
            assert_eq!(first, 0x82);
            let text = String::from_utf8_lossy(&body).to_string();
            assert!(text.contains("pskr/filter/v2/+/+/VP8PJ/#") && text.contains("pskr/filter/v2/+/+/3Y0J/#"), "{text}");
            s.write_all(&[0x90, 4, 0, 1, 0, 0]).await.unwrap();
            let mut body = Vec::new();
            put_str(&mut body, "pskr/filter/v2/20m/FT8/VP8PJ/K1ABC/GD18/FN42/240/291");
            body.extend_from_slice(PAYLOAD.as_bytes());
            s.write_all(&packet(0x30, &body)).await.unwrap();
            // Held open until the client goes.
            let mut buf = [0u8; 64];
            while s.read(&mut buf).await.is_ok_and(|n| n > 0) {}
        });
        let (handle, mut rx) = connect_with("127.0.0.1".into(), port, vec!["VP8PJ".into(), "3Y0J".into()], Duration::from_millis(50));
        let mut got = Vec::new();
        while got.len() < 3 {
            got.push(timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap());
        }
        assert_eq!(got[0], PskEvent::Connecting);
        assert_eq!(got[1], PskEvent::Connected);
        match &got[2] {
            PskEvent::Report(r) => assert_eq!((r.sender.as_str(), r.freq_hz), ("VP8PJ", 14_075_123)),
            other => panic!("expected a report, got {other:?}"),
        }
        drop(handle);
        timeout(Duration::from_secs(5), broker).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_refused_connection_is_reported_and_retried() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { break };
                let _ = read_packet(&mut s).await;
                let _ = s.write_all(&[0x20, 2, 0, 5]).await;
            }
        });
        let (_handle, mut rx) = connect_with("127.0.0.1".into(), port, vec!["K1ABC".into()], Duration::from_millis(20));
        assert_eq!(next(&mut rx).await, PskEvent::Connecting);
        assert!(matches!(next(&mut rx).await, PskEvent::Disconnected { reason } if reason.contains("code 5")));
        assert_eq!(next(&mut rx).await, PskEvent::Connecting, "tried again");
    }

    async fn next(rx: &mut mpsc::Receiver<PskEvent>) -> PskEvent {
        timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap()
    }
}
