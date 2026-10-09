//! Contests: a calendar of the ones on the air now and coming up, read from the
//! WA7BNM Contest Calendar's RSS feed. It is a calendar only; logging a contest
//! happens in a contest logger, not here.
//!
//! The feed is fetched at most every six hours and kept in the log database, so the
//! list is there offline and after a restart. A failed fetch keeps the old list and
//! reports the error next to it.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::station::Hub;

pub const DEFAULT_URL: &str = "https://contestcalendar.com/calendar.rss";
const CACHE_KEY: &str = "contests.cache";

/// How long a good fetch is kept before the feed is read again.
const REFRESH: Duration = Duration::from_secs(6 * 3600);
/// After a failure, try again this much later.
const RETRY: Duration = Duration::from_secs(30 * 60);
/// A manual refresh is ignored when the last fetch was this recent.
const MANUAL_MIN: Duration = Duration::from_secs(60);
/// Contests starting later than this many days from now are left out.
const LOOKAHEAD_DAYS: i64 = 60;

/// One contest from the feed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contest {
    pub title: String,
    /// Unix seconds (UTC) of the first and last moment on the air.
    pub start: i64,
    pub end: i64,
    /// "CW", "PHONE" and/or "DIGITAL"; empty when the title doesn't say (mixed or all modes).
    pub modes: Vec<String>,
    /// The calendar's page for the contest, which links to the rules.
    pub link: String,
    /// The dates as the calendar words them, e.g. "1200Z, Oct 10 to 1200Z, Oct 11".
    pub when: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    fetched_at: i64,
    items: Vec<Contest>,
}

#[derive(Default)]
struct State {
    items: Vec<Contest>,
    fetched_at: Option<i64>,
    error: Option<String>,
    last_attempt: Option<Instant>,
}

pub struct Contests {
    url: Mutex<String>,
    http: reqwest::Client,
    state: Mutex<State>,
    /// One fetch at a time.
    fetching: tokio::sync::Mutex<()>,
}

impl Contests {
    pub fn new(cache: Option<String>) -> Self {
        let cache: Cache = cache.and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default();
        Contests {
            url: Mutex::new(DEFAULT_URL.to_string()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent(concat!("QRZero/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            state: Mutex::new(State { fetched_at: (cache.fetched_at > 0).then_some(cache.fetched_at), items: cache.items, ..State::default() }),
            fetching: tokio::sync::Mutex::new(()),
        }
    }

    pub fn set_url(&self, url: String) {
        *self.url.lock().unwrap_or_else(|p| p.into_inner()) = url;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Wait before the next background refresh, or None when it should be fetched now.
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
        } else if st.error.is_some() || st.items.is_empty() {
            RETRY
        } else {
            REFRESH
        };
        (age < wait).then(|| wait - age)
    }

    async fn fetch(&self, now: i64) -> Result<Vec<Contest>, String> {
        let url = self.url.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let resp = self.http.get(&url).send().await.map_err(|e| format!("could not reach the contest calendar: {}", describe(&e)))?;
        if !resp.status().is_success() {
            return Err(format!("the contest calendar answered {}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| format!("contest calendar: {}", describe(&e)))?;
        let items = parse_rss(&text, now);
        if items.is_empty() {
            return Err("the contest calendar had no entries QRZero could read".into());
        }
        Ok(items)
    }

    /// The pane's lists: what is on now and what starts in the next two months.
    pub fn view(&self, now: i64) -> Value {
        let (mut items, fetched_at, error) = {
            let st = self.lock();
            (st.items.clone(), st.fetched_at, st.error.clone())
        };
        let horizon = now + LOOKAHEAD_DAYS * 86_400;
        items.retain(|c| c.end > now && c.start <= horizon);
        items.sort_by(|a, b| (a.start, &a.title).cmp(&(b.start, &b.title)));
        let rows: Vec<Value> = items
            .into_iter()
            .map(|c| json!({ "title": c.title, "start": c.start, "end": c.end, "active": c.start <= now, "modes": c.modes, "link": c.link, "when": c.when }))
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
    let c = &hub.contests;
    if let Some(wait) = c.due(force) {
        return wait;
    }
    let _one = c.fetching.lock().await;
    // Someone else may have fetched while we waited.
    if let Some(wait) = c.due(force) {
        return wait;
    }
    let result = c.fetch(Utc::now().timestamp()).await;
    let cache = {
        let mut st = c.lock();
        st.last_attempt = Some(Instant::now());
        match result {
            Ok(items) => {
                st.items = items;
                st.fetched_at = Some(Utc::now().timestamp());
                st.error = None;
                serde_json::to_string(&Cache { fetched_at: st.fetched_at.unwrap_or(0), items: st.items.clone() }).ok()
            }
            Err(e) => {
                tracing::warn!("contest calendar: {e}");
                st.error = Some(e);
                None
            }
        }
    };
    if let Some(text) = cache {
        if let Err(e) = hub.set_setting(CACHE_KEY, &text) {
            tracing::warn!("saving the contest calendar: {e}");
        }
    }
    if c.lock().error.is_some() {
        RETRY
    } else {
        REFRESH
    }
}

/// The saved copy of the calendar, for starting up.
pub fn saved(hub_get: impl Fn(&str) -> Option<String>) -> Contests {
    Contests::new(hub_get(CACHE_KEY))
}

// ---- reading the feed --------------------------------------------------------

/// The text of the first `<tag>` in `s`, with CDATA, entities and stray markup removed.
fn tag(s: &str, name: &str) -> Option<String> {
    let open = s.find(&format!("<{name}>")).or_else(|| s.find(&format!("<{name} ")).and_then(|i| s[i..].find('>').map(|j| i + j - name.len() - 1)))?;
    let body_start = s[open..].find('>')? + open + 1;
    let body_end = s[body_start..].find(&format!("</{name}>"))? + body_start;
    Some(clean(&s[body_start..body_end]))
}

fn clean(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")).unwrap_or(raw);
    // Decode the usual entities (&amp; last so "&amp;lt;" stays text), then drop any markup they revealed.
    let decoded = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&nbsp;", " ").replace("&amp;", "&");
    let mut out = String::with_capacity(decoded.len());
    let mut in_tag = false;
    for c in decoded.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Contests in the feed, with the ones that ended long ago left out. `now` picks the year for dates the feed gives without one.
pub fn parse_rss(text: &str, now: i64) -> Vec<Contest> {
    let today = Utc.timestamp_opt(now, 0).single().map(|d| d.date_naive()).unwrap_or_default();
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<item") {
        let after = &rest[i..];
        let Some(end) = after.find("</item>") else { break };
        let item = &after[..end];
        rest = &after[end + 7..];
        let Some(title) = tag(item, "title").filter(|t| !t.is_empty()) else { continue };
        let when = tag(item, "description").unwrap_or_default();
        let Some((start, end)) = parse_when(&when, today).or_else(|| parse_when(&title, today)) else { continue };
        out.push(Contest { modes: modes_of(&title), link: tag(item, "link").unwrap_or_default(), title, start, end, when });
    }
    out
}

fn month(tok: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let t = tok.to_ascii_lowercase();
    (t.len() >= 3 && t.chars().all(|c| c.is_ascii_alphabetic())).then(|| MONTHS.iter().position(|m| t.starts_with(m)).map(|i| i as u32 + 1)).flatten()
}

/// "1200Z" as minutes after midnight.
fn time(tok: &str) -> Option<i64> {
    let t = tok.strip_suffix(['Z', 'z'])?;
    if t.len() != 4 || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (h, m): (i64, i64) = (t[..2].parse().ok()?, t[2..].parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

enum Tok {
    Time(i64),
    Date(u32, u32),
}

/// Start and end (unix seconds) of a calendar's date text. It copes with the shapes the
/// calendar uses: "1200Z, Oct 10 to 1200Z, Oct 11", "0000Z-2359Z, Oct 10" and several
/// such parts joined with "and". The earliest start and the latest end are returned.
fn parse_when(text: &str, today: NaiveDate) -> Option<(i64, i64)> {
    let words: Vec<&str> = text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|w| !w.is_empty()).collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        // "0000Z-0159Z" is two times.
        if let Some((a, b)) = w.split_once('-').and_then(|(a, b)| Some((time(a)?, time(b)?))) {
            toks.push(Tok::Time(a));
            toks.push(Tok::Time(b));
        } else if let Some(t) = time(w) {
            toks.push(Tok::Time(t));
        } else if let Some(m) = month(w.trim_end_matches('.')) {
            // The day may be a range ("Oct 10-11"): the first is the start, the last the end.
            let day = |d: &str| d.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok().filter(|d| (1..=31).contains(d));
            if let Some(next) = words.get(i + 1).filter(|n| n.chars().next().is_some_and(|c| c.is_ascii_digit())) {
                if let Some((a, b)) = next.split_once('-').and_then(|(a, b)| Some((day(a)?, day(b)?))) {
                    toks.push(Tok::Date(m, a));
                    toks.push(Tok::Date(m, b));
                    i += 1;
                } else if let Some(d) = day(next) {
                    toks.push(Tok::Date(m, d));
                    i += 1;
                }
            }
        }
        i += 1;
    }

    // Pair times with dates: (time or none, date). Two times before a date are a span within it.
    let mut spans: Vec<(i64, i64)> = Vec::new();
    let mut halves: Vec<(Option<i64>, (u32, u32))> = Vec::new();
    let mut pending: Vec<i64> = Vec::new();
    let at = |md: (u32, u32), min: i64| -> Option<i64> {
        // The year closest to today, so a December page can list January contests.
        let best = (today.year() - 1..=today.year() + 1)
            .filter_map(|y| NaiveDate::from_ymd_opt(y, md.0, md.1))
            .min_by_key(|d| (*d - today).num_days().abs())?;
        Some(best.and_hms_opt(0, 0, 0)?.and_utc().timestamp() + min * 60)
    };
    for t in toks {
        match t {
            Tok::Time(m) => pending.push(m),
            Tok::Date(mo, d) => {
                match pending.len() {
                    0 => halves.push((None, (mo, d))),
                    1 => halves.push((Some(pending[0]), (mo, d))),
                    _ => {
                        let (a, b) = (at((mo, d), pending[0])?, at((mo, d), pending[1])?);
                        spans.push((a, if b <= a { b + 86_400 } else { b }));
                    }
                }
                pending.clear();
            }
        }
    }
    for pair in halves.chunks(2) {
        let (t1, d1) = pair[0];
        let start = at(d1, t1.unwrap_or(0))?;
        let end = match pair.get(1) {
            Some((t2, d2)) => {
                let e = at(*d2, t2.unwrap_or(23 * 60 + 59))?;
                // A range that wraps the new year.
                if e < start { e + 365 * 86_400 } else { e }
            }
            None => start + if t1.is_some() { 3600 } else { 86_399 },
        };
        spans.push((start, end));
    }
    let start = spans.iter().map(|s| s.0).min()?;
    let end = spans.iter().map(|s| s.1).max()?;
    Some((start, end))
}

/// Which mode groups a contest's name says it is run in; empty means it doesn't say.
fn modes_of(title: &str) -> Vec<String> {
    let lower = title.to_ascii_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let has = |names: &[&str]| words.iter().any(|w| names.contains(w));
    let mut out = Vec::new();
    if has(&["cw"]) {
        out.push("CW".to_string());
    }
    if has(&["ssb", "phone", "voice", "sideband", "fm"]) {
        out.push("PHONE".to_string());
    }
    if has(&["rtty", "digital", "data", "psk", "psk31", "ft8", "ft4", "mfsk", "fsk", "jt65", "olivia"]) || lower.contains("digi") {
        out.push("DIGITAL".to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc().timestamp()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn date_shapes() {
        let today = day(2026, 10, 9);
        assert_eq!(parse_when("1200Z, Oct 10 to 1200Z, Oct 11", today), Some((ts(2026, 10, 10, 12, 0), ts(2026, 10, 11, 12, 0))));
        assert_eq!(parse_when("0000Z-2359Z, Oct 10", today), Some((ts(2026, 10, 10, 0, 0), ts(2026, 10, 10, 23, 59))));
        // Past midnight.
        assert_eq!(parse_when("2000Z-0100Z, Oct 10", today), Some((ts(2026, 10, 10, 20, 0), ts(2026, 10, 11, 1, 0))));
        // Several parts: earliest start to latest end.
        assert_eq!(
            parse_when("0100Z-0300Z, Oct 12 and 1900Z-2100Z, Oct 12 and 0100Z-0300Z, Oct 13", today),
            Some((ts(2026, 10, 12, 1, 0), ts(2026, 10, 13, 3, 0)))
        );
        // The new year.
        assert_eq!(parse_when("1500Z, Dec 31 to 1500Z, Jan 1", day(2026, 12, 20)), Some((ts(2026, 12, 31, 15, 0), ts(2027, 1, 1, 15, 0))));
        assert_eq!(parse_when("0000Z, Jan 2 to 2359Z, Jan 3", day(2026, 12, 20)), Some((ts(2027, 1, 2, 0, 0), ts(2027, 1, 3, 23, 59))));
        // Whole days.
        assert_eq!(parse_when("Oct 10-11", today), Some((ts(2026, 10, 10, 0, 0), ts(2026, 10, 11, 23, 59))));
        assert_eq!(parse_when("nothing here", today), None);
    }

    #[test]
    fn modes() {
        assert_eq!(modes_of("CQ WW DX Contest, CW"), ["CW"]);
        assert_eq!(modes_of("ARRL 10 Meter Contest (CW/SSB)"), ["CW", "PHONE"]);
        assert_eq!(modes_of("CQ WW RTTY WPX Contest"), ["DIGITAL"]);
        assert_eq!(modes_of("Weekly Digimode Sprint"), ["DIGITAL"]);
        assert!(modes_of("Stew Perry Topband Challenge").is_empty());
    }

    #[test]
    fn rss() {
        let now = ts(2026, 10, 9, 12, 0);
        let feed = "<?xml version=\"1.0\"?><rss><channel><title>Calendar of ham radio contests</title>\
            <item><title>Oceania DX Contest, Phone</title><link>https://www.contestcalendar.com/contestdetails.php?ref=1</link><description>0800Z, Oct 3 to 0800Z, Oct 4</description></item>\
            <item><title><![CDATA[Worked All Germany &amp; Friends, CW]]></title><link>https://example.test/waag</link><description>&lt;b&gt;1200Z, Oct 10&lt;/b&gt; to 1200Z, Oct 11</description></item>\
            <item><title>No dates here</title><description>soon</description></item></channel></rss>";
        let list = parse_rss(feed, now);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].title, "Worked All Germany & Friends, CW");
        assert_eq!(list[1].link, "https://example.test/waag");
        assert_eq!((list[1].start, list[1].end), (ts(2026, 10, 10, 12, 0), ts(2026, 10, 11, 12, 0)));
        assert_eq!(list[1].modes, ["CW"]);
    }
}
