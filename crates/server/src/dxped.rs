//! DXpeditions: a calendar of active and upcoming ones (NG3K's ADXO page, plus
//! calls the user adds by hand), marked with what each would be new for.
//!
//! The page is fetched at most every six hours and kept in the log database, so
//! the list is there offline and after a restart. A failed fetch keeps the old
//! list and reports the error next to it. Spots of a listed call are matched with
//! the watch list's call matching, and a needed one raises a normal watch hit.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate, Utc};
use qrzero_core::worked::DxccProfile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::station::Hub;
use crate::watch::{Sighting, Watch, WatchEntry, WatchKind};

pub const DEFAULT_URL: &str = "https://www.ng3k.com/Misc/adxoplain.html";
const MANUAL_KEY: &str = "dxped.manual";
const CACHE_KEY: &str = "dxped.cache";

/// How long a good fetch is kept before the page is read again.
const REFRESH: Duration = Duration::from_secs(6 * 3600);
/// After a failure, try again this much later.
const RETRY: Duration = Duration::from_secs(30 * 60);
/// A manual refresh is ignored when the last fetch was this recent.
const MANUAL_MIN: Duration = Duration::from_secs(60);
/// Expeditions starting later than this many days from now are left out.
const LOOKAHEAD_DAYS: i64 = 60;
/// The bands "new band" is judged on.
const NEED_BANDS: &[&str] = &["160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m"];
const MODE_GROUPS: &[&str] = &["CW", "PHONE", "DIGITAL"];

/// One planned operation: from the calendar or added by hand.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Planned {
    /// Assigned by the server for hand-added ones.
    pub id: u64,
    pub call: String,
    /// "YYYY-MM-DD"; empty for no date (always listed).
    pub start: String,
    pub end: String,
    pub note: String,
    /// The entity's name as the calendar gives it (the country file is tried first).
    pub entity: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    fetched_at: i64,
    items: Vec<Planned>,
}

/// The last spot of an expedition's call.
#[derive(Clone, Debug, Serialize)]
pub struct SpotMemo {
    pub call: String,
    pub freq_hz: u64,
    pub band: Option<String>,
    pub mode: String,
    pub comment: String,
    pub grid: Option<String>,
    /// Unix seconds.
    pub time: i64,
}

/// What an operation would be new for.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Need {
    /// The entity or the log isn't known, so nothing can be said.
    pub unknown: bool,
    pub new_dxcc: bool,
    pub bands: Vec<&'static str>,
    pub modes: Vec<&'static str>,
}

impl Need {
    pub fn any(&self) -> bool {
        self.new_dxcc || !self.bands.is_empty() || !self.modes.is_empty()
    }

    fn of(dxcc: Option<u32>, profile: Option<DxccProfile>) -> Need {
        let (Some(_), Some(p)) = (dxcc, profile) else { return Need { unknown: true, ..Need::default() } };
        if !p.worked {
            return Need { new_dxcc: true, ..Need::default() };
        }
        Need {
            unknown: false,
            new_dxcc: false,
            bands: NEED_BANDS.iter().copied().filter(|b| !p.bands.iter().any(|x| x == b)).collect(),
            modes: MODE_GROUPS.iter().copied().filter(|g| !p.mode_groups.contains(g)).collect(),
        }
    }

    /// Why a spot on this band and mode is wanted, if it is.
    fn why(&self, band: Option<&str>, mode: &str) -> Option<String> {
        if self.new_dxcc {
            return Some("New DXCC".into());
        }
        if let Some(b) = band.filter(|b| self.bands.contains(b)) {
            return Some(format!("New band {b}"));
        }
        let g = qrzero_core::awards::mode_group(mode).filter(|g| self.modes.contains(g))?;
        Some(format!("New mode {}", if g == "PHONE" { "Phone" } else if g == "CW" { "CW" } else { "Digital" }))
    }
}

#[derive(Default)]
struct State {
    feed: Vec<Planned>,
    manual: Vec<Planned>,
    fetched_at: Option<i64>,
    error: Option<String>,
    last_attempt: Option<Instant>,
    /// Matches spots to the listed calls; ids are positions in `matched` + 1.
    matcher: Option<Watch>,
    matched: Vec<Planned>,
    built_on: Option<NaiveDate>,
    spots: HashMap<String, SpotMemo>,
}

pub struct Dxped {
    url: Mutex<String>,
    http: reqwest::Client,
    state: Mutex<State>,
    /// One fetch at a time.
    fetching: tokio::sync::Mutex<()>,
}

fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

fn norm(call: &str) -> String {
    call.chars().filter(|c| !c.is_whitespace()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Whether a planned operation is on the air today (no dates means always).
fn is_active(p: &Planned, today: NaiveDate) -> bool {
    date(&p.start).is_none_or(|d| d <= today) && date(&p.end).is_none_or(|d| d >= today)
}

fn is_listed(p: &Planned, today: NaiveDate) -> bool {
    date(&p.end).is_none_or(|d| d >= today) && date(&p.start).is_none_or(|d| (d - today).num_days() <= LOOKAHEAD_DAYS)
}

impl Dxped {
    pub fn new(manual: Vec<Planned>, cache: Option<String>) -> Self {
        let cache: Cache = cache.and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default();
        let d = Dxped {
            url: Mutex::new(DEFAULT_URL.to_string()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent(concat!("QRZero/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            state: Mutex::new(State {
                fetched_at: (cache.fetched_at > 0).then_some(cache.fetched_at),
                feed: cache.items,
                ..State::default()
            }),
            fetching: tokio::sync::Mutex::new(()),
        };
        d.set_manual(manual);
        d
    }

    pub fn set_url(&self, url: String) {
        *self.url.lock().unwrap_or_else(|p| p.into_inner()) = url;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Cleans up and stores the hand-added list (giving new ones an id).
    pub fn set_manual(&self, mut list: Vec<Planned>) -> Vec<Planned> {
        let mut next = list.iter().map(|p| p.id).max().unwrap_or(0);
        let mut seen = std::collections::HashSet::new();
        for p in &mut list {
            if p.id == 0 || !seen.insert(p.id) {
                next += 1;
                p.id = next;
                seen.insert(p.id);
            }
            p.call = norm(&p.call);
            p.note = p.note.trim().to_string();
            for d in [&mut p.start, &mut p.end] {
                *d = date(d).map(|d| d.to_string()).unwrap_or_default();
            }
        }
        list.retain(|p| !p.call.is_empty());
        let mut st = self.lock();
        st.manual = list.clone();
        st.built_on = None;
        list
    }

    /// Wait before the next background refresh, and whether to fetch now.
    fn due(&self, force: bool) -> Option<Duration> {
        let st = self.lock();
        let Some(at) = st.last_attempt else {
            // Not tried since start: use the saved copy while it is fresh.
            let age = st.fetched_at.map(|t| Utc::now().timestamp() - t).filter(|a| *a >= 0 && st.error.is_none());
            return match age {
                Some(a) if !force && (a as u64) < REFRESH.as_secs() => Some(REFRESH - Duration::from_secs(a as u64)),
                _ => None,
            };
        };
        let age = at.elapsed();
        let wait = if force {
            MANUAL_MIN
        } else if st.error.is_some() || st.feed.is_empty() {
            RETRY
        } else {
            REFRESH
        };
        (age < wait).then(|| wait - age)
    }

    async fn fetch(&self, today: NaiveDate) -> Result<Vec<Planned>, String> {
        let url = self.url.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let resp = self.http.get(&url).send().await.map_err(|e| format!("could not reach the DXpedition calendar: {}", describe(&e)))?;
        if !resp.status().is_success() {
            return Err(format!("the DXpedition calendar answered {}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| format!("DXpedition calendar: {}", describe(&e)))?;
        let items = parse_adxo(&text, today);
        if items.is_empty() {
            return Err("the DXpedition calendar had no entries QRZero could read".into());
        }
        Ok(items)
    }

    /// Rebuilds the call matcher for today's expeditions when the list or the day changed.
    fn matcher<R>(&self, today: NaiveDate, f: impl FnOnce(&Watch, &[Planned]) -> R) -> R {
        let mut st = self.lock();
        if st.built_on != Some(today) || st.matcher.is_none() {
            let slack = chrono::Duration::days(1);
            let mut matched: Vec<Planned> = st
                .feed
                .iter()
                .chain(st.manual.iter())
                .filter(|p| is_active(p, today - slack) || is_active(p, today + slack))
                .cloned()
                .collect();
            matched.dedup_by(|a, b| a.call == b.call && a.start == b.start);
            let entries = matched
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    // The calendar sometimes gives only a prefix (TF, 7P8) before the call is known.
                    let kind = if p.call.len() <= 3 { WatchKind::Prefix } else { WatchKind::Call };
                    WatchEntry { id: i as u64 + 1, kind, value: p.call.clone(), ..WatchEntry::default() }
                })
                .collect();
            st.matcher = Some(Watch::new(entries));
            st.matched = matched;
            st.built_on = Some(today);
        }
        f(st.matcher.as_ref().expect("built above"), &st.matched)
    }

    /// Checks a cluster spot against the listed calls. Remembers the spot for the pane
    /// and returns the alert text when the station would be new (a DXCC, band or mode).
    pub fn on_spot(&self, hub: &Hub, s: &Sighting, dxcc: Option<u32>, now: i64) -> Option<(String, String)> {
        let today = chrono::DateTime::from_timestamp(now, 0)?.date_naive();
        let planned = self.matcher(today, |m, list| m.check(s).and_then(|id| list.get(id as usize - 1).cloned()))?;
        self.lock().spots.insert(
            planned.call.clone(),
            SpotMemo {
                call: norm(s.call),
                freq_hz: s.freq_hz,
                band: s.band.map(str::to_string),
                mode: s.mode.to_string(),
                comment: s.detail.to_string(),
                grid: s.grid.map(str::to_string),
                time: now,
            },
        );
        let need = Need::of(dxcc, dxcc.and_then(|d| hub.dxcc_profile(d)));
        let why = need.why(s.band, s.mode)?;
        Some((format!("DXpedition: {why}"), planned.note))
    }

    /// The list for the pane.
    pub fn view(&self, hub: &Hub, today: NaiveDate) -> Value {
        let (mut items, fetched_at, error, spots) = {
            let st = self.lock();
            let mut items: Vec<(Planned, bool)> = st.feed.iter().map(|p| (p.clone(), false)).collect();
            items.extend(st.manual.iter().map(|p| (p.clone(), true)));
            (items, st.fetched_at, st.error.clone(), st.spots.clone())
        };
        items.retain(|(p, _)| is_listed(p, today));
        items.sort_by(|(a, _), (b, _)| (date(&a.start), &a.call).cmp(&(date(&b.start), &b.call)));
        let rows: Vec<Value> = items
            .into_iter()
            .map(|(p, manual)| {
                let entity = hub.entity(&p.call);
                let dxcc = entity.as_ref().and_then(|e| e.dxcc);
                let need = Need::of(dxcc, dxcc.and_then(|d| hub.dxcc_profile(d)));
                json!({
                    "id": p.id,
                    "manual": manual,
                    "call": p.call,
                    "start": p.start,
                    "end": p.end,
                    "note": p.note,
                    "active": is_active(&p, today),
                    "entity": entity.as_ref().map(|e| e.name.clone()).or_else(|| Some(p.entity.clone()).filter(|n| !n.is_empty())),
                    "prefix": entity.as_ref().map(|e| e.prefix.clone()),
                    "dxcc": dxcc,
                    "need": need,
                    "needed": need.any(),
                    "spot": spots.get(&p.call),
                })
            })
            .collect();
        json!({
            "items": rows,
            "fetched_at": fetched_at,
            "error": error,
            "url": self.url.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        })
    }
}

fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".into()
    } else if e.is_connect() {
        "no connection".into()
    } else {
        e.to_string()
    }
}

/// Reads the calendar if it is due (or `force`), keeps the result, and returns how
/// long until the next background refresh.
pub async fn refresh(hub: &Hub, force: bool) -> Duration {
    let d = &hub.dxped;
    if let Some(wait) = d.due(force) {
        return wait;
    }
    let _one = d.fetching.lock().await;
    // Someone else may have fetched while we waited.
    if let Some(wait) = d.due(force) {
        return wait;
    }
    let today = Utc::now().date_naive();
    let result = d.fetch(today).await;
    let cache = {
        let mut st = d.lock();
        st.last_attempt = Some(Instant::now());
        match result {
            Ok(items) => {
                st.feed = items;
                st.fetched_at = Some(Utc::now().timestamp());
                st.error = None;
                st.built_on = None;
                serde_json::to_string(&Cache { fetched_at: st.fetched_at.unwrap_or(0), items: st.feed.clone() }).ok()
            }
            Err(e) => {
                tracing::warn!("DXpedition calendar: {e}");
                st.error = Some(e);
                None
            }
        }
    };
    if let Some(text) = cache {
        if let Err(e) = hub.set_setting(CACHE_KEY, &text) {
            tracing::warn!("saving the DXpedition calendar: {e}");
        }
    }
    if d.lock().error.is_some() {
        RETRY
    } else {
        REFRESH
    }
}

pub fn save_manual(hub: &Hub, list: Vec<Planned>) -> Result<Vec<Planned>, String> {
    let saved = hub.dxped.set_manual(list);
    hub.set_setting(MANUAL_KEY, &serde_json::to_string(&saved).map_err(|e| e.to_string())?)?;
    Ok(saved)
}

/// The saved hand-added list and calendar copy, for starting up.
pub fn saved(hub_get: impl Fn(&str) -> Option<String>) -> Dxped {
    let manual = hub_get(MANUAL_KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    Dxped::new(manual, hub_get(CACHE_KEY))
}

// ---- reading the calendar ----------------------------------------------------

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

fn month(tok: &str) -> Option<u32> {
    let t = tok.to_ascii_lowercase();
    (t.len() >= 3 && t.chars().all(|c| c.is_ascii_alphabetic()))
        .then(|| MONTHS.iter().position(|m| t.starts_with(m)).map(|i| i as u32 + 1))
        .flatten()
}

fn day(tok: &str) -> Option<u32> {
    tok.parse::<u32>().ok().filter(|d| (1..=31).contains(d))
}

fn year(tok: &str) -> Option<i32> {
    tok.parse::<i32>().ok().filter(|y| (2000..=2100).contains(y))
}

/// What a call on the calendar can be: a full call (V51WH), a bare prefix (TF, 7P8) or a
/// call with a slash.
fn call_like(t: &str) -> bool {
    (2..=14).contains(&t.len()) && t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '/') && t.chars().any(|c| c.is_ascii_uppercase())
}

/// The page is HTML: one paragraph per operation, fields separated by <br>. Turn that
/// into lines of plain text, one per field.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag.trim_start_matches('/').split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("").to_ascii_lowercase();
                match name.as_str() {
                    // A paragraph break is marked, so one operation's fields never run into the next.
                    "p" | "tr" | "li" | "div" | "h1" | "h2" | "h3" => out.push_str("\n\u{1}\n"),
                    "br" => out.push('\n'),
                    _ => out.push(' '),
                }
            }
            _ if in_tag => tag.push(c),
            _ => out.push(c),
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
}

/// Reads NG3K's ADXO page (adxoplain.html). Each operation is a paragraph:
///
/// ```text
/// Oct 1-11, 2026
/// DXCC: Lesotho
/// Callsign: 7P8
/// QSL: As Directed
/// Info: By ZS6LZ; HF; SOTA And POTA
/// ```
///
/// The date line is "Aug 25-Oct 10, 2026", "Oct 1-11, 2026" or "Dec 15, 2026-Jan 3, 2027".
/// Paragraphs without a date line and a call are skipped (month and year headings, links).
pub fn parse_adxo(text: &str, today: NaiveDate) -> Vec<Planned> {
    let mut out: Vec<Planned> = Vec::new();
    let mut cur: Option<(NaiveDate, NaiveDate)> = None;
    let (mut call, mut entity, mut info) = (String::new(), String::new(), String::new());
    let mut flush = |cur: &mut Option<(NaiveDate, NaiveDate)>, call: &mut String, entity: &mut String, info: &mut String| {
        if let Some((s, e)) = cur.take() {
            let c = std::mem::take(call);
            if call_like(&c) && !out.iter().any(|o| o.call == c && o.start == s.to_string()) {
                out.push(Planned { call: c, start: s.to_string(), end: e.to_string(), entity: std::mem::take(entity), note: info.chars().take(200).collect(), ..Planned::default() });
            }
        }
        call.clear();
        entity.clear();
        info.clear();
    };
    for line in plain(text).lines() {
        if line.starts_with('\u{1}') {
            flush(&mut cur, &mut call, &mut entity, &mut info);
            continue;
        }
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if let Some(v) = line.strip_prefix("DXCC:") {
            entity = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Callsign:") {
            // A new-entry marker or a link's text can follow the call.
            call = v.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        } else if let Some(v) = line.strip_prefix("Info:") {
            info = v.trim().to_string();
        } else if let Some(range) = parse_dates(&line, today) {
            flush(&mut cur, &mut call, &mut entity, &mut info);
            cur = Some(range);
        }
    }
    flush(&mut cur, &mut call, &mut entity, &mut info);
    out
}

/// "Aug 25-Oct 10, 2026", "Oct 1-11", "Dec 15, 2026-Jan 3, 2027": the whole line is the range.
fn parse_dates(line: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let spaced = line.replace([',', '\u{a0}'], " ").replace(['-', '\u{2013}', '\u{2014}'], " - ");
    let t: Vec<&str> = spaced.split_whitespace().collect();
    let sm = month(t.first()?)?;
    let sd = day(t.get(1)?)?;
    let mut i = 2;
    let mut sy = None;
    if let Some(y) = t.get(i).and_then(|x| year(x)) {
        sy = Some(y);
        i += 1;
    }
    if *t.get(i)? != "-" {
        return None;
    }
    i += 1;
    let (em, ed) = if let Some(m) = t.get(i).and_then(|x| month(x)) {
        let d = day(t.get(i + 1)?)?;
        i += 2;
        (m, d)
    } else {
        let d = day(t.get(i)?)?;
        i += 1;
        (sm, d)
    };
    let mut ey = None;
    if let Some(y) = t.get(i).and_then(|x| year(x)) {
        ey = Some(y);
        i += 1;
    }
    if i != t.len() {
        return None;
    }
    let (start, end) = match (sy, ey) {
        (Some(sy), Some(ey)) => (NaiveDate::from_ymd_opt(sy, sm, sd)?, NaiveDate::from_ymd_opt(ey, em, ed)?),
        (Some(sy), None) => (NaiveDate::from_ymd_opt(sy, sm, sd)?, NaiveDate::from_ymd_opt(sy + i32::from(em < sm), em, ed)?),
        (None, Some(ey)) => (NaiveDate::from_ymd_opt(ey - i32::from(em < sm), sm, sd)?, NaiveDate::from_ymd_opt(ey, em, ed)?),
        (None, None) => {
            // No year given: take the reading that puts the start nearest today.
            let wraps = i32::from(em < sm);
            (today.year() - 1..=today.year() + 1)
                .filter_map(|y| Some((NaiveDate::from_ymd_opt(y, sm, sd)?, NaiveDate::from_ymd_opt(y + wraps, em, ed)?)))
                .min_by_key(|(s, _)| (*s - today).num_days().abs())?
        }
    };
    (end >= start).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    const SAMPLE: &str = include_str!("dxped_adxo_sample.html");

    #[test]
    fn reads_the_real_page_layout() {
        let list = parse_adxo(SAMPLE, d(2026, 10, 9));
        let got: Vec<_> = list.iter().map(|p| (p.call.as_str(), p.start.as_str(), p.end.as_str(), p.entity.as_str())).collect();
        assert_eq!(
            got,
            [
                ("V51WH", "2026-08-25", "2026-10-10", "Namibia"),
                ("8Q7JH", "2026-09-20", "2026-10-10", "Maldives"),
                ("FW1P", "2026-09-26", "2026-10-17", "Wallis & Futuna"),
                ("7P8", "2026-10-01", "2026-10-11", "Lesotho"),
                ("SV5", "2026-10-03", "2026-10-10", "Dodecanese"),
                ("TF", "2026-10-07", "2026-10-17", "Iceland"),
                ("VP2MAW", "2026-10-10", "2026-10-20", "Montserrat"),
                ("3D2HM", "2026-10-10", "2026-10-22", "Fiji"),
                ("6W", "2026-10-28", "2026-11-09", "Senegal"),
                ("VP9", "2026-10-17", "2026-10-20", "Bermuda"),
                ("VK9XY", "2026-11-16", "2026-12-04", "Christmas I"),
            ]
        );
        assert!(list[0].note.starts_with("By DK2WH fm nr Omaruru; 160-6m"), "{}", list[0].note);
    }

    #[test]
    fn reads_the_date_forms() {
        let para = |dates: &str| format!("<p>\n{dates}\n<br>DXCC: X<br>Callsign: <strong>K1ABC</strong><br>Info: i\n</p>");
        let one = |dates: &str| parse_adxo(&para(dates), d(2027, 1, 5)).into_iter().map(|p| (p.start, p.end)).collect::<Vec<_>>();
        assert_eq!(one("Dec 15, 2026-Jan 3, 2027"), [("2026-12-15".to_string(), "2027-01-03".to_string())]);
        assert_eq!(one("Dec 28-Jan 10"), [("2026-12-28".to_string(), "2027-01-10".to_string())]);
        assert_eq!(one("Oct 1-11, 2026"), [("2026-10-01".to_string(), "2026-10-11".to_string())]);
        assert!(one("Oct 20-Oct 10, 2026").is_empty());
        assert!(one("2026").is_empty());
    }

    #[test]
    fn skips_paragraphs_without_dates_or_a_call() {
        let page = "<p>\n<strong>October</strong>\n</p>\n<p>\nOct 1-11, 2026\n<br>DXCC: Lesotho<br>Info: no call\n</p>\n<p>\n<br>Callsign: K1ABC\n</p>";
        assert!(parse_adxo(page, d(2026, 10, 9)).is_empty());
    }

    #[test]
    fn listing_window() {
        let p = |s: &str, e: &str| Planned { start: s.into(), end: e.into(), call: "K1ABC".into(), ..Planned::default() };
        let today = d(2026, 10, 8);
        assert!(is_listed(&p("2026-10-01", "2026-10-08"), today));
        assert!(!is_listed(&p("2026-09-01", "2026-10-07"), today));
        assert!(is_listed(&p("2026-12-01", "2026-12-10"), today));
        assert!(!is_listed(&p("2027-02-01", "2027-02-10"), today));
        assert!(is_listed(&p("", ""), today));
        assert!(is_active(&p("2026-10-08", "2026-10-09"), today));
        assert!(!is_active(&p("2026-10-09", "2026-10-12"), today));
    }

    #[test]
    fn needs() {
        let new = Need::of(Some(1), Some(DxccProfile::default()));
        assert!(new.new_dxcc);
        assert_eq!(new.why(Some("20m"), "CW").as_deref(), Some("New DXCC"));
        let worked = DxccProfile { worked: true, bands: vec!["20m".into(), "40m".into()], mode_groups: vec!["CW", "DIGITAL"] };
        let n = Need::of(Some(1), Some(worked));
        assert!(n.any() && !n.new_dxcc);
        assert_eq!(n.why(Some("20m"), "CW"), None);
        assert_eq!(n.why(Some("17m"), "CW").as_deref(), Some("New band 17m"));
        assert_eq!(n.why(Some("20m"), "SSB").as_deref(), Some("New mode Phone"));
        assert_eq!(n.why(Some("20m"), ""), None);
        assert!(Need::of(None, None).unknown);
    }

    #[test]
    fn hand_added_entries_are_cleaned() {
        let x = Dxped::new(Vec::new(), None);
        let out = x.set_manual(vec![
            Planned { call: " vp8lp ".into(), start: "2026-10-01".into(), end: "bad".into(), ..Planned::default() },
            Planned { call: "  ".into(), ..Planned::default() },
            Planned { call: "3y0k".into(), ..Planned::default() },
        ]);
        assert_eq!(out.iter().map(|p| (p.id, p.call.as_str(), p.end.as_str())).collect::<Vec<_>>(), [(1, "VP8LP", ""), (3, "3Y0K", "")]);
    }
}
