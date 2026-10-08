//! UDP connections the user sets up for programs QRZero doesn't know by name: antenna switches,
//! band decoders, rotator software, their own scripts. Each one sends a message to a host and port
//! when something happens (a QSO is logged, a radio changes frequency, ...), in a format that
//! program understands. A connection can instead relay the raw WSJT-X or N1MM packets QRZero
//! receives, so several programs can share one stream.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use qrzero_core::adif::{self, Fields};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::UdpSocket;

/// Where the connections are kept.
pub const SETTING_KEY: &str = "udp_connections";
/// Radio messages to one connection are at most this often; the latest state follows the gap.
const RADIO_GAP: Duration = Duration::from_millis(250);

/// What makes a connection send.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// A QSO was logged (from the QSO panel or another program).
    #[default]
    QsoLogged,
    /// A radio's frequency, mode or transmit state changed.
    Radio,
    /// The rotator was asked to turn (map, rotator pane).
    Rotator,
    /// A callsign was entered and looked up.
    Lookup,
    /// Every packet WSJT-X / JTDX send to QRZero, passed on unchanged.
    RelayWsjtx,
    /// Every packet N1MM Logger+ sends to QRZero, passed on unchanged.
    RelayN1mm,
}

impl Event {
    pub fn is_relay(self) -> bool {
        matches!(self, Event::RelayWsjtx | Event::RelayN1mm)
    }

    fn name(self) -> &'static str {
        match self {
            Event::QsoLogged => "qso_logged",
            Event::Radio => "radio",
            Event::Rotator => "rotator",
            Event::Lookup => "lookup",
            Event::RelayWsjtx => "relay_wsjtx",
            Event::RelayN1mm => "relay_n1mm",
        }
    }
}

/// The message a connection sends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// N1MM Logger+ `<RadioInfo>` XML (antenna switches, band decoders).
    N1mmRadio,
    /// N1MM Logger+ `<contactinfo>` XML.
    N1mmContact,
    /// One ADIF record.
    Adif,
    /// A JSON object.
    #[default]
    Json,
    /// PstRotatorAz `<PST><AZIMUTH>n</AZIMUTH></PST>`.
    Pst,
    /// The connection's own text, with {PLACEHOLDERS}.
    Template,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UdpConnection {
    /// Stable id (for status); 0 gets one when saved.
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub event: Event,
    pub format: Format,
    /// The text for [`Format::Template`].
    pub template: String,
    /// Radio events only: the radio's name as the QSO panel shows it; empty for every radio.
    pub radio: String,
}

impl Default for UdpConnection {
    fn default() -> Self {
        UdpConnection {
            id: 0,
            name: String::new(),
            enabled: true,
            host: "127.0.0.1".into(),
            port: 12060,
            event: Event::QsoLogged,
            format: Format::Json,
            template: String::new(),
            radio: String::new(),
        }
    }
}

/// What happened, with everything a message might carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ctx {
    pub event: Event,
    pub call: String,
    pub freq_hz: u64,
    pub tx_freq_hz: u64,
    pub band: String,
    pub mode: String,
    /// The rig's own mode name (USB, CW-R, DIGU, ...), when it's a radio.
    pub rig_mode: String,
    pub tx: bool,
    pub az: Option<f64>,
    pub grid: String,
    /// Radio name, and its number (1 for the first channel of a rig).
    pub radio: String,
    pub radio_key: String,
    pub radio_nr: u32,
    /// The QSO (or the looked-up station) as ADIF fields.
    pub fields: Fields,
    /// The QSO's id in the log.
    pub qso_id: Option<i64>,
}

impl Ctx {
    /// A logged QSO.
    pub fn qso(fields: &Fields, qso_id: Option<i64>) -> Ctx {
        let get = |k: &str| fields.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
        let hz = |k: &str| fields.get(k).and_then(|v| v.trim().parse::<f64>().ok()).map_or(0, |mhz| (mhz * 1e6).round() as u64);
        let mode = Some(get("SUBMODE")).filter(|m| !m.is_empty()).unwrap_or_else(|| get("MODE"));
        Ctx {
            event: Event::QsoLogged,
            call: get("CALL"),
            freq_hz: Some(hz("FREQ_RX")).filter(|f| *f > 0).unwrap_or_else(|| hz("FREQ")),
            tx_freq_hz: hz("FREQ"),
            band: get("BAND"),
            mode,
            grid: get("GRIDSQUARE"),
            radio: get("MY_RIG"),
            radio_nr: 1,
            fields: fields.clone(),
            qso_id,
            ..Ctx::default()
        }
    }

    fn sig(&self) -> String {
        format!("{}|{}|{}|{}|{}", self.freq_hz, self.tx_freq_hz, self.mode, self.rig_mode, self.tx)
    }

    /// The value of a {PLACEHOLDER}.
    fn value(&self, key: &str) -> String {
        let now = chrono::Utc::now();
        match key {
            "CALL" => self.call.clone(),
            "FREQ_HZ" => self.freq_hz.to_string(),
            "FREQ_KHZ" => format!("{}.{:03}", self.freq_hz / 1000, self.freq_hz % 1000),
            "FREQ_MHZ" => format!("{}.{:06}", self.freq_hz / 1_000_000, self.freq_hz % 1_000_000),
            "TX_FREQ_HZ" => self.tx_freq_hz.to_string(),
            "BAND" => self.band.clone(),
            "MODE" => self.mode.clone(),
            "RIG_MODE" => self.rig_mode.clone(),
            "TX" => (if self.tx { "1" } else { "0" }).into(),
            "AZ" => self.az.map(|a| deg(a).to_string()).unwrap_or_default(),
            "GRID" => self.grid.clone(),
            "RADIO" => self.radio.clone(),
            "RADIO_NR" => self.radio_nr.to_string(),
            "EVENT" => self.event.name().into(),
            "DATE" => now.format("%Y%m%d").to_string(),
            "TIME" => now.format("%H%M%S").to_string(),
            other => self.fields.get(other).cloned().unwrap_or_default(),
        }
    }
}

/// Degrees, rounded and folded into 0..359.
fn deg(az: f64) -> u32 {
    az.round().rem_euclid(360.0) as u32
}

fn xml(s: &str) -> std::borrow::Cow<'_, str> {
    quick_xml::escape::escape(s)
}

/// Fills in a template: {CALL}, {FREQ_HZ}, ... and any ADIF field of the QSO ({RST_SENT}).
/// Braces around anything that isn't a placeholder name stay as they are, so JSON-like text
/// works; `\r`, `\n`, `\t` and `\\` become the characters.
pub fn fill_template(template: &str, ctx: &Ctx) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            if let Some(end) = rest[1..].find('}') {
                let key = &rest[1..1 + end];
                if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    out.push_str(&ctx.value(&key.to_ascii_uppercase()));
                    rest = &rest[end + 2..];
                    continue;
                }
            }
        } else if c == '\\' {
            let esc = match rest[1..].chars().next() {
                Some('r') => Some('\r'),
                Some('n') => Some('\n'),
                Some('t') => Some('\t'),
                Some('\\') => Some('\\'),
                _ => None,
            };
            if let Some(e) = esc {
                out.push(e);
                rest = &rest[2..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// The message a connection sends for an event.
pub fn render(conn: &UdpConnection, ctx: &Ctx) -> Result<Vec<u8>, String> {
    Ok(match conn.format {
        Format::N1mmRadio => n1mm_radio_info(ctx).into_bytes(),
        Format::N1mmContact => n1mm_contact_info(ctx).into_bytes(),
        Format::Adif => {
            let mut out = String::new();
            adif::write_record(&mut out, &adif_fields(ctx), |_| true);
            out.into_bytes()
        }
        Format::Json => json_message(ctx).to_string().into_bytes(),
        Format::Pst => match ctx.az {
            Some(az) => qrzero_radio::pst::set_azimuth(az).into_bytes(),
            None => return Err("this event has no heading to send to PstRotatorAz".into()),
        },
        Format::Template => {
            if conn.template.is_empty() {
                return Err("the message template is empty".into());
            }
            fill_template(&conn.template, ctx).into_bytes()
        }
    })
}

/// The QSO's fields, or the few that describe a radio or lookup event.
fn adif_fields(ctx: &Ctx) -> Fields {
    let mut f = ctx.fields.clone();
    let mut put = |k: &str, v: String| {
        if !v.is_empty() && v != "0" {
            f.entry(k.to_string()).or_insert(v);
        }
    };
    put("CALL", ctx.call.clone());
    if ctx.freq_hz > 0 {
        put("FREQ", format!("{:.6}", ctx.freq_hz as f64 / 1e6));
    }
    put("BAND", ctx.band.clone());
    put("MODE", ctx.mode.clone());
    put("GRIDSQUARE", ctx.grid.clone());
    if let Some(az) = ctx.az {
        put("ANT_AZ", deg(az).to_string());
    }
    f
}

fn json_message(ctx: &Ctx) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("app".into(), json!("QRZero"));
    m.insert("event".into(), json!(ctx.event.name()));
    let mut text = |k: &str, v: &str| {
        if !v.is_empty() {
            m.insert(k.into(), json!(v));
        }
    };
    text("call", &ctx.call);
    text("band", &ctx.band);
    text("mode", &ctx.mode);
    text("rig_mode", &ctx.rig_mode);
    text("grid", &ctx.grid);
    text("radio", &ctx.radio);
    if ctx.freq_hz > 0 {
        m.insert("freq_hz".into(), json!(ctx.freq_hz));
    }
    if ctx.event == Event::Radio {
        m.insert("radio_nr".into(), json!(ctx.radio_nr));
        m.insert("tx_freq_hz".into(), json!(ctx.tx_freq_hz));
        m.insert("tx".into(), json!(ctx.tx));
    }
    if let Some(az) = ctx.az {
        m.insert("azimuth".into(), json!(deg(az)));
    }
    if let Some(id) = ctx.qso_id {
        m.insert("qso_id".into(), json!(id));
    }
    if !ctx.fields.is_empty() {
        m.insert("fields".into(), json!(ctx.fields));
    }
    serde_json::Value::Object(m)
}

/// N1MM Logger+ RadioInfo, as N1MM broadcasts it: frequencies in tens of Hz.
pub fn n1mm_radio_info(ctx: &Ctx) -> String {
    let mode = if ctx.rig_mode.is_empty() { &ctx.mode } else { &ctx.rig_mode };
    let tx = if ctx.tx_freq_hz > 0 { ctx.tx_freq_hz } else { ctx.freq_hz };
    let b = |v: bool| if v { "True" } else { "False" };
    let nr = ctx.radio_nr.max(1);
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<RadioInfo>\r\n\
         \t<app>QRZero</app>\r\n\
         \t<StationName>{station}</StationName>\r\n\
         \t<RadioNr>{nr}</RadioNr>\r\n\
         \t<Freq>{rx}</Freq>\r\n\
         \t<TXFreq>{tx}</TXFreq>\r\n\
         \t<Mode>{mode}</Mode>\r\n\
         \t<OpCall>{op}</OpCall>\r\n\
         \t<IsRunning>False</IsRunning>\r\n\
         \t<FocusEntry>0</FocusEntry>\r\n\
         \t<EntryWindowHwnd>0</EntryWindowHwnd>\r\n\
         \t<Antenna>-1</Antenna>\r\n\
         \t<Rotors></Rotors>\r\n\
         \t<FocusRadioNr>{nr}</FocusRadioNr>\r\n\
         \t<IsStereo>False</IsStereo>\r\n\
         \t<IsSplit>{split}</IsSplit>\r\n\
         \t<ActiveRadioNr>{nr}</ActiveRadioNr>\r\n\
         \t<IsTransmitting>{txing}</IsTransmitting>\r\n\
         \t<FunctionKeyCaption></FunctionKeyCaption>\r\n\
         \t<RadioName>{radio}</RadioName>\r\n\
         \t<AuxAntSelected>-1</AuxAntSelected>\r\n\
         \t<AuxAntSelectedName></AuxAntSelectedName>\r\n\
         \t<IsConnected>True</IsConnected>\r\n\
         </RadioInfo>",
        station = xml(&station_name()),
        rx = ctx.freq_hz / 10,
        tx = tx / 10,
        mode = xml(mode),
        op = xml(ctx.fields.get("OPERATOR").map_or("", |s| s.as_str())),
        split = b(tx != ctx.freq_hz),
        txing = b(ctx.tx),
        radio = xml(&ctx.radio),
    )
}

/// N1MM Logger+ contactinfo, the message N1MM broadcasts when a QSO is logged.
pub fn n1mm_contact_info(ctx: &Ctx) -> String {
    let f = &ctx.fields;
    let get = |k: &str| f.get(k).map_or("", |s| s.trim());
    let rx = if ctx.freq_hz > 0 { ctx.freq_hz } else { ctx.tx_freq_hz };
    let tx = if ctx.tx_freq_hz > 0 { ctx.tx_freq_hz } else { rx };
    let timestamp = {
        let (d, t) = (get("QSO_DATE"), get("TIME_ON"));
        if d.len() == 8 && t.len() >= 4 {
            let t = format!("{t:0<6}");
            format!("{}-{}-{} {}:{}:{}", &d[..4], &d[4..6], &d[6..8], &t[..2], &t[2..4], &t[4..6])
        } else {
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
        }
    };
    let band = qrzero_radio::n1mm::band_label(&ctx.band)
        .or_else(|| qrzero_core::band::band_for_freq(rx as f64 / 1e6).and_then(qrzero_radio::n1mm::band_label))
        .unwrap_or_default();
    // N1MM names the sideband for phone; digital modes go by their own name.
    let mode = match (get("MODE"), get("SUBMODE")) {
        ("SSB", "") => if rx < 10_000_000 { "LSB" } else { "USB" }.to_string(),
        (_, sub) if !sub.is_empty() => sub.to_string(),
        (m, _) if !m.is_empty() => m.to_string(),
        _ => ctx.mode.clone(),
    };
    let id = match ctx.qso_id {
        Some(id) => format!("qrzero{id:026}"),
        None => format!("qrzero{:026}", chrono::Utc::now().timestamp_millis()),
    };
    let el = |name: &str, v: &str| format!("\t<{name}>{}</{name}>\r\n", xml(v));
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<contactinfo>\r\n");
    for (name, v) in [
        ("app", "QRZero"),
        ("contestname", if get("CONTEST_ID").is_empty() { "DX" } else { get("CONTEST_ID") }),
        ("contestnr", "1"),
        ("timestamp", &timestamp),
        ("mycall", get("STATION_CALLSIGN")),
        ("band", &band),
        ("rxfreq", &(rx / 10).to_string()),
        ("txfreq", &(tx / 10).to_string()),
        ("operator", if get("OPERATOR").is_empty() { get("STATION_CALLSIGN") } else { get("OPERATOR") }),
        ("mode", &mode),
        ("call", if ctx.call.is_empty() { get("CALL") } else { &ctx.call }),
        ("countryprefix", get("PFX")),
        ("wpxprefix", get("PFX")),
        ("stationprefix", get("STATION_CALLSIGN")),
        ("continent", get("CONT")),
        ("snt", get("RST_SENT")),
        ("sntnr", get("STX")),
        ("rcv", get("RST_RCVD")),
        ("rcvnr", get("SRX")),
        ("gridsquare", get("GRIDSQUARE")),
        ("exchange1", get("SRX_STRING")),
        ("section", get("ARRL_SECT")),
        ("comment", get("COMMENT")),
        ("qth", get("QTH")),
        ("name", get("NAME")),
        ("power", get("TX_PWR")),
        ("misctext", ""),
        ("zone", get("CQZ")),
        ("prec", ""),
        ("ck", ""),
        ("ismultiplier1", "0"),
        ("ismultiplier2", "0"),
        ("ismultiplier3", "0"),
        ("points", "1"),
        ("radionr", &ctx.radio_nr.max(1).to_string()),
        ("run1run2", "1"),
        ("RoverLocation", ""),
        ("RadioInterfaced", "1"),
        ("NetworkedCompNr", "0"),
        ("IsOriginal", "True"),
        ("NetBiosName", ""),
        ("IsRunQSO", "0"),
        ("StationName", &station_name()),
        ("ID", &id),
        ("IsClaimedQso", "1"),
    ] {
        out.push_str(&el(name, v));
    }
    out.push_str("</contactinfo>");
    out
}

fn station_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default()
}

/// The centre of a Maidenhead square (4 or 6 characters), as (lat, lon).
pub fn grid_latlon(grid: &str) -> Option<(f64, f64)> {
    let g: Vec<u8> = grid.trim().to_ascii_uppercase().bytes().collect();
    if g.len() < 4 || !(b'A'..=b'R').contains(&g[0]) || !(b'A'..=b'R').contains(&g[1]) || !g[2].is_ascii_digit() || !g[3].is_ascii_digit() {
        return None;
    }
    let mut lon = (g[0] - b'A') as f64 * 20.0 - 180.0 + (g[2] - b'0') as f64 * 2.0;
    let mut lat = (g[1] - b'A') as f64 * 10.0 - 90.0 + (g[3] - b'0') as f64;
    if g.len() >= 6 && (b'A'..=b'X').contains(&g[4]) && (b'A'..=b'X').contains(&g[5]) {
        lon += (g[4] - b'A') as f64 / 12.0 + 1.0 / 24.0;
        lat += (g[5] - b'A') as f64 / 24.0 + 1.0 / 48.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Some((lat, lon))
}

/// Short-path heading in degrees from one point to another.
pub fn bearing(from: (f64, f64), to: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (from.0.to_radians(), from.1.to_radians(), to.0.to_radians(), to.1.to_radians());
    let dl = lo2 - lo1;
    let y = dl.sin() * la2.cos();
    let x = la1.cos() * la2.sin() - la1.sin() * la2.cos() * dl.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// Sends one datagram. Broadcast addresses work too.
pub async fn send(host: &str, port: u16, payload: &[u8]) -> Result<SocketAddr, String> {
    let host = host.trim();
    if host.is_empty() || port == 0 {
        return Err("needs a host and a port".into());
    }
    let addr = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("can't find {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("can't find {host}"))?;
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).await.map_err(|e| e.to_string())?;
    let _ = sock.set_broadcast(true);
    sock.send_to(payload, addr).await.map_err(|e| format!("couldn't send to {addr}: {e}"))?;
    Ok(addr)
}

#[derive(Clone, Debug, Serialize)]
pub struct ConnStatus {
    pub ok: bool,
    pub text: String,
}

#[derive(Default)]
struct RadioSent {
    sig: String,
    at: Option<Instant>,
    pending: bool,
    latest: Option<Ctx>,
}

/// The saved connections and what they last did.
#[derive(Default)]
pub struct Outputs {
    conns: RwLock<Arc<Vec<UdpConnection>>>,
    /// Resolved relay targets: (connection id, which stream, where).
    relay: RwLock<Arc<Vec<(u64, Event, SocketAddr)>>>,
    relay_counts: Mutex<HashMap<u64, u64>>,
    status: Mutex<BTreeMap<u64, ConnStatus>>,
    radio_sent: Mutex<HashMap<(u64, String), RadioSent>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Outputs {
    pub fn new(conns: Vec<UdpConnection>) -> Arc<Self> {
        let o = Arc::new(Outputs::default());
        o.set(conns);
        o
    }

    pub fn connections(&self) -> Vec<UdpConnection> {
        self.conns.read().unwrap_or_else(|p| p.into_inner()).as_ref().clone()
    }

    /// Replaces the connections (giving new ones ids) and returns them as saved.
    pub fn set(self: &Arc<Self>, mut conns: Vec<UdpConnection>) -> Vec<UdpConnection> {
        let mut next = conns.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let mut seen = std::collections::HashSet::new();
        for c in &mut conns {
            c.host = c.host.trim().to_string();
            c.name = c.name.trim().to_string();
            if c.id == 0 || !seen.insert(c.id) {
                c.id = next;
                seen.insert(next);
                next += 1;
            }
        }
        *self.conns.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(conns.clone());
        lock(&self.status).retain(|id, _| conns.iter().any(|c| c.id == *id));
        lock(&self.radio_sent).clear();
        lock(&self.relay_counts).clear();
        *self.relay.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(Vec::new());
        let relays: Vec<UdpConnection> = conns.iter().filter(|c| c.enabled && c.event.is_relay()).cloned().collect();
        if !relays.is_empty() {
            let me = self.clone();
            let resolve = async move {
                let mut targets = Vec::new();
                for c in relays {
                    match tokio::net::lookup_host((c.host.as_str(), c.port)).await.map(|mut a| a.next()) {
                        Ok(Some(addr)) if c.port != 0 => {
                            targets.push((c.id, c.event, addr));
                            me.set_status(c.id, true, format!("relaying to {addr}"));
                        }
                        Ok(_) | Err(_) => me.set_status(c.id, false, format!("can't find {}:{}", c.host, c.port)),
                    }
                }
                *me.relay.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(targets);
            };
            match tokio::runtime::Handle::try_current() {
                Ok(h) => {
                    h.spawn(resolve);
                }
                Err(_) => tracing::warn!("UDP relay set up outside the runtime"),
            }
        }
        conns
    }

    fn set_status(&self, id: u64, ok: bool, text: String) {
        lock(&self.status).insert(id, ConnStatus { ok, text });
    }

    pub fn status(&self) -> BTreeMap<u64, ConnStatus> {
        let mut out = lock(&self.status).clone();
        for (id, n) in lock(&self.relay_counts).iter() {
            if let Some(s) = out.get_mut(id) {
                s.text = format!("{}, {n} packet{} passed on", s.text, if *n == 1 { "" } else { "s" });
            }
        }
        out
    }

    /// Whether any enabled connection sends on this event (cheap; check before building a [`Ctx`]).
    pub fn wants(&self, event: Event) -> bool {
        self.conns.read().unwrap_or_else(|p| p.into_inner()).iter().any(|c| c.enabled && c.event == event)
    }

    /// Where to relay a raw packet from WSJT-X ([`Event::RelayWsjtx`]) or N1MM ([`Event::RelayN1mm`]).
    pub fn relay_targets(&self, stream: Event) -> Vec<SocketAddr> {
        let all = self.relay.read().unwrap_or_else(|p| p.into_inner()).clone();
        all.iter().filter(|(_, e, _)| *e == stream).map(|(_, _, a)| *a).collect()
    }

    /// Counts a relayed packet (for the status in Settings).
    pub fn relayed(&self, stream: Event, to: SocketAddr) {
        let all = self.relay.read().unwrap_or_else(|p| p.into_inner()).clone();
        let mut counts = lock(&self.relay_counts);
        for (id, _, _) in all.iter().filter(|(_, e, a)| *e == stream && *a == to) {
            *counts.entry(*id).or_default() += 1;
        }
    }

    /// Sends to every enabled connection for this event. Radio events go out only when the
    /// radio's frequency, mode or TX changed, and at most every [`RADIO_GAP`].
    pub fn fire(self: &Arc<Self>, ctx: Ctx) {
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let conns = self.conns.read().unwrap_or_else(|p| p.into_inner()).clone();
        for conn in conns.iter().filter(|c| c.enabled && c.event == ctx.event) {
            if ctx.event == Event::Radio {
                let filter = conn.radio.trim();
                if !filter.is_empty() && !filter.eq_ignore_ascii_case(&ctx.radio) && filter != ctx.radio_key {
                    continue;
                }
                let key = (conn.id, ctx.radio_key.clone());
                let mut sent = lock(&self.radio_sent);
                let entry = sent.entry(key.clone()).or_default();
                let sig = ctx.sig();
                if entry.pending {
                    entry.latest = Some(ctx.clone());
                    continue;
                }
                if entry.sig == sig {
                    continue;
                }
                let wait = entry.at.map(|t| RADIO_GAP.saturating_sub(t.elapsed())).unwrap_or_default();
                if wait.is_zero() {
                    entry.sig = sig;
                    entry.at = Some(Instant::now());
                } else {
                    entry.pending = true;
                    entry.latest = Some(ctx.clone());
                    let (me, conn) = (self.clone(), conn.clone());
                    rt.spawn(async move {
                        tokio::time::sleep(wait).await;
                        let latest = {
                            let mut sent = lock(&me.radio_sent);
                            let Some(entry) = sent.get_mut(&key) else { return };
                            entry.pending = false;
                            match entry.latest.take() {
                                Some(c) if c.sig() != entry.sig => {
                                    entry.sig = c.sig();
                                    entry.at = Some(Instant::now());
                                    c
                                }
                                _ => return,
                            }
                        };
                        me.deliver(&conn, &latest).await;
                    });
                    continue;
                }
            }
            let (me, conn, ctx) = (self.clone(), conn.clone(), ctx.clone());
            rt.spawn(async move { me.deliver(&conn, &ctx).await });
        }
    }

    async fn deliver(&self, conn: &UdpConnection, ctx: &Ctx) {
        let result = match render(conn, ctx) {
            Ok(payload) => send(&conn.host, conn.port, &payload).await,
            Err(e) => Err(e),
        };
        let now = chrono::Utc::now().format("%H:%M:%S");
        match result {
            Ok(addr) => self.set_status(conn.id, true, format!("last sent {now} UTC to {addr}")),
            Err(e) => {
                tracing::debug!("UDP connection {}: {e}", conn.name);
                self.set_status(conn.id, false, format!("{now} UTC: {e}"));
            }
        }
    }

    /// Sends a made-up example of the connection's event, and returns what was sent.
    pub async fn test(&self, conn: &UdpConnection) -> Result<String, String> {
        let payload = match conn.event {
            Event::RelayWsjtx => qrzero_radio::wsjtx::encode_heartbeat("QRZero relay test", crate::VERSION),
            Event::RelayN1mm => n1mm_radio_info(&sample(Event::Radio)).into_bytes(),
            e => render(conn, &sample(e))?,
        };
        let result = send(&conn.host, conn.port, &payload).await;
        if conn.id != 0 {
            match &result {
                Ok(addr) => self.set_status(conn.id, true, format!("test sent to {addr}")),
                Err(e) => self.set_status(conn.id, false, e.clone()),
            }
        }
        result?;
        Ok(String::from_utf8_lossy(&payload).into_owned())
    }
}

/// An example of an event, for "Send test".
pub fn sample(event: Event) -> Ctx {
    let mut fields = Fields::new();
    for (k, v) in [
        ("CALL", "W1AW"),
        ("QSO_DATE", "20240101"),
        ("TIME_ON", "120000"),
        ("BAND", "20m"),
        ("FREQ", "14.074"),
        ("MODE", "FT8"),
        ("RST_SENT", "-10"),
        ("RST_RCVD", "-12"),
        ("GRIDSQUARE", "FN31"),
        ("NAME", "Test"),
        ("STATION_CALLSIGN", "N0CALL"),
    ] {
        fields.insert(k.into(), v.into());
    }
    match event {
        Event::Radio => Ctx {
            event,
            freq_hz: 14_074_000,
            tx_freq_hz: 14_074_000,
            band: "20m".into(),
            mode: "SSB".into(),
            rig_mode: "USB".into(),
            radio: "Test radio".into(),
            radio_key: "test".into(),
            radio_nr: 1,
            ..Ctx::default()
        },
        Event::Rotator => Ctx { event, az: Some(45.0), ..Ctx::default() },
        Event::Lookup => Ctx { event, call: "W1AW".into(), grid: "FN31".into(), az: Some(45.0), fields, ..Ctx::default() },
        _ => Ctx::qso(&fields, Some(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qso() -> Ctx {
        let f: Fields = [
            ("CALL", "DL1ABC"),
            ("QSO_DATE", "20240315"),
            ("TIME_ON", "1405"),
            ("BAND", "20m"),
            ("FREQ", "14.0255"),
            ("MODE", "CW"),
            ("RST_SENT", "599"),
            ("RST_RCVD", "579"),
            ("STATION_CALLSIGN", "N0CALL"),
            ("COMMENT", "Tom & Jerry"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        Ctx::qso(&f, Some(42))
    }

    fn conn(format: Format) -> UdpConnection {
        UdpConnection { format, ..UdpConnection::default() }
    }

    #[test]
    fn templates() {
        let mut ctx = sample(Event::Radio);
        ctx.az = Some(359.7);
        let t = |s: &str| fill_template(s, &ctx);
        assert_eq!(t("{FREQ_HZ} {FREQ_KHZ} {FREQ_MHZ} {BAND} {MODE} {RIG_MODE} {RADIO} {AZ}"), "14074000 14074.000 14.074000 20m SSB USB Test radio 0");
        assert_eq!(t("F{freq_hz};\\r\\n"), "F14074000;\r\n");
        // Braces that aren't placeholders are left alone, so JSON-like templates work.
        assert_eq!(t("{\"f\": {FREQ_HZ}, \"x\": {}}"), "{\"f\": 14074000, \"x\": {}}");
        assert_eq!(t("{NOPE}|{"), "|{");
        // ADIF fields of a QSO by name.
        assert_eq!(fill_template("{CALL} {RST_SENT} {TIME_ON} {FREQ_HZ}", &qso()), "DL1ABC 599 1405 14025500");
    }

    #[test]
    fn pst_and_json() {
        let ctx = sample(Event::Rotator);
        assert_eq!(render(&conn(Format::Pst), &ctx).unwrap(), b"<PST><AZIMUTH>45</AZIMUTH></PST>");
        assert!(render(&conn(Format::Pst), &qso()).is_err(), "a QSO has no heading");
        let j: serde_json::Value = serde_json::from_slice(&render(&conn(Format::Json), &ctx).unwrap()).unwrap();
        assert_eq!(j, json!({"app": "QRZero", "event": "rotator", "azimuth": 45}));
        let j: serde_json::Value = serde_json::from_slice(&render(&conn(Format::Json), &sample(Event::Radio)).unwrap()).unwrap();
        assert_eq!(j["freq_hz"], 14_074_000);
        assert_eq!(j["radio"], "Test radio");
        assert_eq!(j["tx"], false);
        let j: serde_json::Value = serde_json::from_slice(&render(&conn(Format::Json), &qso()).unwrap()).unwrap();
        assert_eq!((j["event"].as_str(), j["call"].as_str(), j["qso_id"].as_i64()), (Some("qso_logged"), Some("DL1ABC"), Some(42)));
        assert_eq!(j["fields"]["RST_RCVD"], "579");
    }

    #[test]
    fn adif_record() {
        let text = String::from_utf8(render(&conn(Format::Adif), &qso()).unwrap()).unwrap();
        assert!(text.contains("<CALL:6>DL1ABC"), "{text}");
        assert!(text.trim_end().ends_with("<EOR>"));
        let parsed = adif::parse(text.as_bytes()).records;
        assert_eq!(parsed[0].get("FREQ").map(String::as_str), Some("14.0255"));
        // A radio event is described by its frequency and mode.
        let text = String::from_utf8(render(&conn(Format::Adif), &sample(Event::Radio)).unwrap()).unwrap();
        assert!(text.contains("<FREQ:9>14.074000") && text.contains("<BAND:3>20m"), "{text}");
    }

    #[test]
    fn n1mm_messages_read_back() {
        use qrzero_radio::n1mm::{parse, N1mmMessage};
        let mut ctx = sample(Event::Radio);
        ctx.radio_nr = 2;
        ctx.tx = true;
        ctx.freq_hz = 7_025_120;
        ctx.tx_freq_hz = 7_025_120;
        let xml = n1mm_radio_info(&ctx);
        match parse(xml.as_bytes()).unwrap() {
            N1mmMessage::RadioInfo { radio_nr, freq_hz, tx_freq_hz, mode, is_transmitting, .. } => {
                assert_eq!((radio_nr, freq_hz, tx_freq_hz, mode.as_str(), is_transmitting), (2, 7_025_120, 7_025_120, "USB", true));
            }
            other => panic!("{other:?}"),
        }
        assert!(xml.contains("<Freq>702512</Freq>"));

        let xml = n1mm_contact_info(&qso());
        assert!(xml.contains("<band>14</band>") && xml.contains("<rxfreq>1402550</rxfreq>"), "{xml}");
        assert!(xml.contains("Tom &amp; Jerry"));
        match parse(xml.as_bytes()).unwrap() {
            N1mmMessage::Contact { replace, fields, .. } => {
                assert!(!replace);
                for (k, v) in [("CALL", "DL1ABC"), ("BAND", "20m"), ("FREQ", "14.0255"), ("MODE", "CW"), ("QSO_DATE", "20240315"), ("TIME_ON", "140500"), ("RST_SENT", "599"), ("COMMENT", "Tom & Jerry")] {
                    assert_eq!(fields.get(k).map(String::as_str), Some(v), "{k}");
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn headings_from_grids() {
        let (lat, lon) = grid_latlon("FN31pr").unwrap();
        assert!((lat - 41.729).abs() < 0.01 && (lon - -72.708).abs() < 0.01, "{lat} {lon}");
        assert_eq!(grid_latlon("JO62"), Some((52.5, 13.0)));
        assert_eq!(grid_latlon("ZZ00"), None);
        // New England to Berlin is about 47 degrees; to Sydney about 260 (long way round the west).
        let b = bearing(grid_latlon("FN31").unwrap(), grid_latlon("JO62").unwrap());
        assert!((b - 47.0).abs() < 3.0, "{b}");
        let b = bearing(grid_latlon("FN31").unwrap(), grid_latlon("QF56").unwrap());
        assert!((b - 260.0).abs() < 10.0, "{b}");
    }

    #[test]
    fn saved_connections_get_ids() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let o = Outputs::new(Vec::new());
            let saved = o.set(vec![
                UdpConnection { id: 5, ..UdpConnection::default() },
                UdpConnection::default(),
                UdpConnection { id: 5, ..UdpConnection::default() },
            ]);
            let ids: Vec<u64> = saved.iter().map(|c| c.id).collect();
            assert_eq!(ids, vec![5, 6, 7]);
            assert!(o.wants(Event::QsoLogged) && !o.wants(Event::Radio));
        });
    }

    #[tokio::test]
    async fn radio_changes_are_deduplicated_and_rate_limited() {
        let rx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = rx.local_addr().unwrap().port();
        let o = Outputs::new(vec![UdpConnection {
            id: 1,
            event: Event::Radio,
            format: Format::Template,
            template: "{FREQ_HZ}".into(),
            host: "127.0.0.1".into(),
            port,
            ..UdpConnection::default()
        }]);
        let at = |hz: u64| Ctx { freq_hz: hz, ..sample(Event::Radio) };
        o.fire(at(14_000_000));
        o.fire(at(14_000_000)); // no change: nothing
        o.fire(at(14_001_000)); // too soon: held back...
        o.fire(at(14_002_000)); // ...and replaced by the latest
        let mut buf = [0u8; 64];
        let mut got = Vec::new();
        for _ in 0..2 {
            let (n, _) = tokio::time::timeout(Duration::from_secs(2), rx.recv_from(&mut buf)).await.unwrap().unwrap();
            got.push(String::from_utf8_lossy(&buf[..n]).into_owned());
        }
        assert_eq!(got, ["14000000", "14002000"]);
        assert!(tokio::time::timeout(Duration::from_millis(400), rx.recv_from(&mut buf)).await.is_err(), "nothing more");
        // Another radio isn't held back by the first, unless the connection is for one radio only.
        o.fire(Ctx { radio: "Other".into(), radio_key: "other".into(), ..at(7_000_000) });
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), rx.recv_from(&mut buf)).await.unwrap().unwrap();
        assert_eq!(&buf[..n], b"7000000");
        assert!(o.status()[&1].ok);
    }
}
