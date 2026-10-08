//! Solar data and band conditions from N0NBH's feed (hamqsl.com).
//!
//! Fetched only when someone asks for it, at most every 30 minutes, so the
//! feed is left alone while nobody has the Propagation pane open. A failed
//! fetch keeps the last good data and reports the error next to it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDateTime, Utc};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::Serialize;
use serde_json::{json, Value};

pub const DEFAULT_URL: &str = "https://www.hamqsl.com/solarxml.php";

/// How long good data is kept before it is fetched again.
const REFRESH: Duration = Duration::from_secs(30 * 60);
/// After a failure, try again this much later.
const RETRY: Duration = Duration::from_secs(5 * 60);
/// A manual refresh is ignored when the last fetch was this recent.
const MANUAL_MIN: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Solar {
    /// Who calculated it ("N0NBH").
    pub source: String,
    /// The feed's own timestamp, as given (" 08 Oct 2026 1400 GMT").
    pub updated: String,
    /// The same as RFC 3339 UTC, when it could be read.
    pub updated_utc: Option<String>,
    /// Every simple value under <solardata>, by element name, trimmed.
    pub values: BTreeMap<String, String>,
    pub bands: Vec<BandCondition>,
    pub vhf: Vec<VhfCondition>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BandCondition {
    /// "80m-40m".
    pub name: String,
    /// "day" or "night".
    pub time: String,
    /// "Good", "Fair" or "Poor".
    pub condition: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VhfCondition {
    /// "vhf-aurora", "E-Skip".
    pub name: String,
    /// "northern_hemi", "europe_6m".
    pub location: String,
    /// "Band Closed", "High MUF", ...
    pub condition: String,
}

#[derive(Default)]
struct Inner {
    data: Option<Solar>,
    fetched_at: Option<DateTime<Utc>>,
    error: Option<String>,
    last_attempt: Option<Instant>,
}

pub struct Propagation {
    url: String,
    http: reqwest::Client,
    inner: Mutex<Inner>,
    /// One fetch at a time; others wait for it and use its result.
    fetching: tokio::sync::Mutex<()>,
}

impl Propagation {
    pub fn new(url: String) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("QRZero/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Arc::new(Propagation { url, http, inner: Mutex::default(), fetching: tokio::sync::Mutex::new(()) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn due(&self, force: bool) -> bool {
        let inner = self.lock();
        let Some(at) = inner.last_attempt else { return true };
        let age = at.elapsed();
        if force {
            return age >= MANUAL_MIN;
        }
        let wait = if inner.error.is_some() || inner.data.is_none() { RETRY } else { REFRESH };
        age >= wait
    }

    /// The current data, fetched first if it is stale (or `force` and not
    /// fetched in the last minute).
    pub async fn get(&self, force: bool) -> Value {
        if self.due(force) {
            let _one = self.fetching.lock().await;
            // Someone else may have fetched while we waited.
            if self.due(force) {
                let result = self.fetch().await;
                let mut inner = self.lock();
                inner.last_attempt = Some(Instant::now());
                match result {
                    Ok(data) => {
                        inner.data = Some(data);
                        inner.fetched_at = Some(Utc::now());
                        inner.error = None;
                    }
                    Err(e) => {
                        tracing::warn!("propagation feed: {e}");
                        inner.error = Some(e);
                    }
                }
            }
        }
        let inner = self.lock();
        json!({
            "data": inner.data,
            "fetched_at": inner.fetched_at.map(|t| t.to_rfc3339()),
            "error": inner.error,
        })
    }

    async fn fetch(&self) -> Result<Solar, String> {
        let resp = self.http.get(&self.url).send().await.map_err(|e| format!("could not reach the solar data feed: {}", describe(e)))?;
        if !resp.status().is_success() {
            return Err(format!("the solar data feed answered {}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| format!("solar data feed: {}", describe(e)))?;
        parse(&text)
    }
}

/// A reqwest error and its causes, without the URL.
fn describe(e: reqwest::Error) -> String {
    let e = e.without_url();
    let mut out = e.to_string();
    let mut src = std::error::Error::source(&e);
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&s.to_string());
        src = s.source();
    }
    out
}

fn attr(e: &BytesStart, name: &[u8]) -> String {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref().eq_ignore_ascii_case(name))
        .and_then(|a| a.unescape_value().ok().map(|v| v.trim().to_string()))
        .unwrap_or_default()
}

/// Reads N0NBH's solar XML. Tolerates extra or missing elements and stops
/// at the first malformed part, keeping what came before it.
pub fn parse(xml: &str) -> Result<Solar, String> {
    let mut reader = Reader::from_str(xml);
    let mut out = Solar::default();
    // Open elements, lower-case, with the attributes we care about.
    let mut path: Vec<(String, String, String)> = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                let (a, b) = match name.as_str() {
                    "band" => (attr(&e, b"name"), attr(&e, b"time")),
                    "phenomenon" => (attr(&e, b"name"), attr(&e, b"location")),
                    _ => (String::new(), String::new()),
                };
                path.push((name, a, b));
                text.clear();
            }
            Ok(Event::Text(t)) => text.push_str(&t.decode().unwrap_or_default()),
            Ok(Event::CData(t)) => text.push_str(&t.decode().unwrap_or_default()),
            Ok(Event::GeneralRef(r)) => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Some(s) = r.decode().ok().and_then(|n| quick_xml::escape::resolve_predefined_entity(&n)) {
                    text.push_str(s);
                }
            }
            Ok(Event::End(_)) => {
                let Some((name, a, b)) = path.pop() else { continue };
                let parent = path.last().map(|p| p.0.as_str()).unwrap_or("");
                let value = text.trim().to_string();
                text.clear();
                match (parent, name.as_str()) {
                    ("solardata", "source") => out.source = value,
                    ("solardata", "updated") => {
                        out.updated_utc = parse_updated(&value);
                        out.updated = value;
                    }
                    ("solardata", "calculatedconditions" | "calculatedvhfconditions") => {}
                    ("solardata", _) => {
                        out.values.insert(name, value);
                    }
                    ("calculatedconditions", "band") => out.bands.push(BandCondition { name: a, time: b.to_lowercase(), condition: value }),
                    ("calculatedvhfconditions", "phenomenon") => out.vhf.push(VhfCondition { name: a, location: b, condition: value }),
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                if out.values.is_empty() {
                    return Err(format!("the solar data feed sent something unreadable: {e}"));
                }
                break;
            }
        }
    }
    if out.values.is_empty() && out.bands.is_empty() {
        return Err("the solar data feed had no solar data in it".into());
    }
    Ok(out)
}

/// " 08 Oct 2026 1400 GMT" to RFC 3339.
fn parse_updated(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches("GMT").trim_end_matches("UTC").trim();
    let t = NaiveDateTime::parse_from_str(s, "%d %b %Y %H%M").ok()?;
    Some(t.and_utc().to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_feed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<solar><solardata>
 <source url="http://www.hamqsl.com/solar.html">N0NBH</source>
 <updated> 08 Oct 2026 1400 GMT</updated>
 <solarflux>148</solarflux><kindex> 3</kindex><kindexnt>No Report</kindexnt>
 <calculatedconditions>
  <band name="80m-40m" time="day">Fair</band>
  <band name="80m-40m" time="night">Good</band>
 </calculatedconditions>
 <calculatedvhfconditions><phenomenon name="E-Skip" location="europe_6m">Band Closed</phenomenon></calculatedvhfconditions>
 <signalnoise>S1-S2 &amp; rising</signalnoise>
</solardata></solar>"#;
        let s = parse(xml).unwrap();
        assert_eq!(s.source, "N0NBH");
        assert_eq!(s.updated_utc.as_deref(), Some("2026-10-08T14:00:00+00:00"));
        assert_eq!(s.values["solarflux"], "148");
        assert_eq!(s.values["kindex"], "3");
        assert_eq!(s.values["kindexnt"], "No Report");
        assert_eq!(s.values["signalnoise"], "S1-S2 & rising");
        assert!(!s.values.contains_key("calculatedconditions"));
        assert_eq!(s.bands.len(), 2);
        assert_eq!(s.bands[1], BandCondition { name: "80m-40m".into(), time: "night".into(), condition: "Good".into() });
        assert_eq!(s.vhf[0].location, "europe_6m");
    }

    #[tokio::test]
    async fn a_failed_refresh_keeps_the_last_good_data() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let broken = Arc::new(AtomicBool::new(false));
        let b = broken.clone();
        let app = axum::Router::new().route(
            "/",
            axum::routing::get(move || {
                let b = b.clone();
                async move {
                    if b.load(Ordering::SeqCst) {
                        "<html>oops</html>".to_string()
                    } else {
                        "<solar><solardata><solarflux>120</solarflux></solardata></solar>".to_string()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let p = Propagation::new(url);
        let v = p.get(false).await;
        assert_eq!(v["data"]["values"]["solarflux"], "120");
        let fetched = v["fetched_at"].clone();

        broken.store(true, Ordering::SeqCst);
        p.lock().last_attempt = Instant::now().checked_sub(REFRESH);
        let v = p.get(false).await;
        assert_eq!(v["data"]["values"]["solarflux"], "120");
        assert_eq!(v["fetched_at"], fetched);
        assert!(v["error"].as_str().unwrap().contains("no solar data"), "{v}");

        // Retried sooner after a failure, and the error clears once it works.
        broken.store(false, Ordering::SeqCst);
        p.lock().last_attempt = Instant::now().checked_sub(RETRY);
        let v = p.get(false).await;
        assert!(v["error"].is_null());
    }

    #[test]
    fn keeps_what_it_read_before_garbage() {
        let s = parse("<solar><solardata><solarflux>101</solarflux><aindex>5</aindex></solardata><<").unwrap();
        assert_eq!(s.values["aindex"], "5");
        assert!(parse("<html><body>Service unavailable</body></html>").is_err());
        assert!(parse("not xml at all <<<").is_err());
    }
}
