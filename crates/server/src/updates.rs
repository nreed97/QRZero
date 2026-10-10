//! Update check: finds out whether a newer QRZero release has been tagged on GitHub
//! and links its page. It never downloads or installs anything.
//!
//! One small request a little after start and then once a day, in the background.
//! A failed check is quietly tried again later. Settings > General can turn it off.

use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::station::Hub;

pub const DEFAULT_URL: &str = "https://api.github.com/repos/nreed97/QRZero/releases?per_page=15";
const ENABLED_KEY: &str = "updates.enabled";

/// Wait this long after start before the first check, so launch is never slowed.
const FIRST_DELAY: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(24 * 3600);
const RETRY: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, Default, PartialEq)]
struct Release {
    /// The version as numbers, e.g. [0, 14].
    version: Vec<u64>,
    /// "0.14" as the release names it.
    label: String,
    url: String,
}

pub struct Updates {
    url: Mutex<String>,
    http: reqwest::Client,
    latest: Mutex<Option<Release>>,
    fetching: tokio::sync::Mutex<()>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
}

/// The numbers in the version part of a tag: "QRZero-Alpha-v0.14" gives [0, 14].
fn tag_version(tag: &str) -> Option<Vec<u64>> {
    let rest = &tag[tag.rfind(['v', 'V'])? + 1..];
    let v: Vec<u64> = rest.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    (!v.is_empty()).then_some(v)
}

/// True when `a` is a later version than `b` ("0.14" > "0.13.0", "0.13.0" == "0.13").
fn newer(a: &[u64], b: &[u64]) -> bool {
    let n = a.len().max(b.len());
    let get = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    (0..n).map(|i| get(a, i).cmp(&get(b, i))).find(|o| o.is_ne()).is_some_and(|o| o.is_gt())
}

fn highest(list: Vec<GhRelease>) -> Option<Release> {
    list.into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = tag_version(&r.tag_name)?;
            let label = version.iter().map(u64::to_string).collect::<Vec<_>>().join(".");
            Some(Release { version, label, url: r.html_url })
        })
        .reduce(|best, r| if newer(&r.version, &best.version) { r } else { best })
}

impl Updates {
    pub fn new() -> Self {
        Updates {
            url: Mutex::new(DEFAULT_URL.to_string()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("QRZero/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            latest: Mutex::new(None),
            fetching: tokio::sync::Mutex::new(()),
        }
    }

    pub fn set_url(&self, url: String) {
        *self.url.lock().unwrap_or_else(|p| p.into_inner()) = url;
    }

    async fn fetch(&self) -> Result<Option<Release>, String> {
        let url = self.url.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let resp = self.http.get(&url).header("accept", "application/vnd.github+json").send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("answered {}", resp.status()));
        }
        let list: Vec<GhRelease> = serde_json::from_str(&resp.text().await.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(highest(list))
    }

    /// What the header shows: `update` is set only when a newer release exists.
    pub fn view(&self, enabled: bool) -> Value {
        let current = crate::VERSION;
        let cur: Vec<u64> = current.split('.').filter_map(|p| p.parse().ok()).collect();
        let latest = self.latest.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let update = latest.filter(|l| enabled && newer(&l.version, &cur)).map(|l| json!({ "version": l.label, "url": l.url }));
        json!({ "enabled": enabled, "current": current, "update": update })
    }
}

pub fn enabled(hub: &Hub) -> bool {
    hub.setting(ENABLED_KEY).as_deref() != Some("0")
}

pub fn set_enabled(hub: &Hub, on: bool) -> Result<(), String> {
    hub.set_setting(ENABLED_KEY, if on { "1" } else { "0" })
}

/// Checks once now (if the check is on) and returns whether it worked.
pub async fn check(hub: &Hub) -> bool {
    if !enabled(hub) {
        return true;
    }
    let u = &hub.updates;
    let _one = u.fetching.lock().await;
    match u.fetch().await {
        Ok(found) => {
            *u.latest.lock().unwrap_or_else(|p| p.into_inner()) = found;
            true
        }
        Err(e) => {
            tracing::info!("update check: {e}");
            false
        }
    }
}

/// Background loop: first check shortly after start, then daily. Holds the hub weakly so
/// it never keeps the database open.
pub fn spawn(hub: &std::sync::Arc<Hub>) {
    let weak = std::sync::Arc::downgrade(hub);
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_DELAY).await;
        loop {
            let Some(hub) = weak.upgrade() else { break };
            let ok = check(&hub).await;
            drop(hub);
            tokio::time::sleep(if ok { EVERY } else { RETRY }).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(tag_version("QRZero-Alpha-v0.14"), Some(vec![0, 14]));
        assert_eq!(tag_version("Alpha-Release"), None);
        assert!(newer(&[0, 14], &[0, 13, 0]));
        assert!(newer(&[0, 10], &[0, 9]));
        assert!(!newer(&[0, 13], &[0, 13, 0]));
        assert!(!newer(&[0, 12], &[0, 13, 0]));
    }

    #[test]
    fn picks_the_highest_non_draft() {
        let r = |tag: &str, draft| GhRelease { tag_name: tag.into(), html_url: format!("https://x/{tag}"), draft };
        let best = highest(vec![r("QRZero-Alpha-v0.9", false), r("QRZero-Alpha-v0.10", false), r("QRZero-Alpha-v0.11", true), r("Alpha-Release", false)]).unwrap();
        assert_eq!(best.label, "0.10");
    }
}
