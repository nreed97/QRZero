//! PSK Reporter feed: live reception reports of the stations the user is looking for (a
//! DXpedition, a friend), from PSK Reporter's MQTT server, grouped into one spot per call, band
//! and mode.
//!
//! Off by default. It connects only while turned on with at least one call, and subscribes to just
//! those calls, so it costs nothing otherwise. Reports are kept in memory for a set number of
//! minutes; the UI is told at most every 2 seconds that the list changed and reads it then.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use qrzero_core::band::band_for_freq;
use qrzero_core::cty::Entity;
use qrzero_core::worked::Needed;
use qrzero_radio::cluster::dial_for;
use qrzero_radio::pskreporter::{self, PskEvent, PskHandle, Report};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::station::Hub;

const SETTING_KEY: &str = "psk";
/// Reports kept at most, however short the time limit.
const MAX_REPORTS: usize = 5000;
/// Calls looked for at most.
const MAX_CALLS: usize = 20;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PskConfig {
    /// Connect to the feed (and connect when QRZero starts).
    pub enabled: bool,
    /// Transmitting stations to look for.
    pub calls: Vec<String>,
    /// How long a report is kept, in minutes.
    pub minutes: u32,
    pub host: String,
    pub port: u16,
}

impl Default for PskConfig {
    fn default() -> Self {
        PskConfig { enabled: false, calls: Vec::new(), minutes: 30, host: pskreporter::DEFAULT_HOST.into(), port: pskreporter::DEFAULT_PORT }
    }
}

/// One station on one band and mode, from its reports.
#[derive(Clone, Debug, Serialize)]
pub struct PskSpot {
    pub call: String,
    pub band: Option<String>,
    pub mode: String,
    /// The latest report's frequency.
    pub freq_hz: u64,
    /// Where to tune: the dial frequency for the digital modes, else `freq_hz`.
    pub tune_hz: u64,
    /// Unix seconds of the first and latest report kept.
    pub first: i64,
    pub last: i64,
    pub reports: u32,
    /// Different stations that heard it.
    pub receivers: u32,
    pub best_snr: Option<i32>,
    pub last_snr: Option<i32>,
    pub last_receiver: String,
    pub last_receiver_grid: String,
    /// The station's own grid, as its reports give it.
    pub grid: String,
    pub entity: Option<Entity>,
    pub needed: Option<Needed>,
}

struct Kept {
    /// Unix seconds of the reception (when it was received, if the report has none).
    time: i64,
    report: Report,
}

#[derive(Default)]
struct Inner {
    config: PskConfig,
    handle: Option<PskHandle>,
    task: Option<JoinHandle<()>>,
    state: String,
    connected: bool,
    reports: VecDeque<Kept>,
}

pub struct Psk {
    hub: Weak<Hub>,
    inner: Mutex<Inner>,
}

/// Upper-cased, trimmed and without repeats; an error names the first entry that isn't a callsign.
fn clean_calls(calls: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for c in calls.iter().map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) {
        if !pskreporter::valid_call(&c) {
            return Err(format!("{c} doesn't look like a callsign"));
        }
        if !out.contains(&c) {
            out.push(c);
        }
    }
    if out.len() > MAX_CALLS {
        return Err(format!("look for at most {MAX_CALLS} calls at a time"));
    }
    Ok(out)
}

/// A spot's call, band and mode.
type SpotKey = (String, Option<String>, String);

/// Groups the reports into spots, newest first.
fn group(reports: &VecDeque<Kept>) -> Vec<PskSpot> {
    let mut by: HashMap<SpotKey, (PskSpot, Vec<String>)> = HashMap::new();
    for k in reports {
        let r = &k.report;
        let band = band_for_freq(r.freq_hz as f64 / 1e6).map(str::to_string);
        let (s, heard_by) = by.entry((r.sender.clone(), band.clone(), r.mode.clone())).or_insert_with(|| {
            let spot = PskSpot {
                call: r.sender.clone(),
                band,
                mode: r.mode.clone(),
                freq_hz: r.freq_hz,
                tune_hz: r.freq_hz,
                first: k.time,
                last: i64::MIN,
                reports: 0,
                receivers: 0,
                best_snr: None,
                last_snr: None,
                last_receiver: String::new(),
                last_receiver_grid: String::new(),
                grid: String::new(),
                entity: None,
                needed: None,
            };
            (spot, Vec::new())
        });
        s.reports += 1;
        s.first = s.first.min(k.time);
        if k.time >= s.last {
            (s.last, s.freq_hz, s.last_snr) = (k.time, r.freq_hz, r.snr);
            (s.last_receiver, s.last_receiver_grid) = (r.receiver.clone(), r.receiver_grid.clone());
        }
        if let Some(snr) = r.snr {
            s.best_snr = Some(s.best_snr.map_or(snr, |b| b.max(snr)));
        }
        if !r.sender_grid.is_empty() {
            s.grid = r.sender_grid.clone();
        }
        if !r.receiver.is_empty() && !heard_by.contains(&r.receiver) {
            heard_by.push(r.receiver.clone());
        }
    }
    let mut spots: Vec<PskSpot> = by
        .into_values()
        .map(|(mut s, heard_by)| {
            s.receivers = heard_by.len() as u32;
            s.tune_hz = dial_for(s.freq_hz as f64 / 1000.0, &s.mode).map_or(s.freq_hz, |d| (d * 1000.0).round() as u64);
            s
        })
        .collect();
    spots.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| a.call.cmp(&b.call)));
    spots
}

impl Psk {
    pub fn new(hub: &Arc<Hub>) -> Arc<Self> {
        let config: PskConfig = hub.setting(SETTING_KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let p = Arc::new(Psk { hub: Arc::downgrade(hub), inner: Mutex::new(Inner { config, state: "off".into(), ..Inner::default() }) });
        p.apply();
        p
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Connects or disconnects to match the settings.
    fn apply(self: &Arc<Self>) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let mut inner = self.lock();
        if let Some(t) = inner.task.take() {
            t.abort();
        }
        inner.handle = None;
        inner.connected = false;
        let c = inner.config.clone();
        if !c.enabled || c.calls.is_empty() {
            inner.state = if c.enabled { "add a call to look for".into() } else { "off".into() };
        } else {
            let (handle, rx) = pskreporter::connect(c.host.clone(), c.port, c.calls.clone());
            inner.handle = Some(handle);
            inner.task = Some(tokio::spawn(follow(Arc::downgrade(self), rx)));
            inner.state = format!("connecting to {}", c.host);
        }
        drop(inner);
        self.emit_changed();
    }

    pub fn save_config(self: &Arc<Self>, mut config: PskConfig) -> Result<(), String> {
        config.calls = clean_calls(&config.calls)?;
        config.minutes = config.minutes.clamp(5, 240);
        config.host = config.host.trim().to_string();
        if config.host.is_empty() {
            config.host = pskreporter::DEFAULT_HOST.into();
        }
        if config.port == 0 {
            config.port = pskreporter::DEFAULT_PORT;
        }
        let hub = self.hub.upgrade().ok_or("shutting down")?;
        hub.set_setting(SETTING_KEY, &serde_json::to_string(&config).map_err(|e| e.to_string())?)?;
        let reconnect = {
            let mut inner = self.lock();
            let old = std::mem::replace(&mut inner.config, config.clone());
            inner.reports.retain(|k| config.calls.contains(&k.report.sender));
            old.enabled != config.enabled || old.calls != config.calls || old.host != config.host || old.port != config.port
        };
        if reconnect {
            self.apply();
        } else {
            self.emit_changed();
        }
        Ok(())
    }

    /// The settings, connection state and current spots.
    pub fn view(&self) -> Value {
        let hub = self.hub.upgrade();
        let (config, state, connected, mut spots) = {
            let mut inner = self.lock();
            let cutoff = chrono::Utc::now().timestamp() - i64::from(inner.config.minutes) * 60;
            inner.reports.retain(|k| k.time >= cutoff);
            (inner.config.clone(), inner.state.clone(), inner.connected, group(&inner.reports))
        };
        if let Some(hub) = hub {
            for s in &mut spots {
                s.entity = hub.entity(&s.call);
                s.needed = hub.needed(&s.call, s.entity.as_ref().and_then(|e| e.dxcc), s.band.as_deref(), Some(s.mode.as_str()).filter(|m| !m.is_empty()));
            }
        }
        json!({ "config": config, "state": state, "connected": connected, "spots": spots })
    }

    fn emit_changed(&self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.emit(json!({"type": "psk_changed"}));
        }
    }

    /// Takes in one event; true when it was a report (the list changed).
    fn on_event(&self, e: PskEvent) -> bool {
        let mut inner = self.lock();
        let host = inner.config.host.clone();
        let report = match e {
            PskEvent::Connecting => {
                (inner.state, inner.connected) = (format!("connecting to {host}"), false);
                None
            }
            PskEvent::Connected => {
                (inner.state, inner.connected) = (format!("connected to {host}"), true);
                None
            }
            PskEvent::Disconnected { reason } => {
                (inner.state, inner.connected) = (format!("lost {host}: {reason}"), false);
                None
            }
            PskEvent::Report(r) => Some(r),
        };
        let Some(report) = report else {
            drop(inner);
            self.emit_changed();
            return false;
        };
        let now = chrono::Utc::now().timestamp();
        let time = if report.time > 0 { report.time.min(now) } else { now };
        let cutoff = now - i64::from(inner.config.minutes) * 60;
        while inner.reports.front().is_some_and(|k| k.time < cutoff) || inner.reports.len() >= MAX_REPORTS {
            inner.reports.pop_front();
        }
        inner.reports.push_back(Kept { time, report });
        true
    }
}

/// Feeds the connection's events in, and tells the UI about new reports at most every 2 seconds.
async fn follow(psk: Weak<Psk>, mut rx: mpsc::Receiver<PskEvent>) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let mut changed = false;
    loop {
        tokio::select! {
            e = rx.recv() => {
                let (Some(e), Some(p)) = (e, psk.upgrade()) else { return };
                changed |= p.on_event(e);
            }
            _ = tick.tick(), if changed => {
                let Some(p) = psk.upgrade() else { return };
                p.emit_changed();
                changed = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(time: i64, sender: &str, receiver: &str, freq_hz: u64, mode: &str, snr: i32) -> Kept {
        let report = Report {
            sender: sender.into(),
            sender_grid: "GD18".into(),
            receiver: receiver.into(),
            receiver_grid: "FN42".into(),
            freq_hz,
            mode: mode.into(),
            snr: Some(snr),
            time,
        };
        Kept { time, report }
    }

    #[test]
    fn reports_group_by_call_band_and_mode() {
        let reports: VecDeque<Kept> = [
            kept(100, "VP8PJ", "K1ABC", 14_075_100, "FT8", -15),
            kept(130, "VP8PJ", "W2XYZ", 14_075_120, "FT8", -3),
            kept(160, "VP8PJ", "K1ABC", 14_075_110, "FT8", -9),
            kept(120, "VP8PJ", "DL1AA", 14_024_000, "CW", 12),
            kept(90, "VP8PJ", "K1ABC", 7_075_000, "FT8", -20),
        ]
        .into_iter()
        .collect();
        let spots = group(&reports);
        assert_eq!(spots.len(), 3);
        let ft8 = &spots[0];
        assert_eq!((ft8.band.as_deref(), ft8.mode.as_str(), ft8.first, ft8.last), (Some("20m"), "FT8", 100, 160));
        assert_eq!((ft8.reports, ft8.receivers, ft8.best_snr, ft8.last_snr), (3, 2, Some(-3), Some(-9)));
        assert_eq!((ft8.freq_hz, ft8.tune_hz, ft8.last_receiver.as_str()), (14_075_110, 14_074_000, "K1ABC"));
        let cw = &spots[1];
        assert_eq!((cw.mode.as_str(), cw.tune_hz), ("CW", 14_024_000), "a CW spot tunes to its own frequency");
        assert_eq!(spots[2].band.as_deref(), Some("40m"));
    }

    #[test]
    fn calls_are_cleaned() {
        assert_eq!(clean_calls(&[" vp8pj".into(), "VP8PJ".into(), "".into(), "3y0j".into()]).unwrap(), vec!["VP8PJ", "3Y0J"]);
        assert!(clean_calls(&["K1ABC/#".into()]).unwrap_err().contains("K1ABC/#"));
    }
}
