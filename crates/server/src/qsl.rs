//! Uploads to QSL services: QRZ Logbook and Club Log on a timer, LoTW on
//! demand through the user's TQSL.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use qrzero_core::adif::{self, Fields};
use qrzero_core::awards::{Award, AwardQso, NewConfirm, Source};
use qrzero_core::confirm::{self, Service as ConfirmService};
use qrzero_core::qsl::{self, ClubLog, QrzLogbook, QslError, TqslJob, Upload};
use qrzero_core::{secrets, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::station::{cty_facts, Hub};

const BATCH: i64 = 500;
/// Shortest wait (minutes) before a live upload, so a burst of edits goes up once.
const LIVE_MIN: u32 = 1;
/// Minutes between sweeps of everything not yet uploaded, for services set to upload automatically.
const SWEEP_MIN: u64 = 15;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LotwMapping {
    pub callsign: String,
    pub location_id: i64,
    /// The station location's name in TQSL.
    pub station_location: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QslConfig {
    /// Settings from older versions, read only so `normalize` can carry them over:
    /// "upload shortly after logging" became "upload automatically".
    #[serde(skip_serializing)]
    pub qrz_live: bool,
    #[serde(skip_serializing)]
    pub clublog_live: bool,
    #[serde(skip_serializing)]
    pub eqsl_live: bool,
    /// Minutes to wait after a QSO is logged or edited before uploading it, per
    /// service; every change restarts the wait (1 to 60; 0: not set, `normalize` fills 2).
    pub qrz_live_delay_min: u32,
    pub clublog_live_delay_min: u32,
    pub eqsl_live_delay_min: u32,
    pub qrz_enabled: bool,
    /// Only QSOs from this date (YYYY-MM-DD) on are uploaded.
    pub qrz_since: String,
    /// Station callsigns that have a QRZ Logbook API key.
    pub qrz_calls: Vec<String>,
    pub clublog_enabled: bool,
    pub clublog_since: String,
    pub clublog_email: String,
    /// Station callsigns uploaded to Club Log (each to its own log there).
    pub clublog_calls: Vec<String>,
    pub tqsl_path: String,
    pub lotw_since: String,
    pub lotw: Vec<LotwMapping>,
    /// LoTW website login, for downloading confirmations.
    pub lotw_username: String,
    /// Confirmations already downloaded up to this date (YYYY-MM-DD; empty: all).
    pub lotw_rcvd_since: String,
    pub eqsl_enabled: bool,
    pub eqsl_since: String,
    pub eqsl_username: String,
    /// eQSL "QTH nickname", for accounts with more than one.
    pub eqsl_nickname: String,
    /// Station callsigns uploaded to the eQSL account.
    pub eqsl_calls: Vec<String>,
    pub eqsl_rcvd_since: String,
    /// Club Log matches already downloaded up to this date (YYYY-MM-DD; empty: all).
    pub clublog_rcvd_since: String,
    /// Download new eQSL confirmations once a day. (Older versions also
    /// downloaded LoTW's with it; see `lotw_download_enabled`.)
    pub confirm_daily: bool,
    /// Download new LoTW confirmations every `lotw_download_interval_min` minutes.
    pub lotw_download_enabled: bool,
    /// 0: not set yet (settings from an older version); `normalize` fills it.
    pub lotw_download_interval_min: u32,
    /// Tell TQSL to use the MY_* details in each QSO (state, grid, zones) instead of the
    /// station location's own (`-f update`). Records are never changed.
    pub lotw_use_log_qth: bool,
}

impl QslConfig {
    /// Carries settings from older versions over, and keeps every value in range.
    fn normalize(&mut self) {
        if self.lotw_download_interval_min == 0 {
            // Before this setting, "once a day" also downloaded LoTW.
            self.lotw_download_interval_min = 24 * 60;
            self.lotw_download_enabled = self.confirm_daily;
        }
        self.lotw_download_interval_min = self.lotw_download_interval_min.clamp(1, 7 * 24 * 60);
        for d in [&mut self.qrz_live_delay_min, &mut self.clublog_live_delay_min, &mut self.eqsl_live_delay_min] {
            *d = if *d == 0 { 2 } else { (*d).clamp(LIVE_MIN, 60) };
        }
        // The old "upload shortly after logging" tick is now the one automatic upload.
        self.qrz_enabled |= std::mem::take(&mut self.qrz_live);
        self.clublog_enabled |= std::mem::take(&mut self.clublog_live);
        self.eqsl_enabled |= std::mem::take(&mut self.eqsl_live);
    }
}

impl Default for QslConfig {
    fn default() -> Self {
        QslConfig {
            qrz_live: false,
            clublog_live: false,
            eqsl_live: false,
            qrz_live_delay_min: 0,
            clublog_live_delay_min: 0,
            eqsl_live_delay_min: 0,
            qrz_enabled: false,
            qrz_since: String::new(),
            qrz_calls: Vec::new(),
            clublog_enabled: false,
            clublog_since: String::new(),
            clublog_email: String::new(),
            clublog_calls: Vec::new(),
            tqsl_path: String::new(),
            lotw_since: String::new(),
            lotw: Vec::new(),
            lotw_username: String::new(),
            lotw_rcvd_since: String::new(),
            eqsl_enabled: false,
            eqsl_since: String::new(),
            eqsl_username: String::new(),
            eqsl_nickname: String::new(),
            eqsl_calls: Vec::new(),
            eqsl_rcvd_since: String::new(),
            clublog_rcvd_since: String::new(),
            confirm_daily: false,
            lotw_download_enabled: false,
            lotw_download_interval_min: 0,
            lotw_use_log_qth: true,
        }
    }
}

/// The outcome of the last upload to a service.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Run {
    pub at: i64,
    pub running: bool,
    pub uploaded: usize,
    pub duplicates: usize,
    pub rejected: Vec<String>,
    pub error: Option<String>,
}

/// The outcome of the last confirmation download from LoTW or eQSL.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Download {
    pub at: i64,
    pub running: bool,
    /// Confirmations downloaded.
    pub received: usize,
    /// QSOs newly marked confirmed.
    pub confirmed: usize,
    /// Downloaded confirmations with no matching QSO (first 1000).
    pub unmatched: Vec<String>,
    pub unmatched_count: usize,
    /// Award cells this download confirmed for the first time from its service (first 5000).
    pub new_awards: Vec<NewConfirm>,
    /// Started by a timer, not by the user.
    pub auto: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct Inner {
    config: QslConfig,
    runs: BTreeMap<&'static str, Run>,
    downloads: BTreeMap<&'static str, Download>,
    /// QSOs a service refused this session, so they aren't retried every few minutes.
    refused: HashSet<(&'static str, i64)>,
}

pub struct Qsl {
    store: Arc<Mutex<Store>>,
    hub: Arc<Hub>,
    secret_service: String,
    data_dir: PathBuf,
    endpoints: Endpoints,
    inner: Mutex<Inner>,
    /// One upload at a time per service.
    busy: tokio::sync::Mutex<()>,
}

/// Service URLs, overridable for tests.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub qrz: String,
    pub clublog: String,
    pub clublog_matches: String,
    pub eqsl_upload: String,
    pub eqsl_inbox: String,
    pub lotw_report: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            qrz: qsl::QRZ_LOGBOOK_ENDPOINT.into(),
            clublog: qsl::CLUBLOG_ENDPOINT.into(),
            clublog_matches: confirm::CLUBLOG_MATCHES_ENDPOINT.into(),
            eqsl_upload: confirm::EQSL_UPLOAD_ENDPOINT.into(),
            eqsl_inbox: confirm::EQSL_INBOX_ENDPOINT.into(),
            lotw_report: confirm::LOTW_REPORT_ENDPOINT.into(),
        }
    }
}

struct Service {
    name: &'static str,
    status_key: &'static str,
    date_key: &'static str,
}

const QRZ: Service = Service { name: "qrz", status_key: "QRZCOM_QSO_UPLOAD_STATUS", date_key: "QRZCOM_QSO_UPLOAD_DATE" };
const CLUBLOG: Service = Service { name: "clublog", status_key: "CLUBLOG_QSO_UPLOAD_STATUS", date_key: "CLUBLOG_QSO_UPLOAD_DATE" };
const EQSL: Service = Service { name: "eqsl", status_key: "EQSL_QSL_SENT", date_key: "EQSL_QSLSDATE" };
const LOTW: Service = Service { name: "lotw", status_key: "LOTW_QSL_SENT", date_key: "LOTW_QSLSDATE" };

fn qrz_secret(call: &str) -> String {
    format!("qrz-logbook:{}", call.to_ascii_uppercase())
}

fn today() -> String {
    Utc::now().format("%Y%m%d").to_string()
}

/// Unix seconds for a YYYY-MM-DD date; an empty or bad date means "from now on".
fn since(date: &str) -> i64 {
    NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map_or_else(|| Utc::now().timestamp(), |d| d.and_utc().timestamp())
}

/// Unix seconds for the start of `from` and the end of `to` (exclusive), both YYYY-MM-DD.
/// A blank `from` means the start of the log, a blank `to` means no end.
fn day_range(from: &str, to: &str) -> Result<(i64, i64), String> {
    let day = |d: &str| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").map_err(|_| format!("'{d}' isn't a date"));
    let at = |d: NaiveDate| d.and_hms_opt(0, 0, 0).map_or(0, |t| t.and_utc().timestamp());
    let a = if from.trim().is_empty() { None } else { Some(day(from)?) };
    let b = if to.trim().is_empty() { None } else { Some(day(to)?) };
    if let (Some(a), Some(b)) = (a, b) {
        if b < a {
            return Err("the end date is before the start date".into());
        }
    }
    Ok((a.map_or(0, at), b.map_or(i64::MAX, |b| at(b) + 86_400)))
}

/// A QSO waiting to go to a service, for the preview.
#[derive(Serialize)]
pub struct QueueRow {
    id: i64,
    call: String,
    date: String,
    time: String,
    band: String,
    mode: String,
    station: String,
}

/// What a service hasn't been sent in a date range.
#[derive(Serialize)]
pub struct Queue {
    total: i64,
    rows: Vec<QueueRow>,
}

impl Qsl {
    fn service(name: &str) -> Option<&'static Service> {
        match name {
            "qrz" => Some(&QRZ),
            "clublog" => Some(&CLUBLOG),
            "eqsl" => Some(&EQSL),
            "lotw" => Some(&LOTW),
            _ => None,
        }
    }

    /// The QSOs in a date range that `name` hasn't been sent (status not Y or I), the
    /// first `limit` of them oldest first, and how many there are in all. Only QSOs the
    /// service is set up for count: the ticked callsigns, or for LoTW the mapped locations.
    pub fn queue(&self, name: &str, from: &str, to: &str, limit: i64) -> Result<Queue, String> {
        let svc = Self::service(name).ok_or_else(|| format!("unknown service {name}"))?;
        let (a, b) = day_range(from, to)?;
        let cfg = self.config();
        let mut total = 0;
        let mut qsos = Vec::new();
        let mut take = |st: &mut Store, log_id: i64, calls: &[String], loc: Option<i64>| -> qrzero_core::Result<()> {
            total += st.count_pending_between(log_id, svc.status_key, calls, loc, a, b)?;
            let room = limit - qsos.len() as i64;
            if room > 0 {
                qsos.extend(st.pending_uploads_between(log_id, svc.status_key, calls, loc, a, b, room)?);
            }
            Ok(())
        };
        self.db(|st| {
            if name == "lotw" {
                for m in &cfg.lotw {
                    let loc = st.get_location(m.location_id)?;
                    take(st, loc.log_id, std::slice::from_ref(&m.callsign), Some(m.location_id))?;
                }
            } else {
                let calls = match name {
                    "qrz" => &cfg.qrz_calls,
                    "eqsl" => &cfg.eqsl_calls,
                    _ => &cfg.clublog_calls,
                };
                for log in st.list_logs()? {
                    take(st, log.id, calls, None)?;
                }
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        let rows = qsos
            .into_iter()
            .map(|q| {
                let f = |k: &str| q.fields.get(k).cloned().unwrap_or_default();
                let t = f("TIME_ON");
                QueueRow {
                    id: q.id,
                    call: f("CALL"),
                    date: f("QSO_DATE"),
                    time: t.get(..4).unwrap_or(&t).to_string(),
                    band: f("BAND"),
                    mode: f("MODE"),
                    station: f("STATION_CALLSIGN"),
                }
            })
            .collect();
        Ok(Queue { total, rows })
    }

    /// Takes QSOs out of a service's queue for good: their status becomes "I" (ignore),
    /// which uploads skip until the status is changed in the QSO editor.
    pub fn unqueue(&self, name: &str, ids: &[i64]) -> Result<usize, String> {
        let svc = Self::service(name).ok_or_else(|| format!("unknown service {name}"))?;
        let set: Fields = [(svc.status_key.to_string(), "I".to_string())].into();
        self.db(|st| st.mark_qsos(ids, &set)).map_err(|e| e.to_string())?;
        self.hub.emit(json!({"type": "qso_logged", "log_id": null, "call": "", "source": svc.name, "added": false}));
        Ok(ids.len())
    }

    /// Uploads everything in a date range that `name` hasn't been sent, whatever the
    /// service's "QSOs from" date says.
    pub async fn upload_range(&self, name: &str, from: &str, to: &str, location: Option<&str>) -> Run {
        let bounds = match day_range(from, to) {
            Ok(b) => b,
            Err(e) => return Run { error: Some(e), at: Utc::now().timestamp(), ..Run::default() },
        };
        match name {
            "lotw" => self.upload_lotw(Some(bounds), location).await,
            _ => self.upload_picked(name, None, Some(bounds)).await,
        }
    }

    pub fn new(store: Arc<Mutex<Store>>, hub: Arc<Hub>, secret_service: String, data_dir: PathBuf, endpoints: Endpoints) -> Arc<Self> {
        let mut config: QslConfig = hub.setting("qsl").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        config.normalize();
        Arc::new(Qsl {
            store,
            hub,
            secret_service,
            data_dir,
            endpoints,
            inner: Mutex::new(Inner { config, ..Inner::default() }),
            busy: tokio::sync::Mutex::new(()),
        })
    }

    /// Sweeps QRZ, Club Log and eQSL for QSOs not yet uploaded (the live upload does the rest), and
    /// downloads confirmations once a day when that is on.
    pub fn start(self: &Arc<Self>) {
        self.start_live();
        let me = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut last: BTreeMap<&'static str, std::time::Instant> = BTreeMap::new();
            let begin = std::time::Instant::now();
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let Some(q) = me.upgrade() else { return };
                let cfg = q.config();
                // A slow sweep behind the live upload, for QSOs that arrive another way
                // (an import) or whose upload failed.
                for (svc, on) in [("qrz", cfg.qrz_enabled), ("clublog", cfg.clublog_enabled), ("eqsl", cfg.eqsl_enabled)] {
                    let since = *last.entry(svc).or_insert(begin);
                    if on && since.elapsed() >= Duration::from_secs(60 * SWEEP_MIN) {
                        last.insert(svc, std::time::Instant::now());
                        q.upload(svc).await;
                    }
                }
                for (svc, on, minutes, ready) in [
                    ("lotw", cfg.lotw_download_enabled, cfg.lotw_download_interval_min, !cfg.lotw_username.is_empty()),
                    ("eqsl", cfg.confirm_daily, 24 * 60, !cfg.eqsl_username.is_empty()),
                ] {
                    let key = if svc == "lotw" { "lotw-rcvd" } else { "eqsl-rcvd" };
                    let since = *last.entry(key).or_insert(begin);
                    if on && ready && since.elapsed() >= Duration::from_secs(60 * minutes as u64) {
                        last.insert(key, std::time::Instant::now());
                        q.download(svc, true).await;
                    }
                }
            }
        });
    }

    /// Live upload: a new or edited QSO starts (or restarts) each live service's
    /// delay; when it runs out, everything pending goes up.
    fn start_live(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        let (_, mut rx) = self.hub.subscribe();
        tokio::spawn(async move {
            let mut due: BTreeMap<&'static str, std::time::Instant> = BTreeMap::new();
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            loop {
                tokio::select! {
                    ev = rx.recv() => {
                        let changed = match ev {
                            Ok(raw) => serde_json::from_str::<serde_json::Value>(&raw).is_ok_and(|v| is_qso_change(&v)),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                            Err(_) => return,
                        };
                        if !changed {
                            continue;
                        }
                        let Some(q) = me.upgrade() else { return };
                        let cfg = q.config();
                        for (svc, live, delay) in [
                            ("qrz", cfg.qrz_enabled, cfg.qrz_live_delay_min),
                            ("clublog", cfg.clublog_enabled, cfg.clublog_live_delay_min),
                            ("eqsl", cfg.eqsl_enabled, cfg.eqsl_live_delay_min),
                        ] {
                            if live {
                                due.insert(svc, std::time::Instant::now() + Duration::from_secs(60 * delay as u64));
                            }
                        }
                    }
                    _ = tick.tick() => {
                        let now = std::time::Instant::now();
                        let ready: Vec<&'static str> = due.iter().filter(|(_, t)| **t <= now).map(|(s, _)| *s).collect();
                        if ready.is_empty() {
                            continue;
                        }
                        let Some(q) = me.upgrade() else { return };
                        let cfg = q.config();
                        for svc in ready {
                            due.remove(svc);
                            let live = match svc { "qrz" => cfg.qrz_enabled, "clublog" => cfg.clublog_enabled, _ => cfg.eqsl_enabled };
                            if live {
                                q.upload(svc).await;
                            }
                        }
                    }
                }
            }
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn db<T>(&self, f: impl FnOnce(&mut Store) -> qrzero_core::Result<T>) -> qrzero_core::Result<T> {
        let mut st = self.store.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut st)
    }

    pub fn config(&self) -> QslConfig {
        self.lock().config.clone()
    }

    fn secret(&self, name: &str) -> Option<String> {
        self.db(|st| secrets::get(st, &self.secret_service, name)).ok().flatten().filter(|s| !s.is_empty())
    }

    /// Settings, which secrets are saved, pending counts and last results, for the QSL dialog.
    pub fn overview(&self) -> serde_json::Value {
        let cfg = self.config();
        let qrz_keys: Vec<&String> = cfg.qrz_calls.iter().filter(|c| self.secret(&qrz_secret(c)).is_some()).collect();
        let pending = |svc: &Service, calls: &[String], date: &str| -> i64 {
            self.db(|st| {
                let mut n = 0;
                for log in st.list_logs()? {
                    n += st.count_pending(log.id, svc.status_key, calls, None, since(date))?;
                }
                Ok(n)
            })
            .unwrap_or(0)
        };
        let lotw: Vec<_> = cfg
            .lotw
            .iter()
            .map(|m| {
                let n = self
                    .db(|st| {
                        let loc = st.get_location(m.location_id)?;
                        st.count_pending(loc.log_id, LOTW.status_key, std::slice::from_ref(&m.callsign), Some(m.location_id), since(&cfg.lotw_since))
                    })
                    .unwrap_or(0);
                json!({"mapping": m, "pending": n})
            })
            .collect();
        let tqsl = if cfg.tqsl_path.is_empty() { qsl::find_tqsl() } else { Some(PathBuf::from(&cfg.tqsl_path)) };
        json!({
            "config": cfg,
            "secrets": {
                "qrz_calls": qrz_keys,
                "clublog_password": self.secret("clublog-password").is_some(),
                "lotw_password": self.secret("lotw-password").is_some(),
                "eqsl_password": self.secret("eqsl-password").is_some(),
            },
            "pending": {
                "qrz": pending(&QRZ, &cfg.qrz_calls, &cfg.qrz_since),
                "clublog": pending(&CLUBLOG, &cfg.clublog_calls, &cfg.clublog_since),
                "eqsl": pending(&EQSL, &cfg.eqsl_calls, &cfg.eqsl_since),
                "lotw": lotw,
            },
            "runs": self.lock().runs,
            "downloads": self.lock().downloads,
            "tqsl": {
                "path": tqsl.as_ref().map(|p| p.display().to_string()),
                "found": tqsl.as_ref().is_some_and(|p| p.exists()),
                "locations": qsl::tqsl_station_locations(),
            },
        })
    }

    pub fn save(&self, mut cfg: QslConfig, update: SecretsUpdate) -> Result<(), String> {
        cfg.normalize();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        // Turning a service on starts from today, so an imported log isn't sent again.
        for (on, date) in [(cfg.qrz_enabled, &mut cfg.qrz_since), (cfg.clublog_enabled, &mut cfg.clublog_since), (cfg.eqsl_enabled, &mut cfg.eqsl_since)] {
            if on && date.trim().is_empty() {
                *date = today.clone();
            }
        }
        if cfg.lotw_since.trim().is_empty() && !cfg.lotw.is_empty() {
            cfg.lotw_since = today;
        }
        // A key typed in for a callsign means that logbook should be uploaded.
        for (call, key) in &update.qrz_keys {
            if !key.trim().is_empty() && !cfg.qrz_calls.iter().any(|c| c.eq_ignore_ascii_case(call.trim())) {
                cfg.qrz_calls.push(call.clone());
            }
        }
        for c in cfg.qrz_calls.iter_mut().chain(cfg.clublog_calls.iter_mut()).chain(cfg.eqsl_calls.iter_mut()) {
            *c = c.trim().to_ascii_uppercase();
        }
        let svc = self.secret_service.clone();
        self.db(|st| {
            for (call, key) in &update.qrz_keys {
                secrets::set(st, &svc, &qrz_secret(call), Some(key.trim()).filter(|k| !k.is_empty()))?;
            }
            if let Some(p) = &update.clublog_password {
                secrets::set(st, &svc, "clublog-password", Some(p.as_str()).filter(|p| !p.is_empty()))?;
            }
            if let Some(k) = &update.clublog_app_key {
                secrets::set(st, &svc, "clublog-app-key", Some(k.trim()).filter(|k| !k.is_empty()))?;
            }
            if let Some(p) = &update.lotw_password {
                secrets::set(st, &svc, "lotw-password", Some(p.as_str()).filter(|p| !p.is_empty()))?;
            }
            if let Some(p) = &update.eqsl_password {
                secrets::set(st, &svc, "eqsl-password", Some(p.as_str()).filter(|p| !p.is_empty()))?;
            }
            st.set_setting("qsl", &serde_json::to_string(&cfg)?)
        })
        .map_err(|e| e.to_string())?;
        self.lock().config = cfg;
        Ok(())
    }

    pub async fn test_qrz(&self, call: &str) -> Result<String, String> {
        let key = self.secret(&qrz_secret(call)).ok_or("no API key saved for that callsign")?;
        QrzLogbook::new(&self.endpoints.qrz, &key).status().await.map_err(|e| e.to_string())
    }

    fn set_run(&self, name: &'static str, run: Run) {
        self.lock().runs.insert(name, run.clone());
        self.hub.emit(json!({"type": "qsl", "service": name, "run": run}));
    }

    /// Uploads everything pending to QRZ, Club Log or eQSL.
    pub async fn upload(&self, name: &str) -> Run {
        self.upload_picked(name, None, None).await
    }

    /// Uploads the given QSOs (whatever their sent status) to a configured service.
    pub async fn upload_ids(&self, name: &str, ids: &[i64]) -> Run {
        if name == "lotw" {
            let only: HashSet<i64> = ids.iter().copied().collect();
            let _busy = self.busy.lock().await;
            self.set_run(LOTW.name, Run { running: true, at: Utc::now().timestamp(), ..Run::default() });
            let run = self.lotw_run(None, Some(&only), None).await;
            self.set_run(LOTW.name, run.clone());
            return run;
        }
        self.upload_picked(name, Some(ids), None).await
    }

    /// Services that are set up well enough to upload to, for the log's right-click menu.
    pub fn targets(&self) -> Vec<&'static str> {
        let cfg = self.config();
        let mut out = Vec::new();
        if cfg.qrz_calls.iter().any(|c| self.secret(&qrz_secret(c)).is_some()) {
            out.push("qrz");
        }
        if !cfg.clublog_calls.is_empty() && !cfg.clublog_email.is_empty() && self.secret("clublog-password").is_some() && self.clublog_app_key().is_some() {
            out.push("clublog");
        }
        if !cfg.eqsl_calls.is_empty() && !cfg.eqsl_username.is_empty() && self.secret("eqsl-password").is_some() {
            out.push("eqsl");
        }
        if !cfg.lotw.is_empty() {
            out.push("lotw");
        }
        out
    }

    async fn upload_picked(&self, name: &str, only: Option<&[i64]>, bounds: Option<(i64, i64)>) -> Run {
        let svc = match name {
            "qrz" => &QRZ,
            "clublog" => &CLUBLOG,
            "eqsl" => &EQSL,
            _ => return Run { error: Some(format!("unknown service {name}")), ..Run::default() },
        };
        let _busy = self.busy.lock().await;
        self.set_run(svc.name, Run { running: true, at: Utc::now().timestamp(), ..Run::default() });
        let run = self.upload_service(svc, only, bounds).await;
        self.set_run(svc.name, run.clone());
        run
    }

    /// The Club Log application key: built into release builds (`CLUBLOG_API_KEY` at
    /// compile time). A key an earlier version saved from the user is the fallback.
    fn clublog_app_key(&self) -> Option<String> {
        option_env!("CLUBLOG_API_KEY")
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
            .or_else(|| self.secret("clublog-app-key"))
    }

    async fn upload_service(&self, svc: &Service, only: Option<&[i64]>, bounds: Option<(i64, i64)>) -> Run {
        let mut run = Run { at: Utc::now().timestamp(), ..Run::default() };
        let cfg = self.config();
        let (calls, date) = match svc.name {
            "qrz" => (&cfg.qrz_calls, &cfg.qrz_since),
            "eqsl" => (&cfg.eqsl_calls, &cfg.eqsl_since),
            _ => (&cfg.clublog_calls, &cfg.clublog_since),
        };
        let eqsl_password = self.secret("eqsl-password");
        let (password, app_key) = (self.secret("clublog-password"), self.clublog_app_key());
        for call in calls {
            enum Client {
                Qrz(QrzLogbook),
                ClubLog(ClubLog),
                Eqsl(String),
            }
            let client = if svc.name == "eqsl" {
                match &eqsl_password {
                    Some(p) if !cfg.eqsl_username.is_empty() => Client::Eqsl(p.clone()),
                    _ => {
                        run.error = Some("eQSL needs your username and password".into());
                        return run;
                    }
                }
            } else if svc.name == "qrz" {
                match self.secret(&qrz_secret(call)) {
                    Some(key) => Client::Qrz(QrzLogbook::new(&self.endpoints.qrz, &key)),
                    None => continue,
                }
            } else {
                match (&password, &app_key) {
                    (Some(p), Some(k)) if !cfg.clublog_email.is_empty() => Client::ClubLog(ClubLog::new(&self.endpoints.clublog, &cfg.clublog_email, p, call, k)),
                    _ => {
                        run.error = Some(if app_key.is_none() && password.is_some() && !cfg.clublog_email.is_empty() {
                            "this build of QRZero has no Club Log application key yet".into()
                        } else {
                            "Club Log needs your email and password".into()
                        });
                        return run;
                    }
                }
            };
            let pending = self
                .db(|st| {
                    let mut all = Vec::new();
                    if let Some(ids) = only {
                        // Picked by hand: any status, but only QSOs logged as this station callsign.
                        for &id in ids {
                            let q = st.get_qso(id)?;
                            if q.fields.get("STATION_CALLSIGN").is_some_and(|c| c.eq_ignore_ascii_case(call)) {
                                all.push(q);
                            }
                        }
                        return Ok(all);
                    }
                    for log in st.list_logs()? {
                        all.extend(match bounds {
                            Some((a, b)) => st.pending_uploads_between(log.id, svc.status_key, std::slice::from_ref(call), None, a, b, 100_000)?,
                            None => st.pending_uploads(log.id, svc.status_key, std::slice::from_ref(call), None, since(date), BATCH)?,
                        });
                    }
                    Ok(all)
                })
                .unwrap_or_default();
            let mut changed_logs = HashSet::new();
            for qso in pending {
                if only.is_none() && self.lock().refused.contains(&(svc.name, qso.id)) {
                    continue;
                }
                let result = match &client {
                    Client::Qrz(c) => c.upload(&qso.fields, qso.fields.get(svc.status_key).is_some_and(|s| s == "M")).await,
                    Client::ClubLog(c) => c.upload(&qso.fields).await,
                    Client::Eqsl(password) => {
                        let nick = Some(cfg.eqsl_nickname.as_str()).filter(|n| !n.is_empty());
                        confirm::eqsl_upload(&self.endpoints.eqsl_upload, &cfg.eqsl_username, password, nick, &qso.fields).await
                    }
                };
                match result {
                    Ok(Upload::Added) | Ok(Upload::Duplicate) => {
                        if matches!(result, Ok(Upload::Added)) {
                            run.uploaded += 1;
                        } else {
                            run.duplicates += 1;
                        }
                        let set: Fields = [(svc.status_key.to_string(), "Y".to_string()), (svc.date_key.to_string(), today())].into();
                        if let Err(e) = self.db(|st| st.mark_qsos(&[qso.id], &set)) {
                            run.error = Some(e.to_string());
                            return run;
                        }
                        changed_logs.insert(qso.log_id);
                    }
                    Ok(Upload::Rejected(reason)) => {
                        self.lock().refused.insert((svc.name, qso.id));
                        let call = qso.fields.get("CALL").cloned().unwrap_or_default();
                        run.rejected.push(format!("{call}: {reason}"));
                    }
                    Err(e) => {
                        run.error = Some(match e {
                            QslError::Auth(m) => format!("{call}: login refused ({m})"),
                            other => other.to_string(),
                        });
                        break;
                    }
                }
            }
            if !changed_logs.is_empty() {
                self.hub.refresh_awards();
            }
            for log_id in changed_logs {
                self.hub.emit(json!({"type": "qso_logged", "log_id": log_id, "call": "", "source": svc.name, "added": false}));
            }
            if run.error.is_some() {
                break;
            }
        }
        run
    }

    fn set_download(&self, name: &'static str, d: Download) {
        self.lock().downloads.insert(name, d.clone());
        self.hub.emit(json!({"type": "qsl_download", "service": name, "run": d}));
    }

    /// Downloads new confirmations from LoTW or eQSL and marks the matching QSOs.
    pub async fn download(&self, name: &str, auto: bool) -> Download {
        let name: &'static str = match name {
            "lotw" => "lotw",
            "eqsl" => "eqsl",
            "qrz" => "qrz",
            "clublog" => "clublog",
            _ => return Download { error: Some(format!("unknown service {name}")), ..Download::default() },
        };
        let _busy = self.busy.lock().await;
        self.set_download(name, Download { running: true, at: Utc::now().timestamp(), auto, ..Download::default() });
        let mut d = self.download_service(name).await;
        d.auto = auto;
        self.set_download(name, d.clone());
        d
    }

    async fn download_service(&self, name: &'static str) -> Download {
        let mut d = Download { at: Utc::now().timestamp(), ..Download::default() };
        let cfg = self.config();
        let today_iso = || Utc::now().format("%Y-%m-%d").to_string();
        let (service, records, next_since) = match name {
            "lotw" => {
                let Some(password) = self.secret("lotw-password").filter(|_| !cfg.lotw_username.is_empty()) else {
                    d.error = Some("enter your LoTW website username and password first".into());
                    return d;
                };
                match confirm::lotw_confirmations(&self.endpoints.lotw_report, &cfg.lotw_username, &password, None, &cfg.lotw_rcvd_since).await {
                    Ok(r) => {
                        let next = r.last_qsl.as_deref().and_then(|t| t.get(..10)).map(str::to_string);
                        (ConfirmService::Lotw, r.records, next)
                    }
                    Err(e) => {
                        d.error = Some(e.to_string());
                        return d;
                    }
                }
            }
            "eqsl" => {
                let Some(password) = self.secret("eqsl-password").filter(|_| !cfg.eqsl_username.is_empty()) else {
                    d.error = Some("enter your eQSL username and password first".into());
                    return d;
                };
                let nick = Some(cfg.eqsl_nickname.as_str()).filter(|n| !n.is_empty());
                match confirm::eqsl_confirmations(&self.endpoints.eqsl_inbox, &cfg.eqsl_username, &password, nick, &cfg.eqsl_rcvd_since).await {
                    Ok(r) => (ConfirmService::Eqsl, r, Some(today_iso())),
                    Err(e) => {
                        d.error = Some(e.to_string());
                        return d;
                    }
                }
            }
            "qrz" => {
                // One logbook per callsign that has an API key.
                let keys: Vec<String> = cfg.qrz_calls.iter().filter_map(|c| self.secret(&qrz_secret(c))).collect();
                if keys.is_empty() {
                    d.error = Some("enter a QRZ Logbook API key and tick its callsign first".into());
                    return d;
                }
                let mut all = Vec::new();
                for key in keys {
                    match confirm::qrz_confirmations(&self.endpoints.qrz, &key).await {
                        Ok(r) => all.extend(r),
                        Err(e) => {
                            d.error = Some(e.to_string());
                            return d;
                        }
                    }
                }
                // QRZ has no "confirmed since" filter, so every download asks for them all.
                (ConfirmService::Qrz, all, None)
            }
            _ => {
                let (Some(password), Some(app_key)) = (self.secret("clublog-password").filter(|_| !cfg.clublog_email.is_empty()), self.clublog_app_key()) else {
                    d.error = Some("enter your Club Log email and password first".into());
                    return d;
                };
                if cfg.clublog_calls.is_empty() {
                    d.error = Some("tick a callsign on the Club Log page first".into());
                    return d;
                }
                let mut all = Vec::new();
                for call in &cfg.clublog_calls {
                    match confirm::clublog_matches(&self.endpoints.clublog_matches, &cfg.clublog_email, &password, call, &app_key, &cfg.clublog_rcvd_since).await {
                        Ok(r) => all.extend(r),
                        Err(e) => {
                            d.error = Some(format!("{call}: {e}"));
                            return d;
                        }
                    }
                }
                (ConfirmService::ClubLog, all, Some(today_iso()))
            }
        };
        let updates: Vec<_> = records
            .iter()
            .map(|r| {
                let u = confirm::confirmation_updates(service, r);
                (r, u.set, u.fill)
            })
            .collect();
        let cty = self.hub.cty();
        // Only LoTW and eQSL confirmations count toward awards.
        let source = match name {
            "lotw" => Some(Source::Lotw),
            "eqsl" => Some(Source::Eqsl),
            _ => None,
        };
        let stats = self.db(|st| {
            let logs: Vec<i64> = st.list_logs()?.iter().map(|l| l.id).collect();
            // The award cells as they stand, to tell which confirmations fill one for the first time.
            let mut cells = HashMap::new();
            if !updates.is_empty() && source.is_some() {
                for &id in &logs {
                    cells.insert(id, st.award_index(id, |c| cty_facts(cty.as_deref(), c))?.1);
                }
            }
            let mut stats = st.apply_confirmations(&logs, updates, confirm::same_qso)?;
            let mut seen = HashSet::new();
            for (log_id, fields) in std::mem::take(&mut stats.changed) {
                let (Some(index), Some(source)) = (cells.get_mut(&log_id), source) else { continue };
                let qso = AwardQso::from_fields(&fields, |c| cty_facts(cty.as_deref(), c));
                for mut n in index.confirmed(&qso, source) {
                    if n.award == Award::Dxcc {
                        let entity = cty.as_ref().and_then(|c| c.entities().iter().find(|e| e.dxcc.is_some_and(|d| d.to_string() == n.key)));
                        if let Some(e) = entity {
                            n.name = e.name.clone();
                        }
                    }
                    if d.new_awards.len() < 5000 && seen.insert((n.award as usize, n.key.clone(), n.column.clone())) {
                        d.new_awards.push(n);
                    }
                }
            }
            Ok(stats)
        });
        match stats {
            Ok(s) => {
                d.received = s.received;
                d.confirmed = s.new;
                d.unmatched_count = s.unmatched.len();
                d.unmatched = s.unmatched.into_iter().take(1000).collect();
            }
            Err(e) => {
                d.error = Some(e.to_string());
                return d;
            }
        }
        if let Some(next) = next_since {
            let mut c = self.config();
            match name {
                "lotw" => c.lotw_rcvd_since = next,
                "eqsl" => c.eqsl_rcvd_since = next,
                _ => c.clublog_rcvd_since = next,
            }
            if let Ok(text) = serde_json::to_string(&c) {
                let _ = self.db(|st| st.set_setting("qsl", &text));
            }
            self.lock().config = c;
        }
        if d.confirmed > 0 {
            self.hub.refresh_awards();
            self.hub.emit(json!({"type": "qso_logged", "log_id": null, "call": "", "source": name, "added": false}));
        }
        d
    }

    /// Signs and uploads pending QSOs to LoTW with TQSL, one station location at a time.
    /// With `bounds` (Unix seconds, end exclusive) it sends every QSO in that range that
    /// isn't marked sent, whatever the "QSOs from" date says.
    pub async fn upload_lotw(&self, bounds: Option<(i64, i64)>, location: Option<&str>) -> Run {
        let _busy = self.busy.lock().await;
        self.set_run(LOTW.name, Run { running: true, at: Utc::now().timestamp(), ..Run::default() });
        let run = self.lotw_run(bounds, None, location.filter(|l| !l.is_empty())).await;
        self.set_run(LOTW.name, run.clone());
        run
    }

    /// With `location`, every mapped location's QSOs are signed for that TQSL station location
    /// instead of the mapped one (the mapping still picks which QSOs go).
    async fn lotw_run(&self, bounds: Option<(i64, i64)>, only: Option<&HashSet<i64>>, location: Option<&str>) -> Run {
        let mut run = Run { at: Utc::now().timestamp(), ..Run::default() };
        let cfg = self.config();
        let Some(tqsl) = (if cfg.tqsl_path.is_empty() { qsl::find_tqsl() } else { Some(PathBuf::from(&cfg.tqsl_path)) }) else {
            run.error = Some("TQSL isn't installed (or set where tqsl.exe is)".into());
            return run;
        };
        if cfg.lotw.is_empty() {
            run.error = Some("pick a TQSL station location for your callsign and location first".into());
            return run;
        }
        let dir = self.data_dir.join("lotw");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            run.error = Some(e.to_string());
            return run;
        }
        for m in &cfg.lotw {
            let pending = self.db(|st| {
                if let Some(ids) = only {
                    let mut all = Vec::new();
                    for &id in ids {
                        let q = st.get_qso(id)?;
                        if q.location_id == Some(m.location_id) && q.fields.get("STATION_CALLSIGN").is_some_and(|c| c.eq_ignore_ascii_case(&m.callsign)) {
                            all.push(q);
                        }
                    }
                    return Ok(all);
                }
                let loc = st.get_location(m.location_id)?;
                let (from, until) = bounds.unwrap_or_else(|| (since(&cfg.lotw_since), i64::MAX));
                st.pending_uploads_between(loc.log_id, LOTW.status_key, std::slice::from_ref(&m.callsign), Some(m.location_id), from, until, 50_000)
            });
            let pending = match pending {
                Ok(p) if p.is_empty() => continue,
                Ok(p) => p,
                Err(e) => {
                    run.error = Some(e.to_string());
                    return run;
                }
            };
            let mut text = String::new();
            adif::write_header(&mut text, crate::VERSION, pending.len());
            for q in &pending {
                adif::write_record(&mut text, &q.fields, adif::is_standard_field);
            }
            let file = dir.join(format!("{}-{}.adi", m.callsign.replace('/', "_"), Utc::now().format("%Y%m%d-%H%M%S")));
            if let Err(e) = std::fs::write(&file, text) {
                run.error = Some(e.to_string());
                return run;
            }
            let station_location = location.unwrap_or(&m.station_location).to_string();
            let job = TqslJob { tqsl_path: tqsl.clone(), station_location, use_log_qth: cfg.lotw_use_log_qth, adif_path: file.clone() };
            let mut cmd = tokio::process::Command::new(&job.tqsl_path);
            cmd.args(job.args());
            #[cfg(windows)]
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            let status = cmd.status().await;
            let _ = std::fs::remove_file(&file);
            let code = match status {
                Ok(s) => s.code().unwrap_or(-1),
                Err(e) => {
                    run.error = Some(format!("couldn't start TQSL: {e}"));
                    return run;
                }
            };
            let outcome = qsl::tqsl_outcome(code);
            if !outcome.ok {
                run.error = Some(format!("{} at {}: {} (TQSL code {code})", m.callsign, job.station_location, outcome.message));
                return run;
            }
            let ids: Vec<i64> = pending.iter().map(|q| q.id).collect();
            let set: Fields = [(LOTW.status_key.to_string(), "Y".to_string()), (LOTW.date_key.to_string(), today())].into();
            if let Err(e) = self.db(|st| st.mark_qsos(&ids, &set)) {
                run.error = Some(e.to_string());
                return run;
            }
            if outcome.uploaded {
                run.uploaded += ids.len();
            } else {
                run.duplicates += ids.len();
            }
            if let Some(q) = pending.first() {
                self.hub.refresh_awards();
                self.hub.emit(json!({"type": "qso_logged", "log_id": q.log_id, "call": "", "source": "LoTW", "added": false}));
            }
        }
        run
    }
}

/// Whether a hub event means a QSO was logged or edited by the user (not our own
/// uploads, paper marks or QRZ lookups, which report as `qso_logged` too).
fn is_qso_change(v: &serde_json::Value) -> bool {
    match v["type"].as_str() {
        Some("qso_saved") => true,
        Some("qso_logged") => v["added"].as_bool() == Some(true),
        _ => false,
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct SecretsUpdate {
    /// API keys by station callsign; an empty key removes it.
    pub qrz_keys: BTreeMap<String, String>,
    pub clublog_password: Option<String>,
    /// Not shown in the UI: Club Log's application key is built into the app. Kept so a key can
    /// still be supplied through the API (and so older saved keys keep working).
    pub clublog_app_key: Option<String>,
    pub lotw_password: Option<String>,
    pub eqsl_password: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_dates() {
        assert_eq!(since("2024-01-02"), 1_704_153_600);
        assert!(since("") >= Utc::now().timestamp() - 5, "no date means from now on");
    }

    #[test]
    fn live_delay_has_a_minimum() {
        let mut c = QslConfig { qrz_live_delay_min: 0, clublog_live_delay_min: 1, eqsl_live_delay_min: 1000, ..QslConfig::default() };
        c.normalize();
        assert_eq!((c.qrz_live_delay_min, c.clublog_live_delay_min, c.eqsl_live_delay_min), (2, 1, 60));
    }

    #[test]
    fn only_saved_qsos_start_live_upload() {
        assert!(is_qso_change(&json!({"type": "qso_saved"})));
        assert!(is_qso_change(&json!({"type": "qso_logged", "added": true})));
        assert!(!is_qso_change(&json!({"type": "qso_logged", "added": false, "source": "qrz"})));
        assert!(!is_qso_change(&json!({"type": "qsl", "service": "qrz"})));
    }

    #[test]
    fn day_ranges() {
        assert_eq!(day_range("2024-01-02", "2024-01-02"), Ok((1_704_153_600, 1_704_153_600 + 86_400)));
        assert_eq!(day_range("2024-01-02", "2024-01-04").unwrap().1, 1_704_153_600 + 3 * 86_400);
        assert!(day_range("2024-01-03", "2024-01-02").is_err());
        assert!(day_range("x", "2024-01-02").is_err());
    }

    #[test]
    fn older_settings_carry_over() {
        // The old "upload shortly after logging" tick, or the timer tick, becomes "upload automatically".
        let mut c: QslConfig = serde_json::from_str(r#"{"interval_min":40,"clublog_live":true,"eqsl_enabled":true}"#).unwrap();
        c.normalize();
        assert!(c.clublog_enabled && c.eqsl_enabled && !c.qrz_enabled);
        assert_eq!((c.qrz_live_delay_min, c.clublog_live_delay_min), (2, 2));
        assert!(!serde_json::to_string(&c).unwrap().contains("_live\""), "the old tick isn't saved again");
        assert!(!c.lotw_download_enabled && c.lotw_download_interval_min == 1440);
        let mut d: QslConfig = serde_json::from_str(r#"{"confirm_daily":true}"#).unwrap();
        d.normalize();
        assert!(d.lotw_download_enabled, "daily LoTW download carries over");
    }
}
