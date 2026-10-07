//! DX cluster client: DXSpider, AR-Cluster, CC-Cluster and VE7CC nodes over telnet.
//!
//! [`connect`] runs a background task that logs in, streams every received line and parsed
//! [`Spot`]s on a broadcast channel, and fails over to the next node when a connection drops.
//! Parsing ([`parse_spot`]) and mode guessing ([`guess_mode`]) are pure functions.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, timeout, Instant};

/// A cluster node to connect to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterNode {
    pub name: String,
    pub host: String,
    pub port: u16,
    /// Login callsign; empty = the station callsign passed to connect.
    #[serde(default)]
    pub login: String,
    /// Optional password some nodes ask for.
    #[serde(default)]
    pub password: String,
    /// Commands sent after login, one per line (e.g. "set/dx/filter", "sh/dx 30").
    #[serde(default)]
    pub commands: Vec<String>,
}

/// One DX spot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Spot {
    /// As sent, including any skimmer suffix ("DL8LAS-#").
    pub spotter: String,
    pub freq_khz: f64,
    pub call: String,
    /// Whitespace-collapsed comment.
    pub comment: String,
    /// "HHMM" UTC as sent by the node, may be empty
    pub time: String,
    /// Spotter's grid/locator if the line carries one (CC-Cluster/AR-Cluster sometimes append it), else empty.
    pub spotter_grid: String,
}

/// What the cluster connection reports.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClusterEvent {
    /// A TCP connection to `node` (its name) is being opened.
    Connecting { node: String },
    /// The TCP connection is up; login follows.
    Connected { node: String },
    /// The connection failed or dropped; the next node is tried after a delay.
    Disconnected { node: String, reason: String },
    /// Every non-empty line received (and login prompts that lack a newline), for the console view.
    Line { text: String },
    /// A line that parsed as a spot (also sent as [`ClusterEvent::Line`]).
    Spot(Spot),
}

/// Handle to the background cluster connection. Dropping it disconnects.
pub struct ClusterHandle {
    events: broadcast::Sender<ClusterEvent>,
    cmds: mpsc::UnboundedSender<String>,
    status: Arc<Mutex<Option<ClusterEvent>>>,
    task: JoinHandle<()>,
}

/// Connects to the first node, logs in as `callsign`, sends the node's commands, and streams events. On disconnect or failure it moves on to the next node in the list (wrapping), waiting 10 s between attempts (exponential up to 2 min if all fail). Dropping the handle disconnects.
///
/// Must be called inside a tokio runtime. With an empty node list nothing happens.
pub fn connect(nodes: Vec<ClusterNode>, callsign: String) -> ClusterHandle {
    connect_with(nodes, callsign, Timing::default())
}

impl ClusterHandle {
    /// Events from now on. Subscribe right after [`connect`]; see [`ClusterHandle::status`] for the state at subscribe time.
    pub fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
        self.events.subscribe()
    }

    /// Sends a raw command line to the node (e.g. "sh/dx 20", or a spot "dx 14025 K1ABC tnx qso").
    /// Dropped when not connected.
    pub fn send(&self, line: &str) {
        let _ = self
            .cmds
            .send(line.trim_end_matches(['\r', '\n']).to_string());
    }

    /// The latest `Connecting`, `Connected` or `Disconnected` event, None before the first attempt.
    pub fn status(&self) -> Option<ClusterEvent> {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for ClusterHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// ---------------------------------------------------------------------------------------------
// Spot parsing

/// Parses one cluster line: "DX de K1ABC:     14025.0  JA1XYZ       CW 22 dB 25 WPM CQ             1234Z FN42" and the many variants (spotter with -#/-@ skimmer suffix like "DL8LAS-#:", freq with or without decimals, comment empty, time "1234Z", optional trailing locator, lines from "sh/dx" responses in the form "  14025.0  JA1XYZ      7-Oct-2026 1234Z  comment   <K1ABC>"). Returns None for anything else.
pub fn parse_spot(line: &str) -> Option<Spot> {
    let line = line.trim_matches(|c: char| c.is_whitespace() || c.is_control());
    match line.get(..6) {
        Some(p) if p.eq_ignore_ascii_case("DX de ") => parse_dx_de(&line[6..]),
        _ => parse_show_dx(line),
    }
}

/// "K1ABC:  14025.0  JA1XYZ  comment  1234Z FN42" (after "DX de ").
fn parse_dx_de(rest: &str) -> Option<Spot> {
    let (spotter, rest) = rest.split_once(':')?;
    let spotter = spotter.trim();
    if spotter.is_empty()
        || !spotter
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-#@".contains(c))
    {
        return None;
    }
    let mut toks: Vec<&str> = rest.split_whitespace().collect();
    let (freq_khz, call) = freq_and_call(&toks)?;
    toks.drain(..2);
    // Time ("1234Z") near the end, optionally followed by a locator (or other short trailers).
    let mut time = String::new();
    let mut spotter_grid = String::new();
    let tail_start = toks.len().saturating_sub(4);
    if let Some(i) = (tail_start..toks.len())
        .rev()
        .find(|&i| split_time(toks[i]).is_some())
    {
        let (before, t) = split_time(toks[i])?;
        time = t.to_string();
        if let Some(g) = toks.get(i + 1).and_then(|g| grid(g)) {
            spotter_grid = g;
        }
        toks.truncate(i);
        if !before.is_empty() {
            toks.push(before);
        }
    }
    Some(Spot {
        spotter: spotter.to_string(),
        freq_khz,
        call,
        comment: toks.join(" "),
        time,
        spotter_grid,
    })
}

/// sh/dx format: "14025.0  JA1XYZ  7-Oct-2026 1234Z  comment  <K1ABC>" (date optional, locator after the spotter allowed).
fn parse_show_dx(line: &str) -> Option<Spot> {
    let mut toks: Vec<&str> = line.split_whitespace().collect();
    let (freq_khz, call) = freq_and_call(&toks)?;
    let mut spotter_grid = String::new();
    if toks.len() > 3 {
        if let Some(g) = grid(toks[toks.len() - 1]) {
            if toks[toks.len() - 2].starts_with('<') {
                spotter_grid = g;
                toks.pop();
            }
        }
    }
    let spotter = toks.pop()?.strip_prefix('<')?.strip_suffix('>')?;
    if spotter.is_empty()
        || !spotter
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-#@".contains(c))
    {
        return None;
    }
    let mut i = 2;
    if toks.get(i).is_some_and(|d| is_date(d)) {
        i += 1;
    }
    let (before, time) = split_time(toks.get(i)?)?;
    if !before.is_empty() {
        return None;
    }
    Some(Spot {
        spotter: spotter.to_string(),
        freq_khz,
        call,
        comment: toks.get(i + 1..).unwrap_or_default().join(" "),
        time: time.to_string(),
        spotter_grid,
    })
}

fn freq_and_call(toks: &[&str]) -> Option<(f64, String)> {
    let f = *toks.first()?;
    if !f.chars().all(|c| c.is_ascii_digit() || c == '.')
        || !f.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    let freq: f64 = f.parse().ok()?;
    let call = toks.get(1)?.to_ascii_uppercase();
    (freq > 0.0 && freq < 1e8 && is_call(&call)).then_some((freq, call))
}

/// Loose callsign check: letters, digits and '/', at least one of each kind.
fn is_call(s: &str) -> bool {
    s.len() >= 3
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '/')
        && s.chars().any(|c| c.is_ascii_digit())
        && s.chars().any(|c| c.is_ascii_alphabetic())
}

/// Splits "1234Z" (or "comment1234Z" when the comment runs into the time) into (prefix, "1234").
fn split_time(tok: &str) -> Option<(&str, &str)> {
    let body = tok.strip_suffix(['Z', 'z'])?;
    let split = body.len().checked_sub(4)?;
    let (before, hhmm) = (body.get(..split)?, body.get(split..)?);
    if !hhmm.bytes().all(|b| b.is_ascii_digit()) || before.ends_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let (h, m): (u8, u8) = (hhmm[..2].parse().ok()?, hhmm[2..].parse().ok()?);
    (h < 24 && m < 60).then_some((before, hhmm))
}

/// Maidenhead locator (4 or 6 characters), optionally wrapped in brackets; returned uppercase.
fn grid(tok: &str) -> Option<String> {
    let g = tok
        .trim_matches(|c| "[]()<>".contains(c))
        .to_ascii_uppercase();
    let b = g.as_bytes();
    let ok = matches!(b.len(), 4 | 6)
        && b[..2].iter().all(|c| (b'A'..=b'R').contains(c))
        && b[2..4].iter().all(u8::is_ascii_digit)
        && b[4..].iter().all(|c| (b'A'..=b'X').contains(c));
    ok.then_some(g)
}

/// "7-Oct-2026" / "07-Oct-2026".
fn is_date(s: &str) -> bool {
    let p: Vec<&str> = s.split('-').collect();
    matches!(p.as_slice(), [d, m, y]
        if (1..=2).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit())
        && m.len() == 3 && m.bytes().all(|b| b.is_ascii_alphabetic())
        && y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()))
}

// ---------------------------------------------------------------------------------------------
// Mode guessing

/// Dial frequencies (kHz) of the common digital modes. A frequency counts when it lies 1 kHz below
/// to 3 kHz above a dial (spots usually carry dial + audio offset); among several matches the
/// highest dial not above the frequency wins.
#[rustfmt::skip]
const DIALS: &[(f64, &str)] = &[
    (1840.0, "FT8"), (3573.0, "FT8"), (5357.0, "FT8"), (7074.0, "FT8"), (10136.0, "FT8"),
    (14074.0, "FT8"), (18100.0, "FT8"), (21074.0, "FT8"), (24915.0, "FT8"), (28074.0, "FT8"),
    (50313.0, "FT8"), (50323.0, "FT8"), (70154.0, "FT8"), (144174.0, "FT8"), (222065.0, "FT8"),
    (432174.0, "FT8"), (1296174.0, "FT8"),
    (3575.0, "FT4"), (7047.5, "FT4"), (10140.0, "FT4"), (14080.0, "FT4"), (18104.0, "FT4"),
    (21140.0, "FT4"), (24919.0, "FT4"), (28180.0, "FT4"), (50318.0, "FT4"), (144170.0, "FT4"),
    (3578.0, "JS8"), (7078.0, "JS8"), (14078.0, "JS8"), (21078.0, "JS8"), (28078.0, "JS8"),
    (50260.0, "MSK144"), (50280.0, "MSK144"), (144150.0, "MSK144"), (144360.0, "MSK144"),
];

/// PSK31 centres (kHz), matched within ±1 kHz before the dials above.
const PSK31: &[f64] = &[3580.0, 7070.0, 10142.0, 14070.0, 21070.0, 28120.0];

/// Band-plan segments `[lo, hi)` in kHz, a blend of IARU Region 1 and the US plan. Narrowband data
/// segments map to "RTTY" (their most common spotted mode), e.g. 7040-7060 → RTTY.
#[rustfmt::skip]
const SEGMENTS: &[(f64, f64, &str)] = &[
    (135.7, 137.8, "CW"), (472.0, 479.0, "CW"),
    (1800.0, 1838.0, "CW"), (1838.0, 1843.0, "RTTY"), (1843.0, 2000.0, "SSB"),
    (3500.0, 3570.0, "CW"), (3570.0, 3600.0, "RTTY"), (3600.0, 4000.0, "SSB"),
    (7000.0, 7040.0, "CW"), (7040.0, 7060.0, "RTTY"), (7060.0, 7300.0, "SSB"),
    (10100.0, 10130.0, "CW"), (10130.0, 10150.0, "RTTY"),
    (14000.0, 14070.0, "CW"), (14070.0, 14112.0, "RTTY"), (14112.0, 14350.0, "SSB"),
    (18068.0, 18095.0, "CW"), (18095.0, 18111.0, "RTTY"), (18111.0, 18168.0, "SSB"),
    (21000.0, 21070.0, "CW"), (21070.0, 21151.0, "RTTY"), (21151.0, 21450.0, "SSB"),
    (24890.0, 24915.0, "CW"), (24915.0, 24931.0, "RTTY"), (24931.0, 24990.0, "SSB"),
    (28000.0, 28070.0, "CW"), (28070.0, 28190.0, "RTTY"), (28190.0, 28225.0, "CW"),
    (28300.0, 29000.0, "SSB"), (29520.0, 29700.0, "FM"),
    (50000.0, 50100.0, "CW"), (50100.0, 50300.0, "SSB"), (50400.0, 50500.0, "CW"), (51000.0, 54000.0, "FM"),
    (144000.0, 144110.0, "CW"), (144150.0, 144400.0, "SSB"), (145000.0, 145800.0, "FM"), (146000.0, 148000.0, "FM"),
    (432000.0, 432100.0, "CW"), (432100.0, 432400.0, "SSB"),
    (1296000.0, 1296150.0, "CW"), (1296150.0, 1296400.0, "SSB"),
];

/// Best guess at the mode from frequency and comment: comment keywords first (CW, SSB/USB/LSB, FT8, FT4, RTTY, PSK31, JS8, Q65, MSK144, FM, AM, SSTV, "dB  WPM" skimmer → CW), then the IARU/US band plans (CW sub-bands, the FT8/FT4 dial frequencies ±3 kHz, else SSB above the CW/data segment). Returns an ADIF-style mode label as used in the UI: "CW", "SSB", "FT8", "FT4", "RTTY", "PSK31", "JS8", "Q65", "MSK144", "FM", "AM", "SSTV", or "" if unsure.
///
/// Details: digital dials (see `DIALS`) match from 1 kHz below to 3 kHz above; narrowband data
/// segments without a known dial give "RTTY" (so 7040 → RTTY, 7035 → CW); 60 m and unplanned
/// frequencies give "".
pub fn guess_mode(freq_khz: f64, comment: &str) -> &'static str {
    let words: Vec<String> = comment
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    for w in &words {
        let m = match w.as_str() {
            "CW" => "CW",
            "SSB" | "USB" | "LSB" => "SSB",
            "FT8" => "FT8",
            "FT4" => "FT4",
            "RTTY" => "RTTY",
            "PSK31" | "BPSK31" | "PSK" => "PSK31",
            "JS8" | "JS8CALL" => "JS8",
            "Q65" => "Q65",
            "MSK144" => "MSK144",
            "FM" => "FM",
            "AM" => "AM",
            "SSTV" => "SSTV",
            _ => continue,
        };
        return m;
    }
    let has = |k: &str| words.iter().any(|w| w == k);
    if has("WPM") {
        return "CW";
    }
    if has("BPS") {
        return "RTTY";
    }
    let f = freq_khz;
    if PSK31.iter().any(|&c| (f - c).abs() <= 1.0) {
        return "PSK31";
    }
    let hits = DIALS.iter().filter(|&&(d, _)| f >= d - 1.0 && f <= d + 3.0);
    let below = hits
        .clone()
        .filter(|&&(d, _)| d <= f)
        .max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some(&(_, m)) = below.or_else(|| hits.min_by(|a, b| a.0.total_cmp(&b.0))) {
        return m;
    }
    SEGMENTS
        .iter()
        .find(|&&(lo, hi, _)| f >= lo && f < hi)
        .map_or("", |s| s.2)
}

// ---------------------------------------------------------------------------------------------
// Connection

/// Timings, shortened in tests.
#[derive(Clone, Copy, Debug)]
struct Timing {
    /// Delay between attempts while the failures have not yet gone round the whole list.
    retry: Duration,
    /// Cap of the exponential back-off once every node failed.
    max_retry: Duration,
    /// A session that stayed up this long resets the back-off.
    stable: Duration,
    connect: Duration,
    /// Send the callsign anyway (and stop waiting for a password prompt) after this.
    prompt_wait: Duration,
    /// How long a partial line must sit idle before it is checked for a prompt.
    partial_idle: Duration,
    keepalive: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            retry: Duration::from_secs(10),
            max_retry: Duration::from_secs(120),
            stable: Duration::from_secs(60),
            connect: Duration::from_secs(10),
            prompt_wait: Duration::from_secs(3),
            partial_idle: Duration::from_millis(250),
            keepalive: Duration::from_secs(300),
        }
    }
}

impl Timing {
    /// Back-off after `fails` consecutive failures over `n` nodes: `retry` until every node failed
    /// once, then doubling per full round up to `max_retry`.
    fn backoff(&self, fails: u32, n: usize) -> Duration {
        let rounds = fails / n.max(1) as u32;
        self.retry
            .saturating_mul(1 << rounds.min(16))
            .min(self.max_retry.max(self.retry))
    }
}

#[derive(Clone)]
struct Events {
    tx: broadcast::Sender<ClusterEvent>,
    status: Arc<Mutex<Option<ClusterEvent>>>,
}

impl Events {
    fn emit(&self, ev: ClusterEvent) {
        if !matches!(ev, ClusterEvent::Line { .. } | ClusterEvent::Spot(_)) {
            *self.status.lock().unwrap() = Some(ev.clone());
        }
        let _ = self.tx.send(ev);
    }
}

fn connect_with(nodes: Vec<ClusterNode>, callsign: String, timing: Timing) -> ClusterHandle {
    let (events, _) = broadcast::channel(1024);
    let (cmds, cmd_rx) = mpsc::unbounded_channel();
    let status = Arc::new(Mutex::new(None));
    let ev = Events {
        tx: events.clone(),
        status: status.clone(),
    };
    let task = tokio::spawn(run(nodes, callsign, timing, ev, cmd_rx));
    ClusterHandle {
        events,
        cmds,
        status,
        task,
    }
}

async fn run(
    nodes: Vec<ClusterNode>,
    callsign: String,
    t: Timing,
    ev: Events,
    mut cmds: mpsc::UnboundedReceiver<String>,
) {
    let mut fails = 0u32;
    for node in nodes.iter().cycle() {
        ev.emit(ClusterEvent::Connecting {
            node: node.name.clone(),
        });
        let started = Instant::now();
        let mut up = false;
        let result = session(node, &callsign, &t, &ev, &mut cmds, &mut up).await;
        let reason = match result {
            Ok(true) => return,
            Ok(false) => "connection closed by the node".to_string(),
            Err(e) => format!("{e:#}"),
        };
        tracing::info!("cluster {}: {reason}", node.name);
        ev.emit(ClusterEvent::Disconnected {
            node: node.name.clone(),
            reason,
        });
        fails = if up && started.elapsed() >= t.stable {
            0
        } else {
            fails.saturating_add(1)
        };
        // Commands sent while disconnected are dropped rather than replayed later.
        let wait = tokio::time::sleep(t.backoff(fails, nodes.len()));
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                cmd = cmds.recv() => if cmd.is_none() { return },
            }
        }
    }
}

/// Login progress.
enum Login {
    /// Waiting for a callsign prompt until the deadline.
    Call(Instant),
    /// Callsign sent; waiting for a password prompt until the deadline.
    Password(Instant),
    Done,
}

struct Session<'a> {
    node: &'a ClusterNode,
    call: &'a str,
    t: &'a Timing,
    w: OwnedWriteHalf,
    login: Login,
}

impl Session<'_> {
    async fn send(&mut self, line: &str) -> Result<()> {
        self.w.write_all(format!("{line}\r\n").as_bytes()).await?;
        Ok(())
    }

    /// Reacts to a (possibly partial) line; true when it was a login prompt.
    async fn on_text(&mut self, text: &str) -> Result<bool> {
        match self.login {
            Login::Call(_) if is_call_prompt(text) => self.call_sent().await.map(|_| true),
            Login::Password(_) if is_password_prompt(text) => {
                let pw = self.node.password.clone();
                self.send(&pw).await?;
                self.finish().await.map(|_| true)
            }
            _ => Ok(false),
        }
    }

    async fn on_deadline(&mut self) -> Result<()> {
        match self.login {
            Login::Call(_) => self.call_sent().await,
            Login::Password(_) => self.finish().await,
            Login::Done => Ok(()),
        }
    }

    async fn call_sent(&mut self) -> Result<()> {
        let call = self.call.to_string();
        self.send(&call).await?;
        if self.node.password.is_empty() {
            self.finish().await
        } else {
            self.login = Login::Password(Instant::now() + self.t.prompt_wait);
            Ok(())
        }
    }

    async fn finish(&mut self) -> Result<()> {
        self.login = Login::Done;
        for cmd in self.node.commands.clone() {
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                self.send(cmd).await?;
            }
        }
        Ok(())
    }
}

fn is_call_prompt(text: &str) -> bool {
    let t = text.trim().to_ascii_lowercase();
    t.len() < 80
        && (t.ends_with(':') || t.ends_with('>') || t.ends_with('?'))
        && (t.contains("login") || t.contains("call"))
}

fn is_password_prompt(text: &str) -> bool {
    let t = text.trim().to_ascii_lowercase();
    t.len() < 80 && t.ends_with(':') && t.contains("password")
}

/// One connection. `Ok(true)` when the handle was dropped, `Ok(false)` when the node closed it.
async fn session(
    node: &ClusterNode,
    callsign: &str,
    t: &Timing,
    ev: &Events,
    cmds: &mut mpsc::UnboundedReceiver<String>,
    up: &mut bool,
) -> Result<bool> {
    let stream = timeout(
        t.connect,
        TcpStream::connect((node.host.as_str(), node.port)),
    )
    .await
    .map_err(|_| anyhow!("connection to {}:{} timed out", node.host, node.port))?
    .map_err(|e| anyhow!("{}:{}: {e}", node.host, node.port))?;
    let _ = stream.set_nodelay(true);
    *up = true;
    ev.emit(ClusterEvent::Connected {
        node: node.name.clone(),
    });
    let (mut r, w) = stream.into_split();
    let call = if node.login.trim().is_empty() {
        callsign.trim()
    } else {
        node.login.trim()
    };
    let mut s = Session {
        node,
        call,
        t,
        w,
        login: Login::Call(Instant::now() + t.prompt_wait),
    };
    let mut telnet = Telnet::default();
    let mut buf = [0u8; 4096];
    let (mut data, mut reply, mut partial) = (Vec::new(), Vec::new(), Vec::new());
    let mut last_rx = Instant::now();
    let mut partial_checked = true;
    loop {
        let login_deadline = match s.login {
            Login::Call(d) | Login::Password(d) => Some(d),
            Login::Done => None,
        };
        tokio::select! {
            n = r.read(&mut buf) => {
                let n = n?;
                if n == 0 {
                    return Ok(false);
                }
                last_rx = Instant::now();
                telnet.feed(&buf[..n], &mut data, &mut reply);
                if !reply.is_empty() {
                    s.w.write_all(&reply).await?;
                    reply.clear();
                }
                for &b in &data {
                    match b {
                        b'\n' => {
                            let text = decode(&partial);
                            partial.clear();
                            if text.trim().is_empty() {
                                continue;
                            }
                            s.on_text(&text).await?;
                            emit_line(ev, text);
                        }
                        b'\t' => partial.push(b),
                        0..=0x1f | 0x7f => {} // CR, bell, NUL and other controls
                        _ => partial.push(b),
                    }
                }
                data.clear();
                partial_checked = partial.is_empty();
            }
            _ = sleep_until(last_rx + t.partial_idle), if !partial_checked => {
                partial_checked = true;
                let text = decode(&partial);
                if s.on_text(&text).await? {
                    partial.clear();
                    emit_line(ev, text);
                }
            }
            _ = sleep_until(login_deadline.unwrap_or(last_rx)), if login_deadline.is_some() => {
                s.on_deadline().await?;
            }
            _ = sleep_until(last_rx + t.keepalive) => {
                s.w.write_all(b"\r\n").await?;
                last_rx = Instant::now();
            }
            cmd = cmds.recv() => match cmd {
                Some(line) => s.send(&line).await?,
                None => return Ok(true),
            },
        }
    }
}

fn emit_line(ev: &Events, text: String) {
    let spot = parse_spot(&text);
    ev.emit(ClusterEvent::Line {
        text: text.trim_end().to_string(),
    });
    if let Some(spot) = spot {
        ev.emit(ClusterEvent::Spot(spot));
    }
}

/// UTF-8, falling back to Latin-1 (which some nodes send) when the bytes are not valid UTF-8.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;

/// Strips telnet negotiation and refuses every option (DO → WONT, WILL → DONT, each answered once).
#[derive(Default)]
struct Telnet {
    state: TState,
    answered: Vec<(u8, u8)>,
}

#[derive(Default, Clone, Copy)]
enum TState {
    #[default]
    Data,
    Iac,
    Opt(u8),
    Sub,
    SubIac,
}

impl Telnet {
    fn feed(&mut self, input: &[u8], data: &mut Vec<u8>, reply: &mut Vec<u8>) {
        for &b in input {
            self.state = match (self.state, b) {
                (TState::Data, IAC) => TState::Iac,
                (TState::Data, _) => {
                    data.push(b);
                    TState::Data
                }
                (TState::Iac, IAC) => {
                    data.push(IAC);
                    TState::Data
                }
                (TState::Iac, WILL | WONT | DO | DONT) => TState::Opt(b),
                (TState::Iac, SB) => TState::Sub,
                (TState::Iac, _) => TState::Data,
                (TState::Opt(verb), opt) => {
                    let answer = match verb {
                        DO => Some(WONT),
                        WILL => Some(DONT),
                        _ => None,
                    };
                    if let Some(a) = answer {
                        if !self.answered.contains(&(a, opt)) {
                            self.answered.push((a, opt));
                            reply.extend([IAC, a, opt]);
                        }
                    }
                    TState::Data
                }
                (TState::Sub, IAC) => TState::SubIac,
                (TState::Sub, _) => TState::Sub,
                (TState::SubIac, SE) => TState::Data,
                (TState::SubIac, _) => TState::Sub,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::TcpListener;

    fn spot(
        spotter: &str,
        freq: f64,
        call: &str,
        comment: &str,
        time: &str,
        grid: &str,
    ) -> Option<Spot> {
        Some(Spot {
            spotter: spotter.into(),
            freq_khz: freq,
            call: call.into(),
            comment: comment.into(),
            time: time.into(),
            spotter_grid: grid.into(),
        })
    }

    #[test]
    fn dxspider_lines() {
        assert_eq!(
            parse_spot(
                "DX de K1ABC:     14025.0  JA1XYZ       CW 22 dB 25 WPM CQ             1234Z FN42"
            ),
            spot(
                "K1ABC",
                14025.0,
                "JA1XYZ",
                "CW 22 dB 25 WPM CQ",
                "1234",
                "FN42"
            )
        );
        assert_eq!(
            parse_spot(
                "DX de G4ABC:     7005.5  ZS6XYZ       tnx qso 599                    0812Z"
            ),
            spot("G4ABC", 7005.5, "ZS6XYZ", "tnx qso 599", "0812", "")
        );
        // Empty comment, integer frequency, bell and CR around it.
        assert_eq!(
            parse_spot(
                "DX de PA3ABC:    28490  VP8/G3XYZ                                     2359Z\x07\r"
            ),
            spot("PA3ABC", 28490.0, "VP8/G3XYZ", "", "2359", "")
        );
        // Lower-case call is normalised; comment running into the time.
        assert_eq!(
            parse_spot("DX de W1AW:      3799.0  k4abc        long comment that was cut here1801Z"),
            spot(
                "W1AW",
                3799.0,
                "K4ABC",
                "long comment that was cut here",
                "1801",
                ""
            )
        );
        // Microwave frequencies and 6-character grid.
        assert_eq!(
            parse_spot("DX de OK1ABC:  10368100.0  DL0XYZ  EME  1000Z JN79fx"),
            spot("OK1ABC", 10_368_100.0, "DL0XYZ", "EME", "1000", "JN79FX")
        );
    }

    #[test]
    fn skimmer_and_cc_cluster_lines() {
        assert_eq!(
            parse_spot(
                "DX de DL8LAS-#:  14025.0  JA1XYZ       CW 12 dB 22 WPM CQ             1234Z"
            ),
            spot(
                "DL8LAS-#",
                14025.0,
                "JA1XYZ",
                "CW 12 dB 22 WPM CQ",
                "1234",
                ""
            )
        );
        assert_eq!(
            parse_spot("DX de W3LPL-2-#: 7014.20  K1ABC        CW 25 dB 30 WPM CQ      2201Z FM19"),
            spot(
                "W3LPL-2-#",
                7014.2,
                "K1ABC",
                "CW 25 dB 30 WPM CQ",
                "2201",
                "FM19"
            )
        );
        assert_eq!(
            parse_spot(
                "DX de KM3T-@:    14080.0  EA8ABC       FT4 -12 dB CQ                  0101Z"
            ),
            spot("KM3T-@", 14080.0, "EA8ABC", "FT4 -12 dB CQ", "0101", "")
        );
        // CC-Cluster with locator in brackets.
        assert_eq!(
            parse_spot("DX de VE7CC:     14195.0  3Y0J         up 5                           1530Z [CN89]"),
            spot("VE7CC", 14195.0, "3Y0J", "up 5", "1530", "CN89")
        );
        // A comment that ends in digits is not mistaken for a time.
        assert_eq!(
            parse_spot("DX de K1ABC: 14025 JA1XYZ qrz 21234Z")
                .unwrap()
                .time,
            ""
        );
        // No time at all.
        assert_eq!(
            parse_spot("DX de K1ABC: 14025 JA1XYZ hello"),
            spot("K1ABC", 14025.0, "JA1XYZ", "hello", "", "")
        );
    }

    #[test]
    fn show_dx_lines() {
        assert_eq!(
            parse_spot("  14025.0  JA1XYZ      7-Oct-2026 1234Z  comment here   <K1ABC>"),
            spot("K1ABC", 14025.0, "JA1XYZ", "comment here", "1234", "")
        );
        assert_eq!(
            parse_spot("14195.0 3Y0J 07-Oct-2026 0915Z  <DL1ABC-#>"),
            spot("DL1ABC-#", 14195.0, "3Y0J", "", "0915", "")
        );
        assert_eq!(
            parse_spot(" 1840.0  JA1XYZ  0001Z FT8 -10 dB  <K1ABC> FN42"),
            spot("K1ABC", 1840.0, "JA1XYZ", "FT8 -10 dB", "0001", "FN42")
        );
    }

    #[test]
    fn rejects_other_lines() {
        for line in [
            "",
            "DX de K1ABC: hello world",
            "DX de K1ABC:     14025.0",
            "DX de :     14025.0  JA1XYZ   1234Z",
            "DX de K1 ABC:     14025.0  JA1XYZ   1234Z",
            "DX de K1ABC:     14025.0  QRZ?   1234Z",
            "DX de K1ABC:     -14025.0  JA1XYZ   1234Z",
            "To ALL de K1ABC: anyone on 14025?",
            "WWV de W0MU <18>:   SFI=150, A=5, K=1, No Storms -> No Storms",
            "K1ABC de GB7DJK  7-Oct-2026 1234Z dxspider >",
            "  14025.0  JA1XYZ      7-Oct-2026 1234Z  comment   K1ABC",
            "  14025.0  JA1XYZ      7-Oct-2026 99:99  comment   <K1ABC>",
            "Please enter your call:",
        ] {
            assert_eq!(parse_spot(line), None, "{line:?}");
        }
    }

    #[test]
    fn modes() {
        #[rustfmt::skip]
        let cases = [
            (14074.0, "FT8"), (14075.6, "FT8"), (14080.0, "FT4"), (14081.2, "FT4"), (7074.0, "FT8"),
            (7076.3, "FT8"), (14025.0, "CW"), (14250.0, "SSB"), (3573.0, "FT8"), (10136.0, "FT8"),
            (50313.0, "FT8"), (28074.0, "FT8"), (21074.0, "FT8"), (18100.0, "FT8"), (1840.0, "FT8"),
            (144174.0, "FT8"), (7040.0, "RTTY"), (7035.0, "CW"), (7150.0, "SSB"), (3790.0, "SSB"),
            (3520.0, "CW"), (1825.0, "CW"), (10115.0, "CW"), (14078.5, "JS8"), (14070.5, "PSK31"),
            (14085.0, "RTTY"), (21300.0, "SSB"), (28500.0, "SSB"), (50150.0, "SSB"), (50050.0, "CW"),
            (144300.0, "SSB"), (145500.0, "FM"), (29600.0, "FM"), (5357.0, "FT8"), (5370.0, ""),
            (50280.5, "MSK144"), (24915.5, "FT8"), (24919.5, "FT4"), (0.0, ""), (2_400_000.0, ""),
        ];
        for (f, m) in cases {
            assert_eq!(guess_mode(f, ""), m, "{f}");
        }
        assert_eq!(guess_mode(14074.0, "FT4 -3 dB"), "FT4");
        assert_eq!(guess_mode(14025.0, "CW 12 dB 22 WPM CQ"), "CW");
        assert_eq!(guess_mode(14090.0, "26 dB 22 WPM"), "CW");
        assert_eq!(guess_mode(14085.0, "RTTY 15 dB 45 BPS CQ"), "RTTY");
        assert_eq!(guess_mode(14200.0, "sstv pic rcvd"), "SSTV");
        assert_eq!(guess_mode(3700.0, "lsb up 5"), "SSB");
        assert_eq!(guess_mode(7076.0, "js8call"), "JS8");
        assert_eq!(guess_mode(50275.0, "Q65 EME"), "Q65");
        assert_eq!(guess_mode(29000.0, "AM net"), "AM");
        assert_eq!(guess_mode(14190.0, "FM19 tnx"), "SSB");
    }

    #[test]
    fn telnet_negotiation() {
        let mut t = Telnet::default();
        let (mut data, mut reply) = (Vec::new(), Vec::new());
        t.feed(
            &[
                b'a', IAC, DO, 1, b'b', IAC, WILL, 3, IAC, IAC, IAC, SB, 24, 1, IAC,
            ],
            &mut data,
            &mut reply,
        );
        t.feed(&[SE, b'c', IAC, DO, 1, IAC, WONT, 5], &mut data, &mut reply);
        assert_eq!(data, [b'a', b'b', 0xff, b'c']);
        assert_eq!(reply, [IAC, WONT, 1, IAC, DONT, 3]);
    }

    #[test]
    fn decode_falls_back_to_latin1() {
        assert_eq!(decode("café".as_bytes()), "café");
        assert_eq!(decode(b"caf\xe9"), "café");
    }

    #[test]
    fn backoff() {
        let t = Timing::default();
        assert_eq!(t.backoff(0, 3), Duration::from_secs(10));
        assert_eq!(t.backoff(2, 3), Duration::from_secs(10));
        assert_eq!(t.backoff(3, 3), Duration::from_secs(20));
        assert_eq!(t.backoff(6, 3), Duration::from_secs(40));
        assert_eq!(t.backoff(100, 3), Duration::from_secs(120));
    }

    fn test_timing() -> Timing {
        Timing {
            retry: Duration::from_millis(50),
            max_retry: Duration::from_millis(200),
            prompt_wait: Duration::from_millis(300),
            partial_idle: Duration::from_millis(50),
            ..Timing::default()
        }
    }

    fn node(name: &str, port: u16) -> ClusterNode {
        ClusterNode {
            name: name.into(),
            host: "127.0.0.1".into(),
            port,
            login: String::new(),
            password: String::new(),
            commands: vec!["set/dx/filter".into(), "sh/dx 5".into()],
        }
    }

    /// A node that sends `greeting` (no newline), records the first `expect` lines it receives,
    /// then sends `after` and keeps reading until the client goes away.
    async fn mock_node(
        greeting: &'static [u8],
        expect: usize,
        after: String,
    ) -> (u16, mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let (r, mut w) = sock.into_split();
            w.write_all(greeting).await.unwrap();
            let mut lines = BufReader::new(r).lines();
            let mut n = 0;
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line.trim_end_matches('\r').to_string());
                n += 1;
                if n == expect {
                    w.write_all(after.as_bytes()).await.unwrap();
                }
            }
        });
        (port, rx)
    }

    async fn next_event(rx: &mut broadcast::Receiver<ClusterEvent>) -> ClusterEvent {
        timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("event timeout")
            .unwrap()
    }

    async fn next_line(rx: &mut mpsc::UnboundedReceiver<String>) -> String {
        timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("line timeout")
            .unwrap()
    }

    const SPOTS: &str = "DX de K1ABC:     14025.0  JA1XYZ       CW 22 dB 25 WPM CQ             1234Z FN42\r\n\
                         \x07DX de DL8LAS-#:  7074.0  ZS6XYZ       FT8 -12 dB                     1235Z\r\n";

    #[tokio::test]
    async fn logs_in_and_streams_spots() {
        let (port, mut sent) = mock_node(
            b"Welcome to TEST\r\n\r\nPlease enter your call: ",
            3,
            format!("Hello N0CALL\r\n{SPOTS}"),
        )
        .await;
        let handle = connect_with(vec![node("test", port)], "N0CALL".into(), test_timing());
        let mut rx = handle.subscribe();
        assert_eq!(
            next_event(&mut rx).await,
            ClusterEvent::Connecting {
                node: "test".into()
            }
        );
        assert_eq!(
            next_event(&mut rx).await,
            ClusterEvent::Connected {
                node: "test".into()
            }
        );
        assert_eq!(
            handle.status(),
            Some(ClusterEvent::Connected {
                node: "test".into()
            })
        );
        assert_eq!(next_line(&mut sent).await, "N0CALL");
        assert_eq!(next_line(&mut sent).await, "set/dx/filter");
        assert_eq!(next_line(&mut sent).await, "sh/dx 5");
        let mut lines = Vec::new();
        let mut spots = Vec::new();
        while spots.len() < 2 {
            match next_event(&mut rx).await {
                ClusterEvent::Line { text } => lines.push(text),
                ClusterEvent::Spot(s) => spots.push(s),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(
            lines[..3],
            ["Welcome to TEST", "Please enter your call:", "Hello N0CALL"]
        );
        assert_eq!(lines.len(), 5);
        assert_eq!(spots[0].call, "JA1XYZ");
        assert_eq!(spots[0].spotter_grid, "FN42");
        assert_eq!(spots[1].spotter, "DL8LAS-#");
        assert_eq!(guess_mode(spots[1].freq_khz, &spots[1].comment), "FT8");
        handle.send("dx 14025 K1ABC tnx qso\n");
        assert_eq!(next_line(&mut sent).await, "dx 14025 K1ABC tnx qso");
        drop(handle);
        // The node sees the connection close.
        assert!(timeout(Duration::from_secs(5), sent.recv())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn password_and_no_prompt() {
        // No callsign prompt at all: the call goes out after prompt_wait, then the password prompt.
        let (port, mut sent) = mock_node(b"DXSpider node\r\n", 1, "password: ".into()).await;
        let mut n = node("pw", port);
        n.login = "N0CALL-2".into();
        n.password = "secret".into();
        let handle = connect_with(vec![n], "N0CALL".into(), test_timing());
        assert_eq!(next_line(&mut sent).await, "N0CALL-2");
        assert_eq!(next_line(&mut sent).await, "secret");
        assert_eq!(next_line(&mut sent).await, "set/dx/filter");
        drop(handle);
    }

    #[tokio::test]
    async fn fails_over_to_next_node() {
        let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_port = closed.local_addr().unwrap().port();
        drop(closed);
        let (port, mut sent) = mock_node(b"login: ", 1, SPOTS.into()).await;
        let handle = connect_with(
            vec![node("dead", dead_port), node("live", port)],
            "N0CALL".into(),
            test_timing(),
        );
        let mut rx = handle.subscribe();
        assert_eq!(
            next_event(&mut rx).await,
            ClusterEvent::Connecting {
                node: "dead".into()
            }
        );
        assert!(
            matches!(next_event(&mut rx).await, ClusterEvent::Disconnected { node, .. } if node == "dead")
        );
        assert_eq!(
            next_event(&mut rx).await,
            ClusterEvent::Connecting {
                node: "live".into()
            }
        );
        assert_eq!(
            next_event(&mut rx).await,
            ClusterEvent::Connected {
                node: "live".into()
            }
        );
        assert_eq!(next_line(&mut sent).await, "N0CALL");
        loop {
            if let ClusterEvent::Spot(s) = next_event(&mut rx).await {
                assert_eq!(s.call, "JA1XYZ");
                break;
            }
        }
    }
}
