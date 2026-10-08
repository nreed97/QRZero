//! Watch list: stations, prefixes and DXCC entities the user is chasing.
//!
//! Every cluster spot and FTx decode is checked against a precompiled set (a
//! few hash lookups, no database), so alerts work with the pane closed.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use qrzero_core::awards::mode_group;
use serde::{Deserialize, Serialize};

/// Where the list is kept (a UI preference row in the settings table).
pub const SETTING_KEY: &str = "pref.watch";
/// The same station on the same band alerts at most once in this many seconds.
pub const REPEAT_SECS: i64 = 10 * 60;
const MAX_HITS: usize = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchKind {
    /// One station: also matches its portable forms (DL1ABC/P, EA8/DL1ABC).
    #[default]
    Call,
    /// A callsign prefix such as VP8 or 3Y0.
    Prefix,
    /// A DXCC entity, by its primary prefix in the country file.
    Entity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchEntry {
    /// Assigned by the server when 0.
    pub id: u64,
    pub kind: WatchKind,
    /// The call, the prefix, or the entity's primary prefix (all upper case).
    pub value: String,
    /// The entity's name, for display.
    pub name: String,
    /// Empty for any band.
    pub bands: Vec<String>,
    /// Empty for any mode; "CW", "PHONE", "DIGITAL" or a mode like "FT8".
    pub modes: Vec<String>,
    pub note: String,
    pub enabled: bool,
}

impl Default for WatchEntry {
    fn default() -> Self {
        WatchEntry { id: 0, kind: WatchKind::Call, value: String::new(), name: String::new(), bands: Vec::new(), modes: Vec::new(), note: String::new(), enabled: true }
    }
}

impl WatchEntry {
    fn label(&self) -> String {
        match self.kind {
            WatchKind::Entity if !self.name.is_empty() => self.name.clone(),
            _ => self.value.clone(),
        }
    }

    fn allows(&self, band: Option<&str>, mode: &str) -> bool {
        let band_ok = self.bands.is_empty() || band.is_some_and(|b| self.bands.iter().any(|x| x.eq_ignore_ascii_case(b)));
        let group = mode_group(mode);
        let mode_ok = self.modes.is_empty() || self.modes.iter().any(|m| (!mode.is_empty() && m.eq_ignore_ascii_case(mode)) || group.is_some_and(|g| m.eq_ignore_ascii_case(g)));
        band_ok && mode_ok
    }
}

/// A station seen on the air, as checked against the list.
#[derive(Clone, Debug, Default)]
pub struct Sighting<'a> {
    pub call: &'a str,
    /// The primary prefix of its entity, when the country file knows it.
    pub entity_prefix: Option<&'a str>,
    pub entity_name: Option<&'a str>,
    pub band: Option<&'a str>,
    pub mode: &'a str,
    pub freq_hz: u64,
    pub grid: Option<&'a str>,
    /// "cluster" or "ftx".
    pub source: &'static str,
    /// The spot comment or the decoded message.
    pub detail: &'a str,
}

/// An alert: one entry matched one station.
#[derive(Clone, Debug, Serialize)]
pub struct WatchHit {
    pub seq: u64,
    pub entry_id: u64,
    /// What was matched, e.g. "VP8" or "South Shetland Islands".
    pub label: String,
    pub note: String,
    pub call: String,
    pub freq_hz: u64,
    pub band: Option<String>,
    pub mode: String,
    pub grid: Option<String>,
    pub country: Option<String>,
    pub source: &'static str,
    pub detail: String,
    /// Unix seconds.
    pub time: i64,
}

/// Enabled entries indexed for lookup.
#[derive(Default)]
struct Compiled {
    calls: HashMap<String, Vec<usize>>,
    prefixes: HashMap<String, Vec<usize>>,
    max_prefix: usize,
    entities: HashMap<String, Vec<usize>>,
}

#[derive(Default)]
struct State {
    entries: Vec<WatchEntry>,
    compiled: Compiled,
    hits: VecDeque<WatchHit>,
    seq: u64,
    /// Last alert per (entry, call, band).
    last: HashMap<(u64, String, String), i64>,
}

#[derive(Default)]
pub struct Watch {
    state: Mutex<State>,
}

/// Portable designators that don't change where a station is.
fn is_modifier(p: &str) -> bool {
    matches!(p, "P" | "M" | "MM" | "AM" | "QRP" | "A" | "B" | "R" | "LH" | "J") || (p.len() == 1 && p.as_bytes()[0].is_ascii_digit())
}

/// The part of a call that says where it is: EA8/G4ABC and G4ABC/EA8 give EA8, G4ABC/P gives G4ABC.
fn location_part(call: &str) -> &str {
    let mut parts: Vec<&str> = call.split('/').filter(|p| !p.is_empty()).collect();
    while parts.len() > 1 && is_modifier(parts[parts.len() - 1]) {
        parts.pop();
    }
    parts.into_iter().min_by_key(|p| p.len()).unwrap_or(call)
}

fn normalize(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).map(|c| c.to_ascii_uppercase()).collect()
}

impl Watch {
    pub fn new(entries: Vec<WatchEntry>) -> Self {
        let w = Watch::default();
        w.set_entries(entries);
        w
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn entries(&self) -> Vec<WatchEntry> {
        self.lock().entries.clone()
    }

    /// Cleans up and stores a new list (giving new entries an id) and returns it.
    pub fn set_entries(&self, mut entries: Vec<WatchEntry>) -> Vec<WatchEntry> {
        let mut next = entries.iter().map(|e| e.id).max().unwrap_or(0);
        let mut seen = std::collections::HashSet::new();
        for e in &mut entries {
            if e.id == 0 || !seen.insert(e.id) {
                next += 1;
                e.id = next;
                seen.insert(e.id);
            }
            e.value = normalize(&e.value);
            e.bands.retain(|b| !b.trim().is_empty());
            e.modes = e.modes.iter().map(|m| normalize(m)).filter(|m| !m.is_empty()).collect();
            e.note = e.note.trim().to_string();
        }
        entries.retain(|e| !e.value.is_empty());
        let mut c = Compiled::default();
        for (i, e) in entries.iter().enumerate().filter(|(_, e)| e.enabled) {
            let map = match e.kind {
                WatchKind::Call => &mut c.calls,
                WatchKind::Prefix => {
                    c.max_prefix = c.max_prefix.max(e.value.len());
                    &mut c.prefixes
                }
                WatchKind::Entity => &mut c.entities,
            };
            map.entry(e.value.clone()).or_default().push(i);
        }
        let mut st = self.lock();
        st.entries = entries.clone();
        st.compiled = c;
        entries
    }

    /// The first enabled entry matching a station, if any.
    pub fn check(&self, s: &Sighting) -> Option<u64> {
        let st = self.lock();
        let c = &st.compiled;
        if c.calls.is_empty() && c.prefixes.is_empty() && c.entities.is_empty() {
            return None;
        }
        let call = normalize(s.call);
        if call.is_empty() {
            return None;
        }
        let mut candidates = Vec::new();
        if !c.calls.is_empty() {
            if let Some(v) = c.calls.get(&call) {
                candidates.extend(v);
            }
            if call.contains('/') {
                for part in call.split('/').filter(|p| !p.is_empty() && !is_modifier(p)) {
                    candidates.extend(c.calls.get(part).into_iter().flatten());
                }
            }
        }
        if !c.prefixes.is_empty() {
            let base = location_part(&call);
            for n in (1..=base.len().min(c.max_prefix)).rev().filter(|&n| base.is_char_boundary(n)) {
                candidates.extend(c.prefixes.get(&base[..n]).into_iter().flatten());
            }
        }
        if let Some(p) = s.entity_prefix {
            candidates.extend(c.entities.get(p).into_iter().flatten());
        }
        candidates.into_iter().map(|&i| &st.entries[i]).find(|e| e.allows(s.band, s.mode)).map(|e| e.id)
    }

    /// Records an alert for a matched station, unless the same entry, call and band
    /// alerted in the last ten minutes.
    pub fn record(&self, entry_id: u64, s: &Sighting, now: i64) -> Option<WatchHit> {
        let mut st = self.lock();
        let entry = st.entries.iter().find(|e| e.id == entry_id)?.clone();
        let call = normalize(s.call);
        let key = (entry_id, call.clone(), s.band.unwrap_or("").to_string());
        if st.last.get(&key).is_some_and(|t| now - t < REPEAT_SECS) {
            return None;
        }
        if st.last.len() > 2000 {
            st.last.retain(|_, t| now - *t < REPEAT_SECS);
        }
        st.last.insert(key, now);
        st.seq += 1;
        let hit = WatchHit {
            seq: st.seq,
            entry_id,
            label: entry.label(),
            note: entry.note,
            call,
            freq_hz: s.freq_hz,
            band: s.band.map(str::to_string),
            mode: s.mode.to_string(),
            grid: s.grid.map(str::to_string),
            country: s.entity_name.map(str::to_string),
            source: s.source,
            detail: s.detail.to_string(),
            time: now,
        };
        st.hits.push_back(hit.clone());
        while st.hits.len() > MAX_HITS {
            st.hits.pop_front();
        }
        Some(hit)
    }

    /// Recent alerts, newest first.
    pub fn hits(&self) -> Vec<WatchHit> {
        self.lock().hits.iter().rev().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: WatchKind, value: &str) -> WatchEntry {
        WatchEntry { kind, value: value.into(), ..WatchEntry::default() }
    }

    fn seen<'a>(call: &'a str, band: &'a str, mode: &'a str) -> Sighting<'a> {
        Sighting { call, band: Some(band), mode, source: "cluster", ..Sighting::default() }
    }

    #[test]
    fn calls_match_their_portable_forms() {
        let w = Watch::new(vec![entry(WatchKind::Call, "dl1abc")]);
        for call in ["DL1ABC", "dl1abc", "DL1ABC/P", "EA8/DL1ABC", "DL1ABC/QRP", "DL1ABC/MM"] {
            assert_eq!(w.check(&seen(call, "20m", "CW")), Some(1), "{call}");
        }
        for call in ["DL1ABCD", "DL1AB", "XDL1ABC", ""] {
            assert_eq!(w.check(&seen(call, "20m", "CW")), None, "{call}");
        }
    }

    #[test]
    fn prefixes_match_where_the_station_is() {
        let w = Watch::new(vec![entry(WatchKind::Prefix, "VP8"), entry(WatchKind::Prefix, "3Y0")]);
        assert_eq!(w.check(&seen("VP8LP", "20m", "CW")), Some(1));
        assert_eq!(w.check(&seen("VP8/G4ABC", "20m", "CW")), Some(1));
        assert_eq!(w.check(&seen("G4ABC/VP8", "20m", "CW")), Some(1));
        assert_eq!(w.check(&seen("3Y0J", "15m", "FT8")), Some(2));
        assert_eq!(w.check(&seen("3Y0J/P", "15m", "FT8")), Some(2));
        assert_eq!(w.check(&seen("G4ABC", "20m", "CW")), None);
        assert_eq!(w.check(&seen("3Y1X", "20m", "CW")), None);
        // A trailing /P is not a prefix P.
        let p = Watch::new(vec![entry(WatchKind::Prefix, "P")]);
        assert_eq!(p.check(&seen("DL1ABC/P", "20m", "CW")), None);
        assert_eq!(p.check(&seen("P29VCX", "20m", "CW")), Some(1));
    }

    #[test]
    fn entities_match_by_country_file_prefix() {
        let w = Watch::new(vec![WatchEntry { name: "Japan".into(), ..entry(WatchKind::Entity, "JA") }]);
        let mut s = seen("7J1ABC", "20m", "FT8");
        assert_eq!(w.check(&s), None, "no entity known");
        s.entity_prefix = Some("JA");
        assert_eq!(w.check(&s), Some(1));
        s.entity_prefix = Some("K");
        assert_eq!(w.check(&s), None);
    }

    #[test]
    fn band_and_mode_limits() {
        let w = Watch::new(vec![
            WatchEntry { bands: vec!["20m".into(), "17m".into()], ..entry(WatchKind::Call, "K1ABC") },
            WatchEntry { modes: vec!["cw".into()], ..entry(WatchKind::Call, "K2ABC") },
            WatchEntry { modes: vec!["DIGITAL".into()], ..entry(WatchKind::Call, "K3ABC") },
            WatchEntry { modes: vec!["PHONE".into()], bands: vec!["40m".into()], ..entry(WatchKind::Call, "K4ABC") },
        ]);
        assert_eq!(w.check(&seen("K1ABC", "17m", "SSB")), Some(1));
        assert_eq!(w.check(&seen("K1ABC", "40m", "SSB")), None);
        assert_eq!(w.check(&Sighting { band: None, ..seen("K1ABC", "", "CW") }), None, "band unknown");
        assert_eq!(w.check(&seen("K2ABC", "40m", "CW")), Some(2));
        assert_eq!(w.check(&seen("K2ABC", "40m", "FT8")), None);
        assert_eq!(w.check(&seen("K2ABC", "40m", "")), None, "mode unknown");
        assert_eq!(w.check(&seen("K3ABC", "40m", "FT4")), Some(3));
        assert_eq!(w.check(&seen("K3ABC", "40m", "CW")), None);
        assert_eq!(w.check(&seen("K4ABC", "40m", "LSB")), Some(4));
        assert_eq!(w.check(&seen("K4ABC", "20m", "SSB")), None);
    }

    #[test]
    fn disabled_entries_and_fallthrough() {
        let w = Watch::new(vec![
            WatchEntry { enabled: false, ..entry(WatchKind::Call, "VP8LP") },
            WatchEntry { bands: vec!["10m".into()], ..entry(WatchKind::Call, "VP8LP") },
            entry(WatchKind::Prefix, "VP8"),
        ]);
        assert_eq!(w.check(&seen("VP8LP", "10m", "CW")), Some(2));
        assert_eq!(w.check(&seen("VP8LP", "20m", "CW")), Some(3), "the prefix entry still covers 20m");
    }

    #[test]
    fn repeats_alert_once_per_ten_minutes() {
        let w = Watch::new(vec![entry(WatchKind::Prefix, "VP8")]);
        let s = seen("VP8LP", "20m", "CW");
        assert!(w.record(1, &s, 1000).is_some());
        assert!(w.record(1, &s, 1000 + REPEAT_SECS - 1).is_none());
        assert!(w.record(1, &seen("VP8LP", "17m", "CW"), 1100).is_some(), "another band");
        assert!(w.record(1, &seen("VP8XYZ", "20m", "CW"), 1100).is_some(), "another call");
        let again = w.record(1, &s, 1000 + REPEAT_SECS).expect("ten minutes later");
        assert_eq!(again.seq, 4);
        let hits = w.hits();
        assert_eq!(hits.len(), 4);
        assert_eq!(hits[0].seq, 4, "newest first");
        assert_eq!(hits[0].label, "VP8");
        assert!(w.record(99, &s, 5000).is_none(), "unknown entry");
    }

    #[test]
    fn ids_and_cleanup() {
        let w = Watch::new(Vec::new());
        let out = w.set_entries(vec![
            WatchEntry { id: 5, ..entry(WatchKind::Call, " k1abc ") },
            entry(WatchKind::Prefix, "vp8"),
            WatchEntry { id: 5, ..entry(WatchKind::Prefix, "3y0") },
            entry(WatchKind::Call, "  "),
        ]);
        assert_eq!(out.iter().map(|e| e.id).collect::<Vec<_>>(), [5, 6, 7]);
        assert_eq!(out[0].value, "K1ABC");
        assert_eq!(out[2].value, "3Y0");
    }

    #[test]
    fn checking_is_cheap() {
        let entries: Vec<_> = (0..500).map(|i| entry(if i % 2 == 0 { WatchKind::Call } else { WatchKind::Prefix }, &format!("K{i}AB"))).collect();
        let w = Watch::new(entries);
        let t = std::time::Instant::now();
        for i in 0..100_000 {
            let call = format!("W{}XYZ/P", i % 1000);
            w.check(&Sighting { entity_prefix: Some("K"), ..seen(&call, "20m", "CW") });
        }
        // Debug builds included; a spot needs microseconds, not milliseconds.
        assert!(t.elapsed().as_millis() < 2000, "{:?}", t.elapsed());
    }
}
