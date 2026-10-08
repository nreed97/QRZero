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
use qrzero_core::worked::{Needed, WorkedIndex};
use qrzero_core::Store;
use qrzero_radio::rig::{self, RigCommand, RigConfig, RigHandle, RigState};
use qrzero_radio::{n1mm, pst, wsjtx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

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
    pub dx_call: String,
    pub transmitting: bool,
    pub tx_enabled: bool,
    #[serde(skip)]
    addr: SocketAddr,
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

#[derive(Default)]
struct Inner {
    active: Active,
    rigs: Vec<RigConn>,
    radios: BTreeMap<String, Radio>,
    worked: Option<(i64, WorkedIndex)>,
    instances: BTreeMap<String, FtxInstance>,
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

pub struct Hub {
    store: Arc<Mutex<Store>>,
    data_dir: PathBuf,
    events: broadcast::Sender<Arc<str>>,
    inner: Mutex<Inner>,
    cty: RwLock<Option<Arc<CtyDb>>>,
}

impl Hub {
    pub fn new(store: Arc<Mutex<Store>>, data_dir: PathBuf) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        let hub = Arc::new(Hub { store, data_dir, events, inner: Mutex::default(), cty: RwLock::default() });
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
    }

    /// Records a newly logged QSO in the worked-before sets.
    pub fn note_qso(&self, log_id: i64, f: &Fields) {
        if let Some((id, idx)) = self.lock().worked.as_mut() {
            if *id == log_id {
                let mode = f.get("SUBMODE").or(f.get("MODE"));
                let dxcc = f.get("DXCC").and_then(|d| d.parse().ok());
                idx.add(f.get("CALL").map_or("", |c| c.as_str()), dxcc, f.get("BAND").map(|s| s.as_str()), mode.map(|s| s.as_str()));
            }
        }
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
            let inst = inner.instances.get(&d.instance).ok_or("that WSJT-X is no longer running")?;
            let socket = inner.wsjtx_sockets.get(&inst.addr).cloned().ok_or("WSJT-X listening is off")?;
            (socket, inst.addr, wsjtx::encode_reply(&d.raw, 0))
        };
        socket.send_to(&packet, addr).await.map_err(|e| e.to_string())?;
        Ok(())
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
            M::Status { id, dial_freq, mode, dx_call, de_call, transmitting, tx_enabled, .. } => {
                let inst = FtxInstance {
                    id: id.clone(),
                    dial_freq,
                    band: band_for_freq(dial_freq as f64 / 1e6).map(str::to_string),
                    mode: mode.clone(),
                    de_call,
                    dx_call,
                    transmitting,
                    tx_enabled,
                    addr: from,
                };
                let (changed, instances) = {
                    let mut inner = self.lock();
                    let old = inner.instances.insert(id.clone(), inst.clone());
                    let changed = old.is_none_or(|o| {
                        (o.dial_freq, &o.mode, &o.dx_call, o.transmitting, o.tx_enabled)
                            != (inst.dial_freq, &inst.mode, &inst.dx_call, inst.transmitting, inst.tx_enabled)
                    });
                    (changed, inner.instances.values().cloned().collect::<Vec<_>>())
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
            }
            M::Decode(d) => {
                if let Some(decode) = self.make_decode(d) {
                    self.emit(json!({"type": "decode", "decode": &decode}));
                    let mut inner = self.lock();
                    inner.decodes.push_back(decode);
                    while inner.decodes.len() > MAX_DECODES {
                        inner.decodes.pop_front();
                    }
                }
            }
            M::Clear { id, window } => {
                if window != 1 {
                    self.lock().decodes.retain(|d| d.instance != id);
                    self.emit(json!({"type": "ftx_clear", "instance": id}));
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
        let inst = inner.instances.get(&d.id);
        let dial = inst.map_or(0, |i| i.dial_freq);
        let band = inst.and_then(|i| i.band.clone());
        let mode = inst.map(|i| i.mode.clone()).filter(|m| !m.is_empty()).unwrap_or_else(|| decode_mode(&d.mode).to_string());
        let mine = [inst.map(|i| i.de_call.clone()).unwrap_or_default(), inner.active.station_callsign.clone()];
        let to_me = ft.to.as_ref().is_some_and(|t| mine.iter().any(|m| !m.is_empty() && m.eq_ignore_ascii_case(t)));
        let needed = match (&inner.worked, &ft.from) {
            (Some((_, idx)), Some(call)) => Some(idx.needed(call, entity.as_ref().and_then(|e| e.dxcc), band.as_deref(), Some(&mode))),
            _ => None,
        };
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
            raw: d,
        })
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
            if let Some((id, replace)) = n1mm_id {
                fields.insert("APP_QRZERO_N1MM_ID".into(), id.to_string());
                if replace {
                    if let Some(existing) = st.find_qso_by_field(log_id, "APP_QRZERO_N1MM_ID", id)? {
                        let old = st.get_qso(existing)?;
                        st.update_qso(existing, old.location_id, &fields)?;
                        return Ok(false);
                    }
                }
            }
            if st.find_duplicate(log_id, &fields)?.is_some() {
                return Ok(false);
            }
            st.insert_qso(log_id, active.location_id, &fields)?;
            Ok(true)
        });
        match result {
            Ok(added) => {
                if added {
                    self.note_qso(log_id, &fields);
                }
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
        if !enabled {
            return Err("turn on the rotator in Settings, Radios first".into());
        }
        let sock = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
        sock.send_to(pst::set_azimuth(azimuth).as_bytes(), &addr).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- country file ---------------------------------------------------

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
        h.lock().wsjtx_sockets.insert(from, socket.clone());
        match wsjtx::parse(&buf[..n]) {
            Ok(msg) => h.on_wsjtx(msg, from),
            Err(e) => tracing::debug!("WSJT-X packet from {from}: {e}"),
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
}
