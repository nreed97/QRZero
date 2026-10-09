//! Uploads to QSL services: QRZ Logbook and Club Log on a timer, LoTW on
//! demand through the user's TQSL.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use qrzero_core::adif::{self, Fields};
use qrzero_core::confirm::{self, Service as ConfirmService};
use qrzero_core::qsl::{self, ClubLog, QrzLogbook, QslError, TqslJob, Upload};
use qrzero_core::{secrets, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::station::Hub;

const BATCH: i64 = 500;

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
    /// Minutes between automatic QRZ and Club Log uploads.
    pub interval_min: u32,
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
    /// Download new LoTW and eQSL confirmations once a day.
    pub confirm_daily: bool,
}

impl Default for QslConfig {
    fn default() -> Self {
        QslConfig {
            interval_min: 15,
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
            confirm_daily: false,
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
    /// Downloaded confirmations with no matching QSO (first 50).
    pub unmatched: Vec<String>,
    pub unmatched_count: usize,
    pub error: Option<String>,
}

#[derive(Default)]
struct Inner {
    config: QslConfig,
    runs: BTreeMap<&'static str, Run>,
    downloads: BTreeMap<&'static str, Download>,
    /// When confirmations were last downloaded automatically (Unix seconds).
    last_auto_download: i64,
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
    pub eqsl_upload: String,
    pub eqsl_inbox: String,
    pub lotw_report: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            qrz: qsl::QRZ_LOGBOOK_ENDPOINT.into(),
            clublog: qsl::CLUBLOG_ENDPOINT.into(),
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
fn day_range(from: &str, to: &str) -> Result<(i64, i64), String> {
    let day = |d: &str| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").map_err(|_| format!("'{d}' isn't a date"));
    let (a, b) = (day(from)?, day(to)?);
    if b < a {
        return Err("the end date is before the start date".into());
    }
    let at = |d: NaiveDate| d.and_hms_opt(0, 0, 0).map_or(0, |t| t.and_utc().timestamp());
    Ok((at(a), at(b) + 86_400))
}

impl Qsl {
    /// QSOs in a date range that LoTW hasn't been sent yet, per mapped location.
    pub fn lotw_waiting(&self, from: &str, to: &str) -> Result<serde_json::Value, String> {
        let (a, b) = day_range(from, to)?;
        let cfg = self.config();
        let rows: Vec<_> = cfg
            .lotw
            .iter()
            .map(|m| {
                let n = self
                    .db(|st| {
                        let loc = st.get_location(m.location_id)?;
                        st.count_pending_between(loc.log_id, LOTW.status_key, std::slice::from_ref(&m.callsign), Some(m.location_id), a, b)
                    })
                    .unwrap_or(0);
                json!({"mapping": m, "waiting": n})
            })
            .collect();
        Ok(json!({ "locations": rows }))
    }

    pub fn new(store: Arc<Mutex<Store>>, hub: Arc<Hub>, secret_service: String, data_dir: PathBuf, endpoints: Endpoints) -> Arc<Self> {
        let config = hub.setting("qsl").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
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

    /// Uploads to QRZ and Club Log every `interval_min` minutes.
    pub fn start(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(q) = me.upgrade() else { return };
                let minutes = q.config().interval_min.clamp(1, 24 * 60);
                drop(q);
                tokio::time::sleep(Duration::from_secs(60 * minutes as u64)).await;
                let Some(q) = me.upgrade() else { return };
                let cfg = q.config();
                if cfg.qrz_enabled {
                    q.upload("qrz").await;
                }
                if cfg.clublog_enabled {
                    q.upload("clublog").await;
                }
                if cfg.eqsl_enabled {
                    q.upload("eqsl").await;
                }
                let now = Utc::now().timestamp();
                if cfg.confirm_daily && now - q.lock().last_auto_download > 24 * 3600 {
                    q.lock().last_auto_download = now;
                    if !cfg.lotw_username.is_empty() {
                        q.download("lotw").await;
                    }
                    if !cfg.eqsl_username.is_empty() {
                        q.download("eqsl").await;
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
                "clublog_app_key": self.secret("clublog-app-key").is_some(),
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

    /// Uploads everything pending to QRZ or Club Log.
    pub async fn upload(&self, name: &str) -> Run {
        let svc = match name {
            "qrz" => &QRZ,
            "clublog" => &CLUBLOG,
            "eqsl" => &EQSL,
            _ => return Run { error: Some(format!("unknown service {name}")), ..Run::default() },
        };
        let _busy = self.busy.lock().await;
        self.set_run(svc.name, Run { running: true, at: Utc::now().timestamp(), ..Run::default() });
        let run = self.upload_service(svc).await;
        self.set_run(svc.name, run.clone());
        run
    }

    async fn upload_service(&self, svc: &Service) -> Run {
        let mut run = Run { at: Utc::now().timestamp(), ..Run::default() };
        let cfg = self.config();
        let (calls, date) = match svc.name {
            "qrz" => (&cfg.qrz_calls, &cfg.qrz_since),
            "eqsl" => (&cfg.eqsl_calls, &cfg.eqsl_since),
            _ => (&cfg.clublog_calls, &cfg.clublog_since),
        };
        let eqsl_password = self.secret("eqsl-password");
        let (password, app_key) = (self.secret("clublog-password"), self.secret("clublog-app-key"));
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
                        run.error = Some("Club Log needs your email, password and an API key".into());
                        return run;
                    }
                }
            };
            let pending = self
                .db(|st| {
                    let mut all = Vec::new();
                    for log in st.list_logs()? {
                        all.extend(st.pending_uploads(log.id, svc.status_key, std::slice::from_ref(call), None, since(date), BATCH)?);
                    }
                    Ok(all)
                })
                .unwrap_or_default();
            let mut changed_logs = HashSet::new();
            for qso in pending {
                if self.lock().refused.contains(&(svc.name, qso.id)) {
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
    pub async fn download(&self, name: &str) -> Download {
        let name: &'static str = match name {
            "lotw" => "lotw",
            "eqsl" => "eqsl",
            _ => return Download { error: Some(format!("unknown service {name}")), ..Download::default() },
        };
        let _busy = self.busy.lock().await;
        self.set_download(name, Download { running: true, at: Utc::now().timestamp(), ..Download::default() });
        let d = self.download_service(name).await;
        self.set_download(name, d.clone());
        d
    }

    async fn download_service(&self, name: &'static str) -> Download {
        let mut d = Download { at: Utc::now().timestamp(), ..Download::default() };
        let cfg = self.config();
        let (service, records, next_since) = if name == "lotw" {
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
        } else {
            let Some(password) = self.secret("eqsl-password").filter(|_| !cfg.eqsl_username.is_empty()) else {
                d.error = Some("enter your eQSL username and password first".into());
                return d;
            };
            let nick = Some(cfg.eqsl_nickname.as_str()).filter(|n| !n.is_empty());
            match confirm::eqsl_confirmations(&self.endpoints.eqsl_inbox, &cfg.eqsl_username, &password, nick, &cfg.eqsl_rcvd_since).await {
                Ok(r) => (ConfirmService::Eqsl, r, Some(Utc::now().format("%Y-%m-%d").to_string())),
                Err(e) => {
                    d.error = Some(e.to_string());
                    return d;
                }
            }
        };
        let updates: Vec<_> = records
            .iter()
            .map(|r| {
                let u = confirm::confirmation_updates(service, r);
                (r, u.set, u.fill)
            })
            .collect();
        let stats = self.db(|st| {
            let logs: Vec<i64> = st.list_logs()?.iter().map(|l| l.id).collect();
            st.apply_confirmations(&logs, updates, confirm::same_qso)
        });
        match stats {
            Ok(s) => {
                d.received = s.received;
                d.confirmed = s.new;
                d.unmatched_count = s.unmatched.len();
                d.unmatched = s.unmatched.into_iter().take(50).collect();
            }
            Err(e) => {
                d.error = Some(e.to_string());
                return d;
            }
        }
        if let Some(next) = next_since {
            let mut c = self.config();
            if name == "lotw" {
                c.lotw_rcvd_since = next;
            } else {
                c.eqsl_rcvd_since = next;
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
    /// With `range` (first and last day, YYYY-MM-DD, both included) it sends every QSO
    /// in that range that isn't marked sent, whatever the "QSOs from" date says.
    pub async fn upload_lotw(&self, range: Option<(&str, &str)>) -> Run {
        let bounds = match range.map(|(a, b)| day_range(a, b)).transpose() {
            Ok(b) => b,
            Err(e) => return Run { error: Some(e), at: Utc::now().timestamp(), ..Run::default() },
        };
        let _busy = self.busy.lock().await;
        self.set_run(LOTW.name, Run { running: true, at: Utc::now().timestamp(), ..Run::default() });
        let run = self.lotw_run(bounds).await;
        self.set_run(LOTW.name, run.clone());
        run
    }

    async fn lotw_run(&self, bounds: Option<(i64, i64)>) -> Run {
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
            let job = TqslJob { tqsl_path: tqsl.clone(), station_location: m.station_location.clone(), adif_path: file.clone() };
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
                run.error = Some(format!("{} at {}: {} (TQSL code {code})", m.callsign, m.station_location, outcome.message));
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

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct SecretsUpdate {
    /// API keys by station callsign; an empty key removes it.
    pub qrz_keys: BTreeMap<String, String>,
    pub clublog_password: Option<String>,
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
    fn day_ranges() {
        assert_eq!(day_range("2024-01-02", "2024-01-02"), Ok((1_704_153_600, 1_704_153_600 + 86_400)));
        assert_eq!(day_range("2024-01-02", "2024-01-04").unwrap().1, 1_704_153_600 + 3 * 86_400);
        assert!(day_range("2024-01-03", "2024-01-02").is_err());
        assert!(day_range("x", "2024-01-02").is_err());
    }
}
