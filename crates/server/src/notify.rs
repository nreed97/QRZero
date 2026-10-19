//! Push alerts through ntfy (<https://ntfy.sh> or a self-hosted server): needed cluster spots,
//! watch list hits and DXpeditions, sent as they happen. Nothing runs while it is off.

use std::collections::HashMap;
use std::sync::Mutex;

use qrzero_core::awards::{Award, GoalHit, HintStatus};
use serde::{Deserialize, Serialize};

use crate::cluster::SpotInfo;
use crate::watch::WatchHit;

/// Settings key for the ntfy settings (the access token is kept with the other passwords).
pub const SETTING_KEY: &str = "ntfy";
/// Secret name of the ntfy access token.
pub const TOKEN_SECRET: &str = "ntfy-token";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NtfyConfig {
    pub enabled: bool,
    /// Server address, like https://ntfy.sh.
    pub server: String,
    pub topic: String,
    /// Send needed cluster spots.
    pub needed: bool,
    /// How needed a spot must be: 0 something new only, 1 also new bands, 2 also new modes, 3 also unconfirmed.
    pub level: u8,
    /// Send watch list and DXpedition alerts.
    pub watch: bool,
    /// ntfy priority, 1 (min) to 5 (max).
    pub priority: u8,
    /// Minutes before the same call, band and mode is sent again.
    pub repeat_min: u32,
}

impl Default for NtfyConfig {
    fn default() -> Self {
        NtfyConfig { enabled: false, server: "https://ntfy.sh".into(), topic: String::new(), needed: true, level: 0, watch: true, priority: 3, repeat_min: 30 }
    }
}

/// What the last send did, for the Settings page.
#[derive(Clone, Debug, Default, Serialize)]
pub struct NtfyStatus {
    /// Unix seconds of the last attempt.
    pub at: Option<i64>,
    pub sent: u64,
    pub error: Option<String>,
}

#[derive(Default)]
struct Inner {
    config: NtfyConfig,
    token: Option<String>,
    /// When each call|band|mode was last sent (Unix seconds).
    sent: HashMap<String, i64>,
    status: NtfyStatus,
}

pub struct Notify {
    inner: Mutex<Inner>,
    http: reqwest::Client,
}

/// One message to send.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub title: String,
    pub body: String,
    pub tags: &'static str,
}

impl Notify {
    pub fn new(config: NtfyConfig) -> Self {
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().unwrap_or_default();
        Notify { inner: Mutex::new(Inner { config, ..Inner::default() }), http }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn config(&self) -> NtfyConfig {
        self.lock().config.clone()
    }

    pub fn set_config(&self, config: NtfyConfig) {
        self.lock().config = config;
    }

    pub fn set_token(&self, token: Option<String>) {
        self.lock().token = token.filter(|t| !t.trim().is_empty());
    }

    pub fn has_token(&self) -> bool {
        self.lock().token.is_some()
    }

    pub fn status(&self) -> NtfyStatus {
        self.lock().status.clone()
    }

    /// Sends a needed cluster spot, if alerts for it are on and it wasn't sent lately.
    pub fn spot(self: &std::sync::Arc<Self>, s: &SpotInfo, now: i64) {
        let msg = {
            let mut inner = self.lock();
            let c = &inner.config;
            if !c.enabled || !c.needed || c.topic.trim().is_empty() || !fresh(&s.time, now) {
                return;
            }
            let Some((rank, label)) = spot_need(s) else { return };
            if rank > c.level {
                return;
            }
            let repeat = i64::from(c.repeat_min) * 60;
            let key = format!("{}|{}|{}", s.call, s.band.as_deref().unwrap_or(""), s.mode);
            if inner.sent.get(&key).is_some_and(|&t| now - t < repeat) {
                return;
            }
            inner.sent.insert(key, now);
            if inner.sent.len() > 2000 {
                inner.sent.retain(|_, t| now - *t < repeat);
            }
            spot_message(s, &label)
        };
        self.send(msg);
    }

    /// Sends a watch list or DXpedition alert (the watch list already limits how often).
    pub fn watch(self: &std::sync::Arc<Self>, hit: &WatchHit) {
        let c = self.config();
        if !c.enabled || !c.watch || c.topic.trim().is_empty() {
            return;
        }
        self.send(watch_message(hit));
    }

    /// Sends in the background; the outcome shows in [`Notify::status`].
    pub fn send(self: &std::sync::Arc<Self>, msg: Message) {
        let me = self.clone();
        tokio::spawn(async move {
            let res = me.post(&msg).await;
            let mut inner = me.lock();
            inner.status.at = Some(chrono::Utc::now().timestamp());
            match res {
                Ok(()) => {
                    inner.status.sent += 1;
                    inner.status.error = None;
                }
                Err(e) => {
                    tracing::info!("ntfy: {e}");
                    inner.status.error = Some(e);
                }
            }
        });
    }

    /// Posts one message now.
    pub async fn post(&self, msg: &Message) -> Result<(), String> {
        let (c, token) = {
            let inner = self.lock();
            (inner.config.clone(), inner.token.clone())
        };
        let topic = c.topic.trim();
        if topic.is_empty() {
            return Err("no topic set".into());
        }
        let url = format!("{}/{}", c.server.trim().trim_end_matches('/'), topic);
        // Header values must be plain text; the body carries anything else.
        let header = |s: &str| s.chars().map(|ch| if ch.is_ascii() && !ch.is_ascii_control() { ch } else { '?' }).collect::<String>();
        let mut req = self
            .http
            .post(&url)
            .header("Title", header(&msg.title))
            .header("Priority", c.priority.clamp(1, 5).to_string())
            .header("Tags", msg.tags)
            .body(msg.body.clone());
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.map_err(|e| format!("could not reach {url}: {e}"))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let text = resp.text().await.unwrap_or_default();
        Err(format!("ntfy said HTTP {}: {}", status.as_u16(), text.trim().chars().take(200).collect::<String>()))
    }
}

/// Whether a spot's HHMM (UTC) time is within ten minutes of now: older ones are what the
/// cluster replays on connect.
fn fresh(time: &str, now: i64) -> bool {
    let (Some(h), Some(m)) = (time.get(0..2).and_then(|h| h.parse::<i64>().ok()), time.get(2..4).and_then(|m| m.parse::<i64>().ok())) else {
        return true;
    };
    let mins_now = (now / 60).rem_euclid(24 * 60);
    let age = (mins_now - (h * 60 + m)).rem_euclid(24 * 60);
    age <= 10
}

/// How needed a spot is (0 something new, 1 new band, 2 new mode, 3 unconfirmed) and what to call
/// it, from the Marathon, the award goals, or the plain flags before the awards are counted.
pub fn spot_need(s: &SpotInfo) -> Option<(u8, String)> {
    if let Some(m) = s.marathon {
        let what = match (m.country, m.zone) {
            (true, true) => "Marathon country, zone",
            (true, false) => "Marathon country",
            _ => "Marathon zone",
        };
        return Some((0, what.into()));
    }
    if let Some(goals) = &s.goals {
        return goals.first().map(|h| (hit_rank(h), hit_label(h)));
    }
    let n = s.needed?;
    if n.new_dxcc {
        Some((0, "New DXCC".into()))
    } else if n.new_band {
        Some((1, "New band".into()))
    } else if n.new_mode {
        Some((2, "New mode".into()))
    } else {
        None
    }
}

fn hit_rank(h: &GoalHit) -> u8 {
    match (h.status, h.column.as_str()) {
        (HintStatus::New, "mixed") => 0,
        (HintStatus::New, "cw" | "phone" | "digital") => 2,
        (HintStatus::New, _) => 1,
        _ => 3,
    }
}

/// Like the UI's labels: "New zone 25", "New band (WAZ)", "Unconfirmed DXCC 20m".
fn hit_label(h: &GoalHit) -> String {
    let award = match h.award {
        Award::Dxcc => "DXCC",
        Award::Waz => "WAZ",
        Award::Wpx => "WPX",
        Award::Wac => "WAC",
        Award::Itu => "ITU",
        Award::Iota => "IOTA",
        Award::Was => "WAS",
        Award::Vucc => "VUCC",
        Award::Counties => "Counties",
    };
    let of = if h.award == Award::Dxcc { String::new() } else { format!(" ({award})") };
    match hit_rank(h) {
        0 => match h.award {
            Award::Dxcc => "New DXCC".into(),
            Award::Waz => format!("New zone {}", h.key),
            Award::Wpx => format!("New prefix {}", h.key),
            Award::Wac => format!("New continent {}", h.key),
            Award::Itu => format!("New ITU zone {}", h.key),
            _ => format!("New {award} {}", h.key),
        },
        1 => format!("New band{of}"),
        2 => format!("New mode{of}"),
        _ if h.column == "mixed" => format!("Unconfirmed {award}"),
        _ => format!("Unconfirmed {award} {}", h.column),
    }
}

fn spot_message(s: &SpotInfo, label: &str) -> Message {
    let country = s.entity.as_ref().map_or("", |e| e.name.as_str());
    let mut body = format!("{} {:.1} kHz {}", s.band.as_deref().unwrap_or(""), s.freq_hz as f64 / 1000.0, s.mode).trim().to_string();
    if !country.is_empty() {
        body.push_str(&format!(", {country}"));
    }
    body.push_str(&format!("\nSpotted by {}", s.spotter));
    if !s.comment.trim().is_empty() {
        body.push_str(&format!(": {}", s.comment.trim()));
    }
    Message { title: format!("{label}: {}", s.call), body, tags: "radio" }
}

fn watch_message(h: &WatchHit) -> Message {
    let mut body = format!("{} {:.1} kHz {}", h.band.as_deref().unwrap_or(""), h.freq_hz as f64 / 1000.0, h.mode).trim().to_string();
    if let Some(c) = h.country.as_deref().filter(|c| !c.is_empty()) {
        body.push_str(&format!(", {c}"));
    }
    if !h.note.trim().is_empty() {
        body.push_str(&format!("\n{}", h.note.trim()));
    }
    if !h.detail.trim().is_empty() {
        body.push_str(&format!("\n{}", h.detail.trim()));
    }
    Message { title: format!("{}: {}", h.label, h.call), body, tags: "eyes" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replayed_spots_are_not_fresh() {
        // 12:05 UTC on some day.
        let now = 86_400 * 100 + 12 * 3600 + 5 * 60;
        assert!(fresh("1200", now));
        assert!(fresh("1205", now));
        assert!(!fresh("1150", now));
        assert!(fresh("", now), "no time: treated as new");
        // Just after midnight, a spot from 23:58 is two minutes old.
        assert!(fresh("2358", 86_400 * 101 + 60 * 3));
    }

    #[test]
    fn labels_match_the_ui() {
        let h = |award, key: &str, column: &str, status| GoalHit { award, key: key.into(), column: column.into(), status };
        assert_eq!(hit_label(&h(Award::Waz, "25", "mixed", HintStatus::New)), "New zone 25");
        assert_eq!(hit_label(&h(Award::Dxcc, "339", "40m", HintStatus::New)), "New band");
        assert_eq!(hit_label(&h(Award::Waz, "25", "cw", HintStatus::New)), "New mode (WAZ)");
        assert_eq!(hit_label(&h(Award::Dxcc, "339", "20m", HintStatus::Worked)), "Unconfirmed DXCC 20m");
        assert_eq!(hit_rank(&h(Award::Dxcc, "339", "20m", HintStatus::Worked)), 3);
    }
}
