//! The live station: rig connections, WSJT-X/JTDX, N1MM and PstRotatorAz over
//! UDP, the country file, and the event stream the UI listens to.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use qrzero_core::adif::{self, Fields};
use qrzero_core::band::band_for_freq;
use qrzero_core::cty::{CtyDb, Entity};
use qrzero_core::awards::{AwardHint, AwardIndex, AwardQso, Counts};
use qrzero_core::worked::{Needed, WorkedIndex};
use qrzero_core::Store;
use qrzero_radio::rig::{self, RigCommand, RigConfig, RigHandle, RigState};
use qrzero_radio::{n1mm, pst, wsjtx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::udp_out::{Ctx as UdpCtx, Event as UdpEvent};

const MAX_DECODES: usize = 1000;
/// Where the country file comes from. cty.csv carries ADIF DXCC numbers; cty.dat is the fallback.
pub const CTY_URLS: [&str; 2] = ["https://www.country-files.com/cty/cty.csv", "https://www.country-files.com/cty/cty.dat"];
const CTY_MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);

/// UDP integration settings, stored as the `integrations` setting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Integrations {
    pub wsjtx_enabled: bool,
    /// Addresses WSJT-X/JTDX send to ("UDP Server" in their Reporting settings),
    /// comma-separated: one per port when several instances each use their own.
    pub wsjtx_listen: String,
    /// Optional multicast group, e.g. 224.0.0.1, when several programs share the stream.
    pub wsjtx_multicast: String,
    /// Other programs (GridTracker, JTAlert, ...) to pass the stream on to.
    pub wsjtx_forward: Vec<String>,
    pub wsjtx_auto_log: bool,
    pub n1mm_enabled: bool,
    pub n1mm_listen: String,
    pub n1mm_auto_log: bool,
    /// Look up QSOs logged by WSJT-X, JTDX or N1MM on QRZ and fill in what they left blank.
    pub auto_log_lookup: bool,
    pub rotator_enabled: bool,
    pub rotator_addr: String,
}

impl Integrations {
    /// The WSJT-X listen addresses, without duplicates.
    pub fn wsjtx_addrs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in self.wsjtx_listen.split([',', ';', ' ']).map(str::trim).filter(|a| !a.is_empty()) {
            if !out.iter().any(|x| x == a) {
                out.push(a.to_string());
            }
        }
        out
    }
}

impl Default for Integrations {
    fn default() -> Self {
        Integrations {
            wsjtx_enabled: true,
            wsjtx_listen: "127.0.0.1:2237".into(),
            wsjtx_multicast: String::new(),
            wsjtx_forward: Vec::new(),
            wsjtx_auto_log: true,
            n1mm_enabled: false,
            n1mm_listen: "127.0.0.1:12060".into(),
            n1mm_auto_log: true,
            auto_log_lookup: true,
            rotator_enabled: false,
            rotator_addr: "127.0.0.1:12000".into(),
        }
    }
}

/// Where logged QSOs go and which equipment is live: what the UI has selected.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Active {
    pub log_id: Option<i64>,
    pub location_id: Option<i64>,
    #[serde(default)]
    pub station_callsign: String,
}

/// A frequency/mode source the entry panel can follow.
#[derive(Clone, Debug, Serialize)]
pub struct Radio {
    pub key: String,
    pub name: String,
    pub source: &'static str,
    pub can_tune: bool,
    #[serde(flatten)]
    pub state: RigState,
}

#[derive(Clone, Debug, Serialize)]
pub struct FtxDecode {
    pub seq: u64,
    pub instance: String,
    /// UTC, HHMMSS.
    pub time: String,
    pub snr: i32,
    pub dt: f64,
    pub df: u32,
    pub mode: String,
    pub message: String,
    pub call: Option<String>,
    pub to: Option<String>,
    pub grid: Option<String>,
    pub cq: bool,
    pub cq_target: Option<String>,
    pub to_me: bool,
    pub band: Option<String>,
    pub freq_hz: u64,
    pub entity: Option<Entity>,
    pub needed: Option<Needed>,
    pub low_confidence: bool,
    /// The instance's short source label (see [`FtxInstance::source`]), copied so the UI needn't join.
    pub source: String,
    pub slice: Option<String>,
    pub color_index: u8,
    /// The watch list entry this station matches.
    pub watched: Option<u64>,
    /// Not a decode but what this instance transmitted in that period (`message`, at `df`).
    pub tx: bool,
    #[serde(skip)]
    raw: wsjtx::Decode,
}

#[derive(Clone, Debug, Serialize)]
pub struct FtxInstance {
    pub id: String,
    pub dial_freq: u64,
    pub band: Option<String>,
    pub mode: String,
    pub de_call: String,
    pub de_grid: String,
    pub dx_call: String,
    pub dx_grid: String,
    /// The report WSJT-X will send.
    pub report: String,
    pub transmitting: bool,
    pub tx_enabled: bool,
    pub decoding: bool,
    /// The message being sent, or to be sent next (WSJT-X 2.1 and later; empty from older ones).
    pub tx_message: String,
    /// Rx and Tx audio offsets, Hz.
    pub rx_df: u32,
    pub tx_df: u32,
    /// The Tx watchdog has stopped transmitting.
    pub tx_watchdog: bool,
    /// Seconds; 0 when the program doesn't say.
    pub tr_period: u32,
    pub sub_mode: String,
    pub fast_mode: bool,
    /// 0 none, 1 NA VHF, 2 EU VHF, 3 Field Day, 4 RTTY RU, 5 WW Digi, 6 Fox, 7 Hound.
    pub special_op_mode: u8,
    /// "WSJT-X", "JTDX" or "MSHV", from the instance id.
    pub program: &'static str,
    /// WSJT-X's configuration name (File, Settings, Configurations), when it sends one.
    pub configuration_name: String,
    /// A slice / VFO / receiver letter from the id, the configuration name or the matched rig.
    pub slice: Option<String>,
    /// The CAT/TCI/Hamlib radio on the same dial frequency, when there's one clear match.
    pub rig_key: Option<String>,
    pub rig_name: Option<String>,
    /// A short human label, e.g. "Slice A · WSJT-X" or "FT-991A · JTDX".
    pub source: String,
    /// Stable per instance (order of first appearance), 0..7, for colouring.
    pub color_index: u8,
    #[serde(skip)]
    addr: SocketAddr,
    /// The transmission period (ms since midnight UTC) last listed as a TX line.
    #[serde(skip)]
    tx_listed: Option<u32>,
}

/// Start of the T/R period holding `ms` (ms since midnight UTC). FT4's 7.5 s period is sent
/// as 7, so the mode wins over `tr_period`; with neither, FT8's 15 s.
pub fn tx_period_start(ms: u32, mode: &str, tr_period: u32) -> u32 {
    // WSJT-X can send no period (0) or "not set" (all bits on); only a sane one is believed.
    let period = match (mode, tr_period) {
        ("FT4", _) => 7_500,
        (_, s @ 1..=300) => s * 1000,
        _ if mode.starts_with("JT") || mode.starts_with("Q65") => 60_000,
        _ => 15_000,
    };
    // A transmission starts just after the boundary; a status sent a moment early still counts.
    let ms = (ms + 500) % 86_400_000;
    ms - ms % period
}

/// Where an instance's decodes come from: built once per Status, copied onto each decode.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FtxSource {
    pub program: &'static str,
    pub slice: Option<String>,
    pub rig_key: Option<String>,
    pub rig_name: Option<String>,
    pub source: String,
}

/// The program behind a WSJT-X protocol id. JTDX and MSHV say so in their id.
pub fn ftx_program(id: &str) -> &'static str {
    let up = id.to_ascii_uppercase();
    if up.contains("JTDX") {
        "JTDX"
    } else if up.contains("MSHV") {
        "MSHV"
    } else {
        "WSJT-X"
    }
}

/// Finds "Slice A", "slice-b", "VFO B", "RX2" and the like. Returns the letter and a short label.
pub fn slice_in(text: &str) -> Option<(char, String)> {
    let up = text.to_ascii_uppercase();
    let tokens: Vec<&str> = up.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| !t.is_empty()).collect();
    let letter = |s: &str, max: u8| {
        let b = s.as_bytes();
        (b.len() == 1 && (b'A'..=max).contains(&b[0])).then(|| b[0] as char)
    };
    let digit = |s: &str| {
        let b = s.as_bytes();
        (b.len() == 1 && (b'1'..=b'8').contains(&b[0])).then(|| b[0])
    };
    for (i, t) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1).copied().unwrap_or("");
        for (word, max, label) in [("SLICE", b'H', "Slice"), ("VFO", b'B', "VFO")] {
            if let Some(rest) = t.strip_prefix(word) {
                if let Some(c) = letter(if rest.is_empty() { next } else { rest }, max) {
                    return Some((c, format!("{label} {c}")));
                }
            }
        }
        if let Some(rest) = t.strip_prefix("RX") {
            if let Some(d) = digit(if rest.is_empty() { next } else { rest }) {
                return Some(((b'A' + d - b'1') as char, format!("RX{}", d as char)));
            }
        }
    }
    None
}

/// The channel letter of a multi-channel rig radio ("FLEX-6600 B"), as `follow_rig` names them.
fn rig_channel(r: &Radio) -> Option<char> {
    let (_, last) = r.name.rsplit_once(' ')?;
    let b = last.as_bytes();
    (b.len() == 1 && b[0].is_ascii_uppercase()).then(|| b[0] as char)
}

/// Works out an instance's source label from its id, configuration name, dial frequency and the
/// rigs QRZero follows. Cheap enough to run on every Status.
pub fn ftx_source<'a>(id: &str, config: &str, dial: u64, radios: impl IntoIterator<Item = &'a Radio>) -> FtxSource {
    let program = ftx_program(id);
    let config = config.trim();
    let named = slice_in(id).or_else(|| slice_in(config));
    let hay = format!("{id} {config}").to_ascii_uppercase();
    let mut best: Option<(u8, &Radio)> = None;
    let mut tie = false;
    for r in radios {
        if r.source != "rig" || !r.state.connected || r.state.freq_hz.abs_diff(dial) > 3000 {
            continue;
        }
        let ch = rig_channel(r);
        let mut score = 0;
        if named.as_ref().is_some_and(|(c, _)| ch == Some(*c)) {
            score += 2;
        }
        let base = match ch {
            Some(_) => r.name.rsplit_once(' ').map_or(r.name.as_str(), |(b, _)| b),
            None => r.name.as_str(),
        };
        if !base.is_empty() && hay.contains(&base.to_ascii_uppercase()) {
            score += 1;
        }
        match best {
            Some((s, _)) if s > score => {}
            Some((s, _)) if s == score => tie = true,
            _ => {
                best = Some((score, r));
                tie = false;
            }
        }
    }
    let rig = best.filter(|_| !tie).map(|(_, r)| r);
    let slice = named.as_ref().map(|(c, _)| *c).or_else(|| rig.and_then(rig_channel));
    let source = if let Some((_, label)) = &named {
        format!("{label} · {program}")
    } else if let Some(r) = rig {
        format!("{} · {program}", r.name)
    } else if !config.is_empty() && !config.eq_ignore_ascii_case("Default") {
        if config.to_ascii_uppercase().contains(&program.to_ascii_uppercase()) {
            config.to_string()
        } else {
            format!("{config} · {program}")
        }
    } else {
        id.to_string()
    };
    FtxSource {
        program,
        slice: slice.map(String::from),
        rig_key: rig.map(|r| r.key.clone()),
        rig_name: rig.map(|r| r.name.clone()),
        source,
    }
}

#[derive(Default, Serialize)]
pub struct ListenerStatus {
    pub wsjtx: Option<String>,
    pub n1mm: Option<String>,
    pub rotator: Option<String>,
}

struct RigConn {
    equipment_id: i64,
    config: RigConfig,
    handle: RigHandle,
    task: JoinHandle<()>,
}

impl Drop for RigConn {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct AwardCells {
    log_id: i64,
    version: i64,
    index: AwardIndex,
}

#[derive(Default)]
struct Inner {
    active: Active,
    rigs: Vec<RigConn>,
    radios: BTreeMap<String, Radio>,
    worked: Option<(i64, WorkedIndex)>,
    /// Award cells of one log, with the QSO version they reflect.
    awards: Option<AwardCells>,
    instances: BTreeMap<String, FtxInstance>,
    /// Colour slot per instance id, in order of first appearance.
    ftx_colors: HashMap<String, u8>,
    decodes: VecDeque<FtxDecode>,
    seq: u64,
    integrations: Integrations,
    listeners: Vec<JoinHandle<()>>,
    status: ListenerStatus,
    /// The socket each WSJT-X instance (by its address) talks to, for replies.
    wsjtx_sockets: HashMap<SocketAddr, Arc<UdpSocket>>,
    /// Per listen address, how its WSJT-X listener is doing.
    wsjtx_status: BTreeMap<String, String>,
    rotator_az: Option<f64>,
}

impl Inner {
    fn ftx_color(&mut self, id: &str) -> u8 {
        if let Some(c) = self.ftx_colors.get(id) {
            return *c;
        }
        let c = (self.ftx_colors.len() % 8) as u8;
        self.ftx_colors.insert(id.to_string(), c);
        c
    }
}

pub struct Hub {
    store: Arc<Mutex<Store>>,
    data_dir: PathBuf,
    events: broadcast::Sender<Arc<str>>,
    inner: Mutex<Inner>,
    cty: RwLock<Option<Arc<CtyDb>>>,
    pub(crate) watch: crate::watch::Watch,
    /// The user's own UDP connections and relays.
    pub(crate) udp: Arc<crate::udp_out::Outputs>,
    /// Where auto-logged QSOs go to be looked up: (log id, QSO id, call).
    auto_lookup: Mutex<Option<tokio::sync::mpsc::UnboundedSender<(i64, i64, String)>>>,
}

impl Hub {
    pub fn new(store: Arc<Mutex<Store>>, data_dir: PathBuf) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        let watch_list = store
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_setting(crate::watch::SETTING_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let watch = crate::watch::Watch::new(watch_list);
        let conns = store
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_setting(crate::udp_out::SETTING_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let udp = crate::udp_out::Outputs::new(conns);
        let hub = Arc::new(Hub { store, data_dir, events, inner: Mutex::default(), cty: RwLock::default(), watch, udp, auto_lookup: Mutex::default() });
        if let Err(e) = hub.load_cty_file() {
            tracing::info!("no country file yet: {e}");
        }
        hub
    }

    /// Loads saved settings and starts the listeners. Call once from inside the runtime.
    pub fn start(self: &Arc<Self>, update_cty: bool) {
        let integrations = self
            .with_store(|st| st.get_setting("integrations"))
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        self.set_integrations(integrations);
        // Until the UI says otherwise: the first log, its default location and callsign.
        let initial = self.with_store(|st| {
            let Some(log) = st.list_logs()?.into_iter().next() else { return Ok(Active::default()) };
            let location_id = st.list_locations(log.id)?.into_iter().find(|l| l.is_default).map(|l| l.id);
            let station_callsign = st.list_callsigns(log.id)?.into_iter().find(|c| c.is_default).map(|c| c.callsign).unwrap_or_default();
            Ok(Active { log_id: Some(log.id), location_id, station_callsign })
        });
        if let Ok(active) = initial {
            self.set_active(active);
        }
        if !update_cty {
            return;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            if hub.cty_age().is_none_or(|age| age > CTY_MAX_AGE) {
                match hub.update_cty().await {
                    Ok(_) => hub.rebuild_worked(),
                    Err(e) => tracing::info!("country file download failed: {e}"),
                }
            }
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn with_store<T>(&self, f: impl FnOnce(&mut Store) -> qrzero_core::Result<T>) -> qrzero_core::Result<T> {
        let mut st = self.store.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut st)
    }

    pub(crate) fn setting(&self, key: &str) -> Option<String> {
        self.with_store(|st| st.get_setting(key)).ok().flatten()
    }

    pub(crate) fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.with_store(|st| st.set_setting(key, value)).map_err(|e| e.to_string())
    }

    pub(crate) fn emit(&self, v: serde_json::Value) {
        let _ = self.events.send(v.to_string().into());
    }

    /// The event stream, preceded by a snapshot of the current state.
    pub fn subscribe(&self) -> (Vec<Arc<str>>, broadcast::Receiver<Arc<str>>) {
        let rx = self.events.subscribe();
        let inner = self.lock();
        let snapshot = [
            json!({"type": "radios", "radios": inner.radios.values().collect::<Vec<_>>()}),
            json!({"type": "ftx_instances", "instances": inner.instances.values().collect::<Vec<_>>()}),
            json!({"type": "rotator", "azimuth": inner.rotator_az}),
            json!({"type": "integrations", "config": inner.integrations}),
        ];
        (snapshot.iter().map(|v| v.to_string().into()).collect(), rx)
    }

    // ---- active log/location --------------------------------------------

    pub fn set_active(self: &Arc<Self>, active: Active) {
        let (log_changed, loc_changed) = {
            let mut inner = self.lock();
            let changes = (inner.active.log_id != active.log_id, inner.active.location_id != active.location_id);
            inner.active = active;
            changes
        };
        if log_changed {
            self.rebuild_worked();
        }
        if loc_changed {
            self.reload_rigs();
        }
    }

    /// What's new about a station for the active log, or None before the log is indexed.
    pub fn needed(&self, call: &str, dxcc: Option<u32>, band: Option<&str>, mode: Option<&str>) -> Option<Needed> {
        self.lock().worked.as_ref().map(|(_, idx)| idx.needed(call, dxcc, band, mode))
    }

    pub fn active(&self) -> Active {
        self.lock().active.clone()
    }

    /// Rebuilds the worked-before sets in the background (after imports, edits and deletes).
    pub fn rebuild_worked(self: &Arc<Self>) {
        let Some(log_id) = self.lock().active.log_id else { return };
        let hub = self.clone();
        let cty = hub.cty.read().unwrap_or_else(|p| p.into_inner()).clone();
        tokio::task::spawn_blocking(move || match hub.with_store(|st| st.worked_index(log_id, |call| cty.as_ref()?.lookup(call)?.dxcc)) {
            Ok(idx) => {
                let mut inner = hub.lock();
                if inner.active.log_id == Some(log_id) {
                    inner.worked = Some((log_id, idx));
                }
            }
            Err(e) => tracing::warn!("worked index: {e}"),
        });
        self.refresh_awards();
    }

    /// Recounts the active log's award cells in the background, after QSL
    /// changes, so the next lookup doesn't wait for it.
    pub fn refresh_awards(self: &Arc<Self>) {
        let Some(log_id) = self.lock().active.log_id else { return };
        let hub = self.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = hub.build_award_cells(log_id) {
                tracing::warn!("award index: {e}");
            }
        });
    }

    /// Counts a log's award cells and keeps them, unless newer ones are already kept.
    fn build_award_cells(&self, log_id: i64) -> qrzero_core::Result<()> {
        let cty = self.cty();
        let (version, index) = self.with_store(|st| st.award_index(log_id, |call| cty.as_ref()?.lookup(call)?.dxcc))?;
        let mut inner = self.lock();
        if inner.awards.as_ref().is_none_or(|a| a.log_id != log_id || a.version <= version) {
            inner.awards = Some(AwardCells { log_id, version, index });
        }
        Ok(())
    }

    /// What a QSO would add to each award in a log. Uses the kept award cells
    /// when they are up to date, else counts the log first (after edits,
    /// deletes, imports and confirmations).
    pub fn award_hints(&self, log_id: i64, q: &AwardQso, counts: Counts) -> qrzero_core::Result<Vec<AwardHint>> {
        let version = self.with_store(|st| st.qso_version())?;
        let fresh = |inner: &Inner| {
            inner
                .awards
                .as_ref()
                .filter(|a| a.log_id == log_id && a.version == version)
                .map(|a| a.index.hints(q, counts))
        };
        if let Some(h) = fresh(&self.lock()) {
            return Ok(h);
        }
        self.build_award_cells(log_id)?;
        let inner = self.lock();
        match inner.awards.as_ref().filter(|a| a.log_id == log_id) {
            Some(a) => Ok(a.index.hints(q, counts)),
            None => Ok(Vec::new()),
        }
    }

    /// Records a newly logged QSO in the worked-before sets.
    pub fn note_qso(&self, log_id: i64, f: &Fields) {
        // The award cells take the QSO only if it is the one change since they were
        // counted; otherwise they are stale and get recounted when next asked.
        if let Ok(version) = self.with_store(|st| st.qso_version()) {
            let cty = self.cty();
            let qso = AwardQso::from_fields(f, |call| cty.as_ref()?.lookup(call)?.dxcc);
            if let Some(a) = self.lock().awards.as_mut() {
                if a.log_id == log_id && a.version + 1 == version {
                    a.index.add(&qso);
                    a.version = version;
                }
            }
        }
        if let Some((id, idx)) = self.lock().worked.as_mut() {
            if *id == log_id {
                let mode = f.get("SUBMODE").or(f.get("MODE"));
                let dxcc = f.get("DXCC").and_then(|d| d.parse().ok());
                idx.add(f.get("CALL").map_or("", |c| c.as_str()), dxcc, f.get("BAND").map(|s| s.as_str()), mode.map(|s| s.as_str()));
            }
        }
    }

    /// Tells the UDP connections about a newly logged QSO.
    pub fn send_qso(&self, f: &Fields, qso_id: i64) {
        if self.udp.wants(UdpEvent::QsoLogged) {
            self.udp.fire(UdpCtx::qso(f, Some(qso_id)));
        }
    }

    /// Tells the UDP connections a call was entered, with the heading to it from the active location.
    pub fn send_lookup(&self, call: &str, station: Option<&Fields>, entity: Option<&Entity>) {
        if !self.udp.wants(UdpEvent::Lookup) {
            return;
        }
        let grid = station.and_then(|f| f.get("GRIDSQUARE")).map(|g| g.trim().to_string()).unwrap_or_default();
        let loc_id = self.active().location_id;
        let my_grid = loc_id
            .and_then(|id| self.with_store(|st| st.get_location(id)).ok())
            .and_then(|l| l.fields.get("MY_GRIDSQUARE").cloned())
            .unwrap_or_default();
        let to = crate::udp_out::grid_latlon(&grid).or_else(|| entity.map(|e| (e.lat, e.lon)));
        let az = crate::udp_out::grid_latlon(&my_grid).zip(to).map(|(a, b)| crate::udp_out::bearing(a, b));
        self.udp.fire(UdpCtx {
            event: UdpEvent::Lookup,
            call: call.to_string(),
            grid,
            az,
            fields: station.cloned().unwrap_or_default(),
            ..UdpCtx::default()
        });
    }

    // ---- rigs -------------------------------------------------------------

    /// (Re)connects the rigs set up for control at the active location.
    pub fn reload_rigs(self: &Arc<Self>) {
        let (log_id, loc_id) = {
            let inner = self.lock();
            (inner.active.log_id, inner.active.location_id)
        };
        let wanted: Vec<(i64, String, RigConfig)> = match (log_id, loc_id) {
            (Some(log), Some(loc)) => self
                .with_store(|st| st.list_equipment(log))
                .unwrap_or_default()
                .into_iter()
                .filter(|e| e.location_id == loc && e.kind == "rig")
                .filter_map(|e| rig_config(&e.fields).map(|c| (e.id, e.name, c)))
                .collect(),
            _ => Vec::new(),
        };
        let mut inner = self.lock();
        inner.rigs.retain(|r| wanted.iter().any(|(id, _, c)| *id == r.equipment_id && *c == r.config));
        let keep: Vec<i64> = inner.rigs.iter().map(|r| r.equipment_id).collect();
        inner.radios.retain(|k, _| !k.starts_with("rig:") || keep.iter().any(|id| k.starts_with(&format!("rig:{id}:"))));
        for (id, name, config) in wanted {
            if keep.contains(&id) {
                continue;
            }
            let handle = rig::spawn(config.clone());
            let task = tokio::spawn(follow_rig(Arc::downgrade(self), id, name, handle.subscribe()));
            inner.rigs.push(RigConn { equipment_id: id, config, handle, task });
        }
        let radios: Vec<_> = inner.radios.values().cloned().collect();
        drop(inner);
        self.emit(json!({"type": "radios", "radios": radios}));
    }

    fn set_radio(&self, radio: Radio) {
        if radio.state.connected && radio.state.freq_hz > 0 && self.udp.wants(UdpEvent::Radio) {
            self.udp.fire(radio_ctx(&radio));
        }
        let mut inner = self.lock();
        inner.radios.insert(radio.key.clone(), radio);
        let radios: Vec<_> = inner.radios.values().cloned().collect();
        drop(inner);
        self.emit(json!({"type": "radios", "radios": radios}));
    }

    fn remove_radios(&self, prefix: &str) {
        let mut inner = self.lock();
        let before = inner.radios.len();
        inner.radios.retain(|k, _| !k.starts_with(prefix));
        if inner.radios.len() != before {
            let radios: Vec<_> = inner.radios.values().cloned().collect();
            drop(inner);
            self.emit(json!({"type": "radios", "radios": radios}));
        }
    }

    /// Tunes a rig (from the entry panel or a clicked spot).
    pub fn tune(&self, key: &str, freq_hz: Option<u64>, mode: Option<String>) -> Result<(), String> {
        let mut parts = key.split(':');
        let (Some("rig"), Some(id), Some(ch)) = (parts.next(), parts.next(), parts.next()) else {
            return Err("that radio can't be tuned from QRZero".into());
        };
        let (id, ch): (i64, usize) = (id.parse().map_err(|_| "bad radio")?, ch.parse().map_err(|_| "bad radio")?);
        let inner = self.lock();
        let conn = inner.rigs.iter().find(|r| r.equipment_id == id).ok_or("that radio isn't connected")?;
        if let Some(f) = freq_hz {
            conn.handle.send(ch, RigCommand::SetFreq(f));
        }
        if let Some(m) = mode.filter(|m| !m.is_empty()) {
            // The rig picks the sideband from its current frequency, which may not have caught up yet.
            let m = match (m.as_str(), freq_hz) {
                ("SSB", Some(f)) => if f < 10_000_000 && !(5_250_000..=5_450_000).contains(&f) { "LSB" } else { "USB" }.to_string(),
                _ => m,
            };
            conn.handle.send(ch, RigCommand::SetMode(m));
        }
        Ok(())
    }

    // ---- integrations -----------------------------------------------------

    pub fn integrations(&self) -> (Integrations, serde_json::Value) {
        let inner = self.lock();
        (inner.integrations.clone(), serde_json::to_value(&inner.status).unwrap_or_default())
    }

    pub fn save_integrations(self: &Arc<Self>, cfg: Integrations) -> qrzero_core::Result<()> {
        let text = serde_json::to_string(&cfg)?;
        self.with_store(|st| st.set_setting("integrations", &text))?;
        self.set_integrations(cfg);
        Ok(())
    }

    fn set_integrations(self: &Arc<Self>, cfg: Integrations) {
        let mut inner = self.lock();
        for l in inner.listeners.drain(..) {
            l.abort();
        }
        inner.wsjtx_sockets.clear();
        inner.wsjtx_status.clear();
        inner.status = ListenerStatus::default();
        inner.integrations = cfg.clone();
        let weak = Arc::downgrade(self);
        if cfg.wsjtx_enabled {
            for addr in cfg.wsjtx_addrs() {
                inner.wsjtx_status.insert(addr.clone(), format!("starting on {addr}"));
                inner.listeners.push(tokio::spawn(wsjtx_listener(weak.clone(), cfg.clone(), addr)));
            }
            inner.status.wsjtx = Some(inner.wsjtx_status.values().cloned().collect::<Vec<_>>().join("; "));
        }
        if cfg.n1mm_enabled {
            inner.status.n1mm = Some(format!("starting on {}", cfg.n1mm_listen));
            inner.listeners.push(tokio::spawn(n1mm_listener(weak.clone(), cfg.n1mm_listen.clone())));
        }
        if cfg.rotator_enabled {
            inner.status.rotator = Some(format!("talking to {}", cfg.rotator_addr));
            inner.listeners.push(tokio::spawn(rotator_poller(weak, cfg.rotator_addr.clone())));
        }
        drop(inner);
        self.remove_radios("wsjtx:");
        self.remove_radios("n1mm:");
        let mut inner = self.lock();
        inner.instances.clear();
        drop(inner);
        self.emit(json!({"type": "ftx_instances", "instances": []}));
        self.emit(json!({"type": "integrations", "config": cfg}));
    }

    fn set_wsjtx_status(&self, addr: &str, text: String) {
        let mut inner = self.lock();
        inner.wsjtx_status.insert(addr.to_string(), text);
        inner.status.wsjtx = Some(inner.wsjtx_status.values().cloned().collect::<Vec<_>>().join("; "));
    }

    fn set_status(&self, which: &str, text: String) {
        let mut inner = self.lock();
        let slot = match which {
            "wsjtx" => &mut inner.status.wsjtx,
            "n1mm" => &mut inner.status.n1mm,
            _ => &mut inner.status.rotator,
        };
        *slot = Some(text);
    }

    // ---- WSJT-X / JTDX ----------------------------------------------------

    pub fn ftx_snapshot(&self, since: u64) -> serde_json::Value {
        let inner = self.lock();
        let decodes: Vec<_> = inner.decodes.iter().filter(|d| d.seq > since).collect();
        json!({"instances": inner.instances.values().collect::<Vec<_>>(), "decodes": decodes})
    }

    /// Asks the WSJT-X instance that decoded a message to answer it, as a double-click there would.
    pub async fn ftx_reply(&self, seq: u64) -> Result<(), String> {
        let (socket, addr, packet) = {
            let inner = self.lock();
            let d = inner.decodes.iter().find(|d| d.seq == seq).ok_or("that decode is no longer listed")?;
            if d.tx {
                return Err("that's your own transmission".into());
            }
            let inst = inner.instances.get(&d.instance).ok_or("that WSJT-X is no longer running")?;
            let socket = inner.wsjtx_sockets.get(&inst.addr).cloned().ok_or("WSJT-X listening is off")?;
            (socket, inst.addr, wsjtx::encode_reply(&d.raw, 0))
        };
        socket.send_to(&packet, addr).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Sends a request to a WSJT-X instance, from the socket and to the address its packets come
    /// from. `make` builds the datagram from the instance's id and state.
    pub async fn ftx_send(&self, instance: &str, make: impl FnOnce(&FtxInstance) -> Vec<u8>) -> Result<(), String> {
        let (socket, addr, packet) = {
            let inner = self.lock();
            let inst = inner.instances.get(instance).ok_or("that WSJT-X is no longer running")?;
            let socket = inner.wsjtx_sockets.get(&inst.addr).cloned().ok_or("WSJT-X listening is off")?;
            (socket, inst.addr, make(inst))
        };
        socket.send_to(&packet, addr).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Asks WSJT-X to clear its Band Activity (0), Rx Frequency (1) or both (2) windows, and
    /// drops the instance's decodes here too when Band Activity is cleared.
    pub async fn ftx_clear(&self, instance: &str, window: u8) -> Result<(), String> {
        self.ftx_send(instance, |i| wsjtx::encode_clear(&i.id, window)).await?;
        if window != 1 {
            self.clear_decodes(instance);
        }
        Ok(())
    }

    fn clear_decodes(&self, id: &str) {
        self.lock().decodes.retain(|d| d.instance != id);
        self.emit(json!({"type": "ftx_clear", "instance": id}));
    }

    /// The TX line for an instance that is transmitting, once per transmission period.
    fn tx_line(inner: &mut Inner, id: &str, now_ms: u32) -> Option<FtxDecode> {
        let inst = inner.instances.get_mut(id)?;
        let msg = inst.tx_message.trim();
        if !inst.transmitting || msg.is_empty() {
            return None;
        }
        let start = tx_period_start(now_ms, &inst.mode, inst.tr_period);
        if inst.tx_listed == Some(start) {
            return None;
        }
        inst.tx_listed = Some(start);
        let ft = wsjtx::parse_ft_message(msg);
        let time = start / 1000;
        let d = FtxDecode {
            seq: 0,
            instance: id.to_string(),
            time: format!("{:02}{:02}{:02}", time / 3600, time / 60 % 60, time % 60),
            snr: 0,
            dt: 0.0,
            df: inst.tx_df,
            mode: inst.mode.clone(),
            message: msg.to_string(),
            call: None,
            to: ft.to,
            grid: ft.grid,
            cq: false,
            cq_target: None,
            to_me: false,
            band: inst.band.clone(),
            freq_hz: inst.dial_freq + u64::from(inst.tx_df),
            entity: None,
            needed: None,
            low_confidence: false,
            source: inst.source.clone(),
            slice: inst.slice.clone(),
            color_index: inst.color_index,
            watched: None,
            tx: true,
            raw: wsjtx::Decode {
                id: id.to_string(),
                new: true,
                time_ms: start,
                snr: 0,
                dt: 0.0,
                df: inst.tx_df,
                mode: String::new(),
                message: msg.to_string(),
                low_confidence: false,
                off_air: false,
            },
        };
        inner.seq += 1;
        Some(FtxDecode { seq: inner.seq, ..d })
    }

    fn push_decode(&self, decode: FtxDecode) {
        let mut inner = self.lock();
        inner.decodes.push_back(decode);
        while inner.decodes.len() > MAX_DECODES {
            inner.decodes.pop_front();
        }
    }

    fn on_wsjtx(&self, msg: wsjtx::Message, from: SocketAddr) {
        use wsjtx::Message as M;
        match msg {
            M::Heartbeat { id, .. } => {
                let mut inner = self.lock();
                if let Some(i) = inner.instances.get_mut(&id) {
                    i.addr = from;
                }
            }
            M::Status {
                id,
                dial_freq,
                mode,
                dx_call,
                report,
                transmitting,
                tx_enabled,
                decoding,
                rx_df,
                tx_df,
                de_call,
                de_grid,
                dx_grid,
                tx_watchdog,
                sub_mode,
                fast_mode,
                special_op_mode,
                tr_period,
                configuration_name,
                tx_message,
                ..
            } => {
                let tr_period = if tr_period <= 300 { tr_period } else { 0 };
                let (changed, instances, tx) = {
                    let mut inner = self.lock();
                    let src = ftx_source(&id, &configuration_name, dial_freq, inner.radios.values());
                    let color_index = inner.ftx_color(&id);
                    let tx_listed = inner.instances.get(&id).and_then(|i| i.tx_listed);
                    let inst = FtxInstance {
                        id: id.clone(),
                        dial_freq,
                        band: band_for_freq(dial_freq as f64 / 1e6).map(str::to_string),
                        mode: mode.clone(),
                        de_call,
                        de_grid,
                        dx_call,
                        dx_grid,
                        report,
                        transmitting,
                        tx_enabled,
                        decoding,
                        tx_message,
                        rx_df,
                        tx_df,
                        tx_watchdog,
                        tr_period,
                        sub_mode,
                        fast_mode,
                        special_op_mode,
                        program: src.program,
                        configuration_name: configuration_name.trim().to_string(),
                        slice: src.slice,
                        rig_key: src.rig_key,
                        rig_name: src.rig_name,
                        source: src.source,
                        color_index,
                        addr: from,
                        tx_listed,
                    };
                    // Everything the UI sees; the address and TX bookkeeping don't count.
                    let key = |i: &FtxInstance| serde_json::to_value(i).ok();
                    let old = inner.instances.insert(id.clone(), inst.clone());
                    let changed = old.is_none_or(|o| key(&o) != key(&inst));
                    let now = chrono::Utc::now().timestamp_millis().rem_euclid(86_400_000) as u32;
                    let tx = Self::tx_line(&mut inner, &id, now);
                    (changed, inner.instances.values().cloned().collect::<Vec<_>>(), tx)
                };
                if changed {
                    self.emit(json!({"type": "ftx_instances", "instances": instances}));
                    let state = RigState {
                        connected: true,
                        freq_hz: dial_freq,
                        mode: String::new(),
                        rig_mode: mode,
                        data: true,
                        tx: transmitting,
                        error: None,
                    };
                    self.set_radio(Radio { key: format!("wsjtx:{id}"), name: id, source: "wsjtx", can_tune: false, state });
                }
                if let Some(tx) = tx {
                    self.emit(json!({"type": "ftx_tx", "decode": &tx}));
                    self.push_decode(tx);
                }
            }
            M::Decode(d) => {
                // A replay (or JTDX re-sending its window) repeats decodes already listed.
                if !d.new {
                    let time = d.time_ms / 1000;
                    let hms = format!("{:02}{:02}{:02}", time / 3600, time / 60 % 60, time % 60);
                    let inner = self.lock();
                    if inner.decodes.iter().any(|x| !x.tx && x.instance == d.id && x.time == hms && x.message == d.message) {
                        return;
                    }
                }
                if let Some(decode) = self.make_decode(d) {
                    self.emit(json!({"type": "decode", "decode": &decode}));
                    if let (Some(id), Some(call)) = (decode.watched, &decode.call) {
                        let s = crate::watch::Sighting {
                            call,
                            entity_prefix: decode.entity.as_ref().map(|e| e.prefix.as_str()),
                            entity_name: decode.entity.as_ref().map(|e| e.name.as_str()),
                            band: decode.band.as_deref(),
                            mode: &decode.mode,
                            freq_hz: decode.freq_hz,
                            grid: decode.grid.as_deref(),
                            source: "ftx",
                            detail: &decode.message,
                        };
                        self.watch_hit(id, &s);
                    }
                    self.push_decode(decode);
                }
            }
            M::Clear { id, window } => {
                if window != 1 {
                    self.clear_decodes(&id);
                }
            }
            M::Close { id } => {
                let instances = {
                    let mut inner = self.lock();
                    inner.instances.remove(&id);
                    inner.instances.values().cloned().collect::<Vec<_>>()
                };
                self.remove_radios(&format!("wsjtx:{id}"));
                self.emit(json!({"type": "ftx_instances", "instances": instances}));
            }
            M::LoggedAdif { id, adif } if self.lock().integrations.wsjtx_auto_log => {
                for record in adif::parse(adif.as_bytes()).records {
                    self.auto_log(record, &id, None);
                }
            }
            _ => {}
        }
    }

    fn make_decode(&self, d: wsjtx::Decode) -> Option<FtxDecode> {
        let ft = wsjtx::parse_ft_message(&d.message);
        let entity = ft.from.as_deref().and_then(|c| self.entity(c));
        let mut inner = self.lock();
        let color_index = inner.ftx_color(&d.id);
        let inst = inner.instances.get(&d.id);
        let source = inst.map_or_else(|| d.id.clone(), |i| i.source.clone());
        let slice = inst.and_then(|i| i.slice.clone());
        let dial = inst.map_or(0, |i| i.dial_freq);
        let band = inst.and_then(|i| i.band.clone());
        let mode = inst.map(|i| i.mode.clone()).filter(|m| !m.is_empty()).unwrap_or_else(|| decode_mode(&d.mode).to_string());
        let mine = [inst.map(|i| i.de_call.clone()).unwrap_or_default(), inner.active.station_callsign.clone()];
        let to_me = ft.to.as_ref().is_some_and(|t| mine.iter().any(|m| !m.is_empty() && m.eq_ignore_ascii_case(t)));
        let needed = match (&inner.worked, &ft.from) {
            (Some((_, idx)), Some(call)) => Some(idx.needed(call, entity.as_ref().and_then(|e| e.dxcc), band.as_deref(), Some(&mode))),
            _ => None,
        };
        let watched = ft.from.as_deref().and_then(|call| {
            self.watch.check(&crate::watch::Sighting {
                call,
                entity_prefix: entity.as_ref().map(|e| e.prefix.as_str()),
                band: band.as_deref(),
                mode: &mode,
                ..Default::default()
            })
        });
        inner.seq += 1;
        let time = d.time_ms / 1000;
        Some(FtxDecode {
            seq: inner.seq,
            instance: d.id.clone(),
            time: format!("{:02}{:02}{:02}", time / 3600, time / 60 % 60, time % 60),
            snr: d.snr,
            dt: d.dt,
            df: d.df,
            mode,
            message: d.message.clone(),
            call: ft.from,
            to: ft.to,
            grid: ft.grid,
            cq: ft.cq,
            cq_target: ft.cq_target,
            to_me,
            band,
            freq_hz: dial + d.df as u64,
            entity,
            needed,
            low_confidence: d.low_confidence,
            source,
            slice,
            color_index,
            watched,
            tx: false,
            raw: d,
        })
    }

    /// Sets where auto-logged QSOs are sent to be looked up.
    pub fn set_auto_lookup(&self, tx: tokio::sync::mpsc::UnboundedSender<(i64, i64, String)>) {
        *self.auto_lookup.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
    }

    /// Logs a QSO that came from another program, unless it's already in the log.
    fn auto_log(&self, mut fields: Fields, source: &str, n1mm_id: Option<(&str, bool)>) {
        let active = self.active();
        let Some(log_id) = active.log_id else { return };
        if !active.station_callsign.is_empty() {
            fields.entry("STATION_CALLSIGN".into()).or_insert_with(|| active.station_callsign.clone());
        }
        self.fill_from_cty(&mut fields);
        let result = self.with_store(|st| {
            if let Some(loc) = active.location_id {
                let gear = st.list_equipment(log_id)?;
                fill_antenna(&mut fields, gear.iter().filter(|e| e.location_id == loc));
            }
            if let Some((id, replace)) = n1mm_id {
                fields.insert("APP_QRZERO_N1MM_ID".into(), id.to_string());
                if replace {
                    if let Some(existing) = st.find_qso_by_field(log_id, "APP_QRZERO_N1MM_ID", id)? {
                        let old = st.get_qso(existing)?;
                        st.update_qso(existing, old.location_id, &fields)?;
                        return Ok(None);
                    }
                }
            }
            if st.find_duplicate(log_id, &fields)?.is_some() {
                return Ok(None);
            }
            Ok(Some(st.insert_qso(log_id, active.location_id, &fields)?))
        });
        match result {
            Ok(added) => {
                let added = match added {
                    Some(qso) => {
                        self.note_qso(log_id, &qso.fields);
                        self.send_qso(&qso.fields, qso.id);
                        if self.lock().integrations.auto_log_lookup {
                            if let (Some(tx), Some(call)) = (self.auto_lookup.lock().unwrap_or_else(|p| p.into_inner()).as_ref(), qso.fields.get("CALL")) {
                                let _ = tx.send((log_id, qso.id, call.clone()));
                            }
                        }
                        true
                    }
                    None => false,
                };
                let call = fields.get("CALL").cloned().unwrap_or_default();
                self.emit(json!({"type": "qso_logged", "log_id": log_id, "call": call, "source": source, "added": added}));
            }
            Err(e) => {
                tracing::warn!("could not log QSO from {source}: {e}");
                self.emit(json!({"type": "error", "message": format!("Couldn't log the QSO from {source}: {e}")}));
            }
        }
    }

    // ---- N1MM -----------------------------------------------------------

    fn on_n1mm(&self, msg: n1mm::N1mmMessage) {
        use n1mm::N1mmMessage as M;
        match msg {
            M::Contact { replace, id, fields } => {
                if self.lock().integrations.n1mm_auto_log {
                    self.auto_log(fields.into_iter().collect(), "N1MM", Some((&id, replace)));
                }
            }
            M::ContactDelete { id, .. } => {
                let active = self.active();
                if let (Some(log_id), true) = (active.log_id, self.lock().integrations.n1mm_auto_log) {
                    let deleted = self.with_store(|st| match st.find_qso_by_field(log_id, "APP_QRZERO_N1MM_ID", &id)? {
                        Some(q) => st.delete_qsos(&[q]),
                        None => Ok(0),
                    });
                    if deleted.is_ok_and(|n| n > 0) {
                        self.emit(json!({"type": "qso_logged", "log_id": log_id, "call": "", "source": "N1MM", "added": false}));
                    }
                }
            }
            M::RadioInfo { station, radio_nr, freq_hz, mode, is_transmitting, .. } => {
                let state = RigState {
                    connected: true,
                    freq_hz,
                    mode: n1mm_mode(&mode).to_string(),
                    data: n1mm_mode(&mode).is_empty(),
                    rig_mode: mode,
                    tx: is_transmitting,
                    error: None,
                };
                let name = if station.is_empty() { format!("N1MM radio {radio_nr}") } else { format!("N1MM {station} radio {radio_nr}") };
                self.set_radio(Radio { key: format!("n1mm:{station}:{radio_nr}"), name, source: "n1mm", can_tune: false, state });
            }
            _ => {}
        }
    }

    // ---- rotator ----------------------------------------------------------

    pub async fn rotate(&self, azimuth: f64) -> Result<(), String> {
        let (enabled, addr) = {
            let inner = self.lock();
            (inner.integrations.rotator_enabled, inner.integrations.rotator_addr.clone())
        };
        let others = self.udp.wants(UdpEvent::Rotator);
        if others {
            self.udp.fire(UdpCtx { event: UdpEvent::Rotator, az: Some(azimuth), ..UdpCtx::default() });
        }
        if !enabled {
            if others {
                return Ok(());
            }
            return Err("turn on the rotator in Settings, Radios first".into());
        }
        let sock = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
        sock.send_to(pst::set_azimuth(azimuth).as_bytes(), &addr).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- country file ---------------------------------------------------

    /// Alerts for a watched station (at most once per ten minutes per call and band).
    pub(crate) fn watch_hit(&self, entry_id: u64, s: &crate::watch::Sighting) {
        if let Some(hit) = self.watch.record(entry_id, s, chrono::Utc::now().timestamp()) {
            self.emit(json!({"type": "watch_hit", "hit": hit}));
        }
    }

    pub fn cty(&self) -> Option<Arc<CtyDb>> {
        self.cty.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn entity(&self, call: &str) -> Option<Entity> {
        self.cty.read().unwrap_or_else(|p| p.into_inner()).as_ref()?.lookup(call)
    }

    /// Fills DXCC, country, continent and zones from the country file where the QSO lacks them.
    pub fn fill_from_cty(&self, f: &mut Fields) {
        let Some(e) = f.get("CALL").and_then(|c| self.entity(c)) else { return };
        let mut put = |k: &str, v: String| {
            if !v.is_empty() && v != "0" {
                f.entry(k.to_string()).or_insert(v);
            }
        };
        if let Some(d) = e.dxcc {
            put("DXCC", d.to_string());
            put("COUNTRY", e.name.clone());
        }
        put("CONT", e.cont.clone());
        put("CQZ", e.cq.to_string());
        put("ITUZ", e.itu.to_string());
    }

    fn cty_path(&self) -> Option<PathBuf> {
        ["cty.csv", "cty.dat"].iter().map(|n| self.data_dir.join(n)).find(|p| p.exists())
    }

    fn cty_age(&self) -> Option<Duration> {
        let modified = std::fs::metadata(self.cty_path()?).ok()?.modified().ok()?;
        modified.elapsed().ok()
    }

    fn load_cty_file(&self) -> anyhow::Result<usize> {
        let path = self.cty_path().ok_or_else(|| anyhow::anyhow!("no cty.csv or cty.dat in the data folder"))?;
        let db = CtyDb::parse(&std::fs::read_to_string(&path)?)?;
        let n = db.len();
        *self.cty.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(db));
        Ok(n)
    }

    /// Saves and loads a country file (downloaded or picked by the user).
    pub fn install_cty(&self, text: &str) -> anyhow::Result<usize> {
        let db = CtyDb::parse(text)?;
        if db.is_empty() {
            anyhow::bail!("that file has no countries in it");
        }
        let name = if text.trim_start().starts_with(|c: char| c.is_ascii_alphanumeric() || c == '*') && text.lines().next().is_some_and(|l| l.contains(',') && !l.contains(':')) {
            "cty.csv"
        } else {
            "cty.dat"
        };
        for old in ["cty.csv", "cty.dat"] {
            let _ = std::fs::remove_file(self.data_dir.join(old));
        }
        std::fs::write(self.data_dir.join(name), text)?;
        let n = db.len();
        *self.cty.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(db));
        self.emit(json!({"type": "cty", "entities": n}));
        Ok(n)
    }

    pub async fn update_cty(&self) -> anyhow::Result<usize> {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
        let mut last = anyhow::anyhow!("no download address");
        for url in CTY_URLS {
            match client.get(url).send().await.and_then(|r| r.error_for_status()) {
                Ok(resp) => match resp.text().await {
                    Ok(text) => match self.install_cty(&text) {
                        Ok(n) => return Ok(n),
                        Err(e) => last = e,
                    },
                    Err(e) => last = e.into(),
                },
                Err(e) => last = e.into(),
            }
        }
        Err(last)
    }

    pub fn cty_status(&self) -> serde_json::Value {
        let n = self.cty.read().unwrap_or_else(|p| p.into_inner()).as_ref().map_or(0, |d| d.len());
        let age_days = self.cty_age().map(|a| a.as_secs() / 86400);
        let file = self.cty_path().and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()));
        json!({"entities": n, "age_days": age_days, "file": file})
    }
}

// ---- background tasks -----------------------------------------------------

async fn follow_rig(hub: std::sync::Weak<Hub>, id: i64, name: String, mut rx: tokio::sync::watch::Receiver<Vec<RigState>>) {
    loop {
        let states = rx.borrow_and_update().clone();
        let Some(hub) = hub.upgrade() else { return };
        let many = states.len() > 1;
        for (ch, state) in states.into_iter().enumerate() {
            let name = if many { format!("{name} {}", (b'A' + ch as u8) as char) } else { name.clone() };
            hub.set_radio(Radio { key: format!("rig:{id}:{ch}"), name, source: "rig", can_tune: true, state });
        }
        drop(hub);
        if rx.changed().await.is_err() {
            return;
        }
    }
}

fn bind_udp(listen: &str, multicast: &str) -> anyhow::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let addr: SocketAddr = listen.parse().map_err(|_| anyhow::anyhow!("{listen} isn't an address like 127.0.0.1:2237"))?;
    let group: Option<Ipv4Addr> = if multicast.trim().is_empty() {
        None
    } else {
        Some(multicast.trim().parse().map_err(|_| anyhow::anyhow!("{multicast} isn't a multicast address"))?)
    };
    let sock = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    let bind = match group {
        Some(g) => {
            sock.set_reuse_address(true)?;
            if !g.is_multicast() {
                anyhow::bail!("{g} isn't a multicast address (224.0.0.0 to 239.255.255.255)");
            }
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, addr.port()))
        }
        None => addr,
    };
    sock.set_nonblocking(true)?;
    sock.bind(&bind.into())?;
    if let Some(g) = group {
        // WSJT-X sends multicast out of the interfaces it's told to, often loopback.
        let mut joined = sock.join_multicast_v4(&g, &Ipv4Addr::UNSPECIFIED).is_ok();
        joined |= sock.join_multicast_v4(&g, &Ipv4Addr::LOCALHOST).is_ok();
        if !joined {
            anyhow::bail!("couldn't join multicast group {g}");
        }
    }
    Ok(UdpSocket::from_std(sock.into())?)
}

async fn wsjtx_listener(hub: std::sync::Weak<Hub>, cfg: Integrations, listen: String) {
    let socket = match bind_udp(&listen, &cfg.wsjtx_multicast) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            if let Some(h) = hub.upgrade() {
                h.set_wsjtx_status(&listen, format!("can't listen on {listen}: {e}"));
            }
            return;
        }
    };
    let forward: Vec<SocketAddr> = cfg.wsjtx_forward.iter().filter_map(|a| a.trim().parse().ok()).collect();
    if let Some(h) = hub.upgrade() {
        h.set_wsjtx_status(&listen, format!("listening on {listen}"));
    }
    let mut buf = vec![0u8; 65536];
    loop {
        let Ok((n, from)) = socket.recv_from(&mut buf).await else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        for to in &forward {
            let _ = socket.send_to(&buf[..n], to).await;
        }
        let Some(h) = hub.upgrade() else { return };
        relay(&h, &socket, UdpEvent::RelayWsjtx, &buf[..n]).await;
        h.lock().wsjtx_sockets.insert(from, socket.clone());
        match wsjtx::parse(&buf[..n]) {
            Ok(msg) => h.on_wsjtx(msg, from),
            Err(e) => tracing::debug!("WSJT-X packet from {from}: {e}"),
        }
    }
}

/// Passes a received packet on, unchanged, to the user's relay connections for that stream.
/// Sent from the listening socket, so programs that answer (WSJT-X's Reply) reach QRZero.
async fn relay(h: &Hub, socket: &UdpSocket, stream: UdpEvent, packet: &[u8]) {
    let targets = h.udp.relay_targets(stream);
    if targets.is_empty() {
        return;
    }
    let me = socket.local_addr().ok();
    for to in targets {
        // Never back to ourselves, or the packet goes round forever.
        if Some(to) == me || me.is_some_and(|m| m.port() == to.port() && m.ip().is_unspecified() && to.ip().is_loopback()) {
            continue;
        }
        if socket.send_to(packet, to).await.is_ok() {
            h.udp.relayed(stream, to);
        }
    }
}

async fn n1mm_listener(hub: std::sync::Weak<Hub>, listen: String) {
    let socket = match bind_udp(&listen, "") {
        Ok(s) => s,
        Err(e) => {
            if let Some(h) = hub.upgrade() {
                h.set_status("n1mm", format!("can't listen on {listen}: {e}"));
            }
            return;
        }
    };
    if let Some(h) = hub.upgrade() {
        h.set_status("n1mm", format!("listening on {listen}"));
    }
    let mut buf = vec![0u8; 65536];
    loop {
        let Ok((n, _)) = socket.recv_from(&mut buf).await else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        let Some(h) = hub.upgrade() else { return };
        relay(&h, &socket, UdpEvent::RelayN1mm, &buf[..n]).await;
        match n1mm::parse(&buf[..n]) {
            Ok(msg) => h.on_n1mm(msg),
            Err(e) => tracing::debug!("N1MM packet: {e}"),
        }
    }
}

/// Asks PstRotatorAz for the heading every couple of seconds; it answers on its port + 1.
async fn rotator_poller(hub: std::sync::Weak<Hub>, addr: String) {
    let Ok(target) = addr.parse::<SocketAddr>() else {
        if let Some(h) = hub.upgrade() {
            h.set_status("rotator", format!("{addr} isn't an address like 127.0.0.1:12000"));
        }
        return;
    };
    let reply_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), target.port() + 1);
    let listen = match UdpSocket::bind(reply_addr).await {
        Ok(s) => Some(s),
        Err(e) => {
            if let Some(h) = hub.upgrade() {
                h.set_status("rotator", format!("can't hear PstRotatorAz on {reply_addr}: {e}"));
            }
            None
        }
    };
    let Ok(send) = UdpSocket::bind("0.0.0.0:0").await else { return };
    let mut buf = [0u8; 512];
    loop {
        let _ = send.send_to(pst::query().as_bytes(), target).await;
        if let Some(l) = &listen {
            if let Ok(Ok((n, _))) = tokio::time::timeout(Duration::from_secs(2), l.recv_from(&mut buf)).await {
                if let Some(az) = pst::parse_reply(&String::from_utf8_lossy(&buf[..n])) {
                    let Some(h) = hub.upgrade() else { return };
                    let changed = h.lock().rotator_az.is_none_or(|old| (old - az).abs() >= 0.5);
                    if changed {
                        h.lock().rotator_az = Some(az);
                        h.emit(json!({"type": "rotator", "azimuth": az}));
                    }
                }
                continue;
            }
        } else {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        if hub.strong_count() == 0 {
            return;
        }
    }
}

// ---- helpers --------------------------------------------------------------

/// A radio's state as a UDP connection event. Rig channels are numbered from 1 (RadioNr in N1MM terms).
fn radio_ctx(r: &Radio) -> UdpCtx {
    let mut parts = r.key.split(':');
    let radio_nr = match (parts.next(), parts.next(), parts.next()) {
        (Some("rig"), Some(_), Some(ch)) => ch.parse::<u32>().map_or(1, |c| c + 1),
        (Some("n1mm"), Some(_), Some(nr)) => nr.parse().unwrap_or(1),
        _ => 1,
    };
    let s = &r.state;
    UdpCtx {
        event: UdpEvent::Radio,
        freq_hz: s.freq_hz,
        tx_freq_hz: s.freq_hz,
        band: band_for_freq(s.freq_hz as f64 / 1e6).unwrap_or_default().to_string(),
        mode: if s.mode.is_empty() { s.rig_mode.clone() } else { s.mode.clone() },
        rig_mode: s.rig_mode.clone(),
        tx: s.tx,
        radio: r.name.clone(),
        radio_key: r.key.clone(),
        radio_nr,
        ..UdpCtx::default()
    }
}

/// Rig control settings live in the rig's equipment fields.
pub fn rig_config(f: &Fields) -> Option<RigConfig> {
    let get = |k: &str| f.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
    let port = |default: u16| get("PORT").parse().unwrap_or(default);
    let host = || Some(get("HOST")).filter(|h| !h.is_empty()).unwrap_or_else(|| "127.0.0.1".into());
    let serial = get("SERIAL_PORT");
    let baud = get("BAUD").parse().unwrap_or(38400);
    Some(match get("CONTROL").to_ascii_lowercase().as_str() {
        "hamlib" => RigConfig::Hamlib { host: host(), port: port(4532) },
        "tci" => RigConfig::Tci { host: host(), port: port(40001) },
        "kenwood" if !serial.is_empty() => RigConfig::Kenwood { port: serial, baud },
        "yaesu" if !serial.is_empty() => RigConfig::Yaesu { port: serial, baud },
        "icom" if !serial.is_empty() => RigConfig::Icom {
            port: serial,
            baud,
            civ_addr: u8::from_str_radix(get("CIV_ADDR").trim_start_matches("0x"), 16).unwrap_or(0x94),
        },
        _ => return None,
    })
}

/// The bands an antenna is set up for: its `BANDS` field, e.g. "40m,20m,15m" (or "40 20 15").
pub fn antenna_bands(f: &Fields) -> Vec<&'static str> {
    let text = f.get("BANDS").map(String::as_str).unwrap_or_default();
    text.split(|c: char| c == ',' || c == ';' || c == '/' || c.is_whitespace())
        .filter_map(|t| qrzero_core::band::normalize_band(t).or_else(|| qrzero_core::band::normalize_band(&format!("{t}m"))))
        .collect()
}

/// Sets MY_ANTENNA on an auto-logged QSO that has none: the first antenna
/// (in tree order) whose bands include the QSO's band.
fn fill_antenna<'a>(fields: &mut Fields, gear: impl Iterator<Item = &'a qrzero_core::model::Equipment>) {
    if fields.get("MY_ANTENNA").is_some_and(|a| !a.trim().is_empty()) {
        return;
    }
    let band = fields
        .get("BAND")
        .and_then(|b| qrzero_core::band::normalize_band(b))
        .or_else(|| fields.get("FREQ").and_then(|f| f.trim().parse().ok()).and_then(band_for_freq));
    let Some(band) = band else { return };
    if let Some(ant) = gear.filter(|e| e.kind == "antenna").find(|e| antenna_bands(&e.fields).contains(&band)) {
        fields.insert("MY_ANTENNA".into(), ant.name.clone());
    }
}

/// WSJT-X marks the mode of a decode with a single character.
fn decode_mode(m: &str) -> &str {
    match m {
        "~" => "FT8",
        "+" => "FT4",
        "#" => "JT65",
        "@" => "JT9",
        ":" => "Q65",
        "`" => "FST4",
        "&" => "MSK144",
        other => other,
    }
}

fn n1mm_mode(m: &str) -> &'static str {
    match m.to_ascii_uppercase().as_str() {
        "CW" => "CW",
        "USB" | "LSB" | "SSB" => "SSB",
        "AM" => "AM",
        "FM" => "FM",
        "RTTY" => "RTTY",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_config_from_equipment_fields() {
        let f = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Fields>();
        assert_eq!(rig_config(&f(&[("CONTROL", "hamlib")])), Some(RigConfig::Hamlib { host: "127.0.0.1".into(), port: 4532 }));
        assert_eq!(rig_config(&f(&[("CONTROL", "TCI"), ("HOST", "10.0.0.5"), ("PORT", "50001")])), Some(RigConfig::Tci { host: "10.0.0.5".into(), port: 50001 }));
        assert_eq!(
            rig_config(&f(&[("CONTROL", "icom"), ("SERIAL_PORT", "COM4"), ("BAUD", "19200"), ("CIV_ADDR", "0xA4")])),
            Some(RigConfig::Icom { port: "COM4".into(), baud: 19200, civ_addr: 0xA4 })
        );
        assert_eq!(rig_config(&f(&[("CONTROL", "kenwood")])), None, "serial rigs need a port");
        assert_eq!(rig_config(&f(&[])), None);
    }

    fn rig(key: &str, name: &str, freq_hz: u64) -> Radio {
        let state = RigState { connected: true, freq_hz, ..Default::default() };
        Radio { key: key.into(), name: name.into(), source: "rig", can_tune: true, state }
    }

    #[test]
    fn slice_patterns() {
        let s = |t: &str| slice_in(t).map(|(c, l)| format!("{c} {l}"));
        assert_eq!(s("WSJT-X - Slice A").as_deref(), Some("A Slice A"));
        assert_eq!(s("Flex slice-b").as_deref(), Some("B Slice B"));
        assert_eq!(s("SliceC").as_deref(), Some("C Slice C"));
        assert_eq!(s("IC-9700 VFO B").as_deref(), Some("B VFO B"));
        assert_eq!(s("JTDX rx2").as_deref(), Some("B RX2"));
        assert_eq!(s("WSJT-X - 20m"), None);
        assert_eq!(s("Slicer"), None);
    }

    #[test]
    fn ftx_source_labels() {
        let radios = [rig("rig:1:0", "FLEX-6600 A", 14_074_000), rig("rig:1:1", "FLEX-6600 B", 7_074_000), rig("rig:2:0", "FT-991A", 7_074_500)];
        let wsjtx = Radio { source: "wsjtx", ..rig("wsjtx:X", "X", 14_074_000) };
        // A slice in the id wins, and picks the matching rig channel.
        let a = ftx_source("WSJT-X - Slice A", "", 14_074_000, &radios);
        assert_eq!((a.source.as_str(), a.slice.as_deref(), a.rig_key.as_deref()), ("Slice A · WSJT-X", Some("A"), Some("rig:1:0")));
        // Two rigs near 7.074; the configuration name says which.
        let b = ftx_source("JTDX", "Flex Slice B", 7_074_000, &radios);
        assert_eq!((b.source.as_str(), b.program, b.rig_name.as_deref()), ("Slice B · JTDX", "JTDX", Some("FLEX-6600 B")));
        let ft = ftx_source("JTDX - 40m", "FT-991A", 7_074_000, &radios);
        assert_eq!((ft.source.as_str(), ft.slice), ("FT-991A · JTDX", None));
        // Ambiguous: no rig, falls back to the id.
        let amb = ftx_source("WSJT-X", "Default", 7_074_000, &radios);
        assert_eq!((amb.source.as_str(), amb.rig_key), ("WSJT-X", None));
        // One rig channel on the frequency gives its letter; wsjtx radios never match.
        let one = ftx_source("WSJT-X", "", 14_075_000, radios.iter().chain([&wsjtx]));
        assert_eq!((one.source.as_str(), one.slice.as_deref()), ("FLEX-6600 A · WSJT-X", Some("A")));
        let none = ftx_source("WSJT-X", "Home", 21_074_000, &radios);
        assert_eq!(none.source, "Home · WSJT-X");
    }

    #[test]
    fn tx_periods() {
        let at = |h: u32, m: u32, s: f64| h * 3_600_000 + m * 60_000 + (s * 1000.0) as u32;
        assert_eq!(tx_period_start(at(12, 0, 15.2), "FT8", 15), at(12, 0, 15.0));
        assert_eq!(tx_period_start(at(12, 0, 29.0), "FT8", 15), at(12, 0, 15.0));
        // A status a moment before the boundary belongs to the period starting.
        assert_eq!(tx_period_start(at(12, 0, 29.7), "FT8", 15), at(12, 0, 30.0));
        assert_eq!(tx_period_start(at(12, 0, 8.0), "FT4", 7), at(12, 0, 7.5));
        assert_eq!(tx_period_start(at(12, 0, 8.0), "FT8", 0), at(12, 0, 0.0));
        assert_eq!(tx_period_start(at(12, 0, 59.0), "JT65", 0), at(12, 0, 0.0));
        assert_eq!(tx_period_start(at(23, 59, 59.8), "FT8", 15), 0);
        // "Not set" from WSJT-X must not overflow into a period that puts every line at 00:00:00.
        assert_eq!(tx_period_start(at(18, 50, 31.0), "FT8", u32::MAX), at(18, 50, 30.0));
    }
}
