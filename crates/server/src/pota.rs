//! POTA spots: the activators currently on the air, from the Parks on the Air spot list.
//!
//! Nothing runs in the background. The list is read only when a pane asks for it (the POTA
//! pane or the band map's POTA option, both about once a minute while open) and is kept for
//! half a minute, so several panes share one request. A failed fetch keeps the old list and
//! reports the error next to it.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use qrzero_core::band::band_for_freq;
use qrzero_core::cty::Entity;
use qrzero_core::worked::Needed;
use serde::Serialize;
use serde_json::{json, Value};

use crate::station::Hub;

pub const DEFAULT_URL: &str = "https://api.pota.app/spot/activator";
/// A fetch this recent is reused.
const KEEP: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize)]
pub struct PotaSpot {
    pub id: u64,
    /// Unix seconds of the spot.
    pub time: i64,
    pub call: String,
    pub freq_hz: u64,
    pub band: Option<String>,
    pub mode: String,
    /// Park reference, e.g. US-0001.
    pub reference: String,
    pub park: String,
    pub location: String,
    pub grid: Option<String>,
    pub spotter: String,
    pub comment: String,
    pub entity: Option<Entity>,
    pub needed: Option<Needed>,
    /// A park already in the log (any call, band or mode): for chasing unique parks.
    pub park_worked: bool,
    /// This call from this park on this band and mode already today (UTC): for the daily reset.
    pub worked_today: bool,
}

#[derive(Default)]
struct State {
    items: Vec<Value>,
    fetched: Option<Instant>,
    fetched_at: Option<i64>,
    error: Option<String>,
}

pub struct Pota {
    url: Mutex<String>,
    http: reqwest::Client,
    state: Mutex<State>,
    /// One fetch at a time.
    fetching: tokio::sync::Mutex<()>,
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// "2026-10-31T22:30:15" (UTC) to Unix seconds.
fn parse_time(s: &str) -> Option<i64> {
    let t = chrono::NaiveDateTime::parse_from_str(s.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(t.and_utc().timestamp())
}

/// One entry of the feed (not yet flagged), or `None` when it has no usable call or frequency.
fn parse_spot(v: &Value) -> Option<PotaSpot> {
    let call = text(v, "activator").to_uppercase();
    let khz: f64 = text(v, "frequency").parse().ok()?;
    if call.is_empty() || !(100.0..=1_000_000.0).contains(&khz) {
        return None;
    }
    let id = v.get("spotId").and_then(Value::as_u64).unwrap_or(0);
    let time = parse_time(&text(v, "spotTime")).unwrap_or(0);
    let grid = Some(text(v, "grid6")).filter(|g| !g.is_empty()).or_else(|| Some(text(v, "grid4")).filter(|g| !g.is_empty()));
    let park = text(v, "name");
    let park = if park.is_empty() { text(v, "parkName") } else { park };
    Some(PotaSpot {
        id,
        time,
        call,
        freq_hz: (khz * 1000.0).round() as u64,
        band: None,
        mode: text(v, "mode").to_uppercase(),
        reference: text(v, "reference").to_uppercase(),
        park,
        location: text(v, "locationDesc"),
        grid,
        spotter: text(v, "spotter"),
        comment: text(v, "comments"),
        entity: None,
        needed: None,
        park_worked: false,
        worked_today: false,
    })
}

/// CW and phone count as one mode each (SSB, USB and LSB are the same), any other mode on its own.
fn mode_key(mode: &str) -> String {
    match qrzero_core::awards::mode_group(mode) {
        Some(g @ ("CW" | "PHONE")) => g.to_string(),
        _ => mode.trim().to_uppercase(),
    }
}

/// A QSO's call, park, band and mode.
type DayKey = (String, String, String, String);

/// The parks in the open log, and which (call, park, band, mode) were worked since 0000z today.
fn worked_parks(hub: &Hub) -> (HashSet<String>, HashSet<DayKey>) {
    let midnight = chrono::Utc::now().date_naive().and_time(chrono::NaiveTime::MIN).and_utc().timestamp();
    let (mut parks, mut today) = (HashSet::new(), HashSet::new());
    let Some(log_id) = hub.active().log_id else { return (parks, today) };
    for q in hub.with_store(|st| st.pota_qsos(log_id)).unwrap_or_default() {
        for r in q.refs.split(',').map(|r| r.trim().to_uppercase()).filter(|r| !r.is_empty()) {
            if q.time_on >= midnight {
                today.insert((q.call.to_uppercase(), r.clone(), q.band.clone().unwrap_or_default(), mode_key(&q.mode)));
            }
            parks.insert(r);
        }
    }
    (parks, today)
}

impl Pota {
    pub fn new() -> Self {
        Pota {
            url: Mutex::new(DEFAULT_URL.to_string()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("QRZero/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            state: Mutex::new(State::default()),
            fetching: tokio::sync::Mutex::new(()),
        }
    }

    pub fn set_url(&self, url: String) {
        *self.url.lock().unwrap_or_else(|p| p.into_inner()) = url;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Reads the list unless a recent one is kept (`force` always reads it).
    pub async fn refresh(&self, force: bool) {
        let _one = self.fetching.lock().await;
        if !force && self.lock().fetched.is_some_and(|t| t.elapsed() < KEEP) {
            return;
        }
        let url = self.url.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let got: Result<Vec<Value>, String> = async {
            let r = self.http.get(&url).send().await.map_err(|_| "Couldn't reach the POTA spot list".to_string())?;
            if !r.status().is_success() {
                return Err(format!("The POTA spot list answered {}", r.status().as_u16()));
            }
            let body = r.text().await.map_err(|_| "Couldn't read the POTA spot list".to_string())?;
            serde_json::from_str::<Vec<Value>>(&body).map_err(|_| "The POTA spot list was not in the expected form".to_string())
        }
        .await;
        let mut st = self.lock();
        st.fetched = Some(Instant::now());
        match got {
            Ok(items) => {
                st.items = items;
                st.fetched_at = Some(chrono::Utc::now().timestamp());
                st.error = None;
            }
            Err(e) => st.error = Some(e),
        }
    }

    /// The spots, newest first, flagged against the open log.
    pub fn view(&self, hub: &Hub) -> Value {
        let (items, fetched_at, error) = {
            let st = self.lock();
            (st.items.clone(), st.fetched_at, st.error.clone())
        };
        let (parks, today) = worked_parks(hub);
        let mut spots: Vec<PotaSpot> = items
            .iter()
            .filter_map(parse_spot)
            .map(|mut s| {
                s.band = band_for_freq(s.freq_hz as f64 / 1e6).map(str::to_string);
                s.entity = hub.entity(&s.call);
                s.park_worked = parks.contains(&s.reference);
                s.worked_today = today.contains(&(s.call.clone(), s.reference.clone(), s.band.clone().unwrap_or_default(), mode_key(&s.mode)));
                s.needed = hub.needed(&s.call, s.entity.as_ref().and_then(|e| e.dxcc), s.band.as_deref(), Some(s.mode.as_str()).filter(|m| !m.is_empty()));
                s
            })
            .collect();
        spots.sort_by_key(|s| std::cmp::Reverse(s.time));
        json!({ "spots": spots, "fetched_at": fetched_at, "error": error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_feed_entry() {
        let v = json!({"spotId": 7, "activator": "k1abc/p", "frequency": "14244", "mode": "ssb", "reference": "us-0001",
            "spotTime": "2026-10-31T22:30:15", "name": "Acadia", "locationDesc": "US-ME", "grid4": "FN54", "spotter": "W1XYZ", "comments": "loud"});
        let s = parse_spot(&v).unwrap();
        assert_eq!((s.id, s.call.as_str(), s.freq_hz, s.mode.as_str(), s.reference.as_str()), (7, "K1ABC/P", 14_244_000, "SSB", "US-0001"));
        assert_eq!((s.park.as_str(), s.location.as_str(), s.grid.as_deref(), s.spotter.as_str(), s.comment.as_str()), ("Acadia", "US-ME", Some("FN54"), "W1XYZ", "loud"));
        assert_eq!(s.time, 1_793_485_815);
    }

    #[test]
    fn skips_entries_without_a_frequency() {
        assert!(parse_spot(&json!({"activator": "K1ABC", "frequency": "x"})).is_none());
        assert!(parse_spot(&json!({"activator": "", "frequency": "7030"})).is_none());
    }
}
