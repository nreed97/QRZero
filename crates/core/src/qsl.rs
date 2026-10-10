//! QSL-service uploads: QRZ.com Logbook API, Club Log realtime API, and
//! TQSL (LoTW) command building.
//!
//! The HTTP clients take their endpoint as a parameter so tests can point them
//! at a local mock server. Forms are encoded by hand because reqwest is built
//! without its "form" feature.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::adif::{is_standard_field, write_record, Fields};

/// Production QRZ.com Logbook API endpoint.
pub const QRZ_LOGBOOK_ENDPOINT: &str = "https://logbook.qrz.com/api";
/// Production Club Log realtime upload endpoint.
pub const CLUBLOG_ENDPOINT: &str = "https://clublog.org/realtime.php";
/// User-Agent sent to QSL services.
pub const USER_AGENT: &str = concat!("QRZero/", env!("CARGO_PKG_VERSION"));

/// Why an upload could not be done at all.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum QslError {
    /// Bad credentials or API key; stop the batch.
    #[error("authentication failed: {0}")]
    Auth(String),
    /// The service could not be reached.
    #[error("network error: {0}")]
    Network(String),
    /// The service replied with something unexpected.
    #[error("unexpected reply: {0}")]
    Service(String),
}

/// Result of uploading one QSO.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Upload {
    /// The QSO is now in the service's log.
    Added,
    /// The service already had this QSO.
    Duplicate,
    /// The service refused this QSO, with its reason.
    Rejected(String),
}

/// One ADIF record (no header) holding the standard ADIF fields of `qso`, minus
/// our own upload bookkeeping (`*_QSO_UPLOAD_STATUS` and `*_QSO_UPLOAD_DATE`).
pub fn upload_record(qso: &Fields) -> String {
    let mut out = String::new();
    write_record(&mut out, qso, |k| is_standard_field(k) && !k.ends_with("_QSO_UPLOAD_STATUS") && !k.ends_with("_QSO_UPLOAD_DATE"));
    out
}

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
        .expect("http client")
}

/// Encodes `pairs` as application/x-www-form-urlencoded.
pub(crate) fn form_encode(pairs: &[(&str, &str)]) -> String {
    fn enc(out: &mut String, s: &str) {
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
    }
    let mut out = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        enc(&mut out, k);
        out.push('=');
        enc(&mut out, v);
    }
    out
}

pub(crate) fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(v) => {
                    out.push(v);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses a form-encoded reply into (upper-case key, value) pairs.
fn form_decode(s: &str) -> Vec<(String, String)> {
    s.trim()
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k).to_ascii_uppercase(), url_decode(v))
        })
        .collect()
}

fn get<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Client for the QRZ.com Logbook API (needs the logbook's API key).
pub struct QrzLogbook {
    http: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl QrzLogbook {
    /// A client for `endpoint` (normally [`QRZ_LOGBOOK_ENDPOINT`]).
    pub fn new(endpoint: &str, api_key: &str) -> Self {
        QrzLogbook { http: http_client(), endpoint: endpoint.to_string(), api_key: api_key.trim().to_string() }
    }

    /// Checks the key; returns the logbook's callsign.
    pub async fn status(&self) -> Result<String, QslError> {
        let r = self.post(&[("KEY", &self.api_key), ("ACTION", "STATUS")]).await?;
        match get(&r, "RESULT") {
            Some("OK") => {
                // CALLSIGN may be top-level or inside an encoded DATA value.
                let data = get(&r, "DATA").map(form_decode).unwrap_or_default();
                get(&r, "CALLSIGN")
                    .or_else(|| get(&data, "CALLSIGN"))
                    .map(str::to_string)
                    .ok_or_else(|| QslError::Service("QRZ status reply has no callsign".into()))
            }
            _ => Err(failure(&r)),
        }
    }

    /// Uploads one QSO; `replace` overwrites a matching QSO already in the logbook.
    pub async fn upload(&self, qso: &Fields, replace: bool) -> Result<Upload, QslError> {
        let adif = upload_record(qso);
        let mut form = vec![("KEY", self.api_key.as_str()), ("ACTION", "INSERT"), ("ADIF", adif.as_str())];
        if replace {
            form.push(("OPTION", "REPLACE"));
        }
        let r = self.post(&form).await?;
        match get(&r, "RESULT") {
            Some("OK") | Some("REPLACE") => Ok(Upload::Added),
            Some("FAIL") => {
                let reason = get(&r, "REASON").unwrap_or("").to_string();
                let lower = reason.to_ascii_lowercase();
                if lower.contains("duplicate") {
                    Ok(Upload::Duplicate)
                } else if lower.contains("api key") || lower.contains("invalid key") || lower.contains("access denied") {
                    Err(QslError::Auth(reason))
                } else {
                    Ok(Upload::Rejected(if reason.is_empty() { "rejected by QRZ".into() } else { reason }))
                }
            }
            _ => Err(failure(&r)),
        }
    }

    async fn post(&self, form: &[(&str, &str)]) -> Result<Vec<(String, String)>, QslError> {
        let resp = self
            .http
            .post(&self.endpoint)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form_encode(form))
            .send()
            .await
            .map_err(|e| QslError::Network(format!("QRZ: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| QslError::Network(format!("QRZ: {e}")))?;
        if !status.is_success() {
            return Err(QslError::Service(format!("QRZ HTTP {status}: {}", text.trim())));
        }
        Ok(form_decode(&text))
    }
}

/// Maps a non-OK QRZ reply to an error.
fn failure(r: &[(String, String)]) -> QslError {
    let reason = get(r, "REASON").unwrap_or("").to_string();
    match get(r, "RESULT") {
        Some("AUTH") => QslError::Auth(if reason.is_empty() { "invalid QRZ API key".into() } else { reason }),
        Some("FAIL") if reason.to_ascii_lowercase().contains("key") => QslError::Auth(reason),
        Some(res) => QslError::Service(format!("QRZ {res}: {reason}")),
        None => QslError::Service("QRZ reply has no RESULT".into()),
    }
}

/// Client for the Club Log realtime upload API.
pub struct ClubLog {
    http: reqwest::Client,
    endpoint: String,
    email: String,
    password: String,
    callsign: String,
    app_key: String,
}

impl ClubLog {
    /// A client for `endpoint` (normally [`CLUBLOG_ENDPOINT`]). `callsign` is the
    /// log's callsign on Club Log; `app_key` is the application API key.
    pub fn new(endpoint: &str, email: &str, password: &str, callsign: &str, app_key: &str) -> Self {
        ClubLog {
            http: http_client(),
            endpoint: endpoint.to_string(),
            email: email.trim().to_string(),
            password: password.to_string(),
            callsign: callsign.trim().to_string(),
            app_key: app_key.trim().to_string(),
        }
    }

    /// Uploads one QSO.
    pub async fn upload(&self, qso: &Fields) -> Result<Upload, QslError> {
        let adif = upload_record(qso);
        let body = form_encode(&[
            ("email", &self.email),
            ("password", &self.password),
            ("callsign", &self.callsign),
            ("api", &self.app_key),
            ("adif", &adif),
        ]);
        let resp = self
            .http
            .post(&self.endpoint)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| QslError::Network(format!("Club Log: {e}")))?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(|e| QslError::Network(format!("Club Log: {e}")))?;
        let text = text.trim().to_string();
        match status {
            200..=299 => Ok(Upload::Added),
            400 if text.to_ascii_lowercase().contains("dupe") || text.to_ascii_lowercase().contains("duplicate") => {
                Ok(Upload::Duplicate)
            }
            400 => Ok(Upload::Rejected(if text.is_empty() { "rejected by Club Log".into() } else { text })),
            403 => Err(QslError::Auth(if text.is_empty() { "Club Log refused the login".into() } else { text })),
            _ => Err(QslError::Service(format!("Club Log HTTP {status}: {text}"))),
        }
    }
}

/// A TQSL sign-and-upload run for LoTW.
#[derive(Debug, Clone)]
pub struct TqslJob {
    pub tqsl_path: PathBuf,
    /// Station location name as defined in TQSL.
    pub station_location: String,
    /// Let the MY_* fields in the file (state, county, grid, zones) override the station
    /// location's details, per QSO (`-f update`).
    pub use_log_qth: bool,
    /// ADIF file to sign and upload.
    pub adif_path: PathBuf,
}

impl TqslJob {
    /// Arguments for an unattended sign-and-upload:
    /// `-d` (no date-range dialog), `-q` (quiet), `-x` (exit when done),
    /// `-a compliant` (sign only valid QSOs, skipping already-signed dupes and
    /// out-of-range ones instead of aborting), `-l <location>`, `-f update` (QTH from the log, if asked), `-u` (upload), `<file>`.
    pub fn args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = ["-d", "-q", "-x", "-a", "compliant", "-l"].iter().map(OsString::from).collect();
        a.push(OsString::from(&self.station_location));
        if self.use_log_qth {
            a.push(OsString::from("-f"));
            a.push(OsString::from("update"));
        }
        a.push(OsString::from("-u"));
        a.push(self.adif_path.clone().into_os_string());
        a
    }
}

/// What a TQSL exit code means.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TqslOutcome {
    pub code: i32,
    /// The QSOs given are now on LoTW, or already were.
    pub ok: bool,
    /// Something new was uploaded.
    pub uploaded: bool,
    pub message: &'static str,
}

/// Meaning of TQSL's exit codes.
pub fn tqsl_outcome(code: i32) -> TqslOutcome {
    let message = match code {
        0 => "Uploaded to LoTW",
        1 => "Cancelled",
        2 => "Rejected by LoTW",
        3 => "Unexpected response from LoTW",
        4 => "TQSL error",
        5 => "TQSLlib error",
        6 => "Unable to open the input file",
        7 => "Unable to open the output file",
        8 => "All QSOs were duplicates or out of date range",
        9 => "Some QSOs were duplicates or out of date range; the rest were uploaded",
        10 => "TQSL command syntax error",
        11 => "LoTW connection error",
        _ => "Unknown TQSL exit code",
    };
    TqslOutcome { code, ok: matches!(code, 0 | 8 | 9), uploaded: matches!(code, 0 | 9), message }
}

/// The usual TQSL install locations; returns the first that exists.
pub fn find_tqsl() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = if cfg!(windows) {
        &["C:\\Program Files (x86)\\TrustedQSL\\tqsl.exe", "C:\\Program Files\\TrustedQSL\\tqsl.exe"]
    } else if cfg!(target_os = "macos") {
        &["/Applications/TrustedQSL/tqsl.app/Contents/MacOS/tqsl", "/Applications/tqsl.app/Contents/MacOS/tqsl"]
    } else {
        &["/usr/bin/tqsl", "/usr/local/bin/tqsl"]
    };
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.is_file())
}

/// TQSL's configuration directory: `$TQSLDIR` if set, else `%APPDATA%\TrustedQSL`
/// on Windows and `~/.tqsl` elsewhere.
fn tqsl_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("TQSLDIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| Path::new(&a).join("TrustedQSL"))
    } else {
        std::env::var_os("HOME").map(|h| Path::new(&h).join(".tqsl"))
    }
}

/// Station locations defined in TQSL, from its `station_data` file. Empty when
/// the file isn't there.
pub fn tqsl_station_locations() -> Vec<String> {
    tqsl_dir()
        .and_then(|d| std::fs::read_to_string(d.join("station_data")).ok())
        .map(|xml| parse_station_data(&xml))
        .unwrap_or_default()
}

/// Names of the `<StationData name="...">` elements in a TQSL station_data file.
pub fn parse_station_data(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut names = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref().eq_ignore_ascii_case(b"StationData") => {
                for attr in e.attributes().flatten() {
                    if attr.key.local_name().as_ref() == b"name" {
                        if let Ok(v) = attr.unescape_value() {
                            if !v.is_empty() {
                                names.push(v.into_owned());
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::StatusCode, routing::post, Router};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<HashMap<String, String>>>>;

    /// Starts a mock server whose handler maps the decoded form to (status, body).
    async fn mock(reply: fn(&HashMap<String, String>) -> (u16, String)) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/",
                post(move |State(seen): State<Seen>, axum::Form(form): axum::Form<HashMap<String, String>>| async move {
                    let (code, body) = reply(&form);
                    seen.lock().unwrap().push(form);
                    (StatusCode::from_u16(code).unwrap(), body)
                }),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/"), seen)
    }

    fn qso(call: &str) -> Fields {
        [("CALL", call), ("QSO_DATE", "20240101"), ("TIME_ON", "1200"), ("BAND", "20M"), ("MODE", "CW")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn qrz_reply(f: &HashMap<String, String>) -> (u16, String) {
        if f["KEY"] != "GOOD" {
            return (200, "RESULT=AUTH&REASON=invalid api key".into());
        }
        let body = match f["ACTION"].as_str() {
            "STATUS" => "RESULT=OK&CALLSIGN=K1ABC&COUNT=42".into(),
            _ if f["ADIF"].contains("<CALL:4>DUPE") => {
                "RESULT=FAIL&REASON=Unable+to+add+QSO+to+database%3A+duplicate&EXTENDED=".into()
            }
            _ if f["ADIF"].contains("<CALL:3>BAD") => "RESULT=FAIL&REASON=invalid+band".into(),
            _ => "RESULT=OK&LOGID=123&COUNT=1".into(),
        };
        (200, body)
    }

    #[tokio::test]
    async fn qrz_insert_duplicate_rejected() {
        let (url, seen) = mock(qrz_reply).await;
        let c = QrzLogbook::new(&url, "GOOD");
        assert_eq!(c.upload(&qso("W1AW"), false).await, Ok(Upload::Added));
        assert_eq!(c.upload(&qso("DUPE"), false).await, Ok(Upload::Duplicate));
        assert_eq!(c.upload(&qso("BAD"), false).await, Ok(Upload::Rejected("invalid band".into())));
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0]["ACTION"], "INSERT");
        assert!(!seen[0].contains_key("OPTION"));
        assert!(seen[0]["ADIF"].starts_with("<CALL:4>W1AW "));
    }

    #[tokio::test]
    async fn qrz_auth_failure() {
        let (url, _) = mock(qrz_reply).await;
        let c = QrzLogbook::new(&url, "WRONG");
        assert!(matches!(c.upload(&qso("W1AW"), false).await, Err(QslError::Auth(_))));
        assert!(matches!(c.status().await, Err(QslError::Auth(_))));
    }

    #[tokio::test]
    async fn qrz_replace_and_status() {
        let (url, seen) = mock(qrz_reply).await;
        let c = QrzLogbook::new(&url, "GOOD");
        assert_eq!(c.upload(&qso("W1AW"), true).await, Ok(Upload::Added));
        assert_eq!(seen.lock().unwrap()[0]["OPTION"], "REPLACE");
        assert_eq!(c.status().await.unwrap(), "K1ABC");
    }

    #[tokio::test]
    async fn qrz_form_encoding_round_trips() {
        let (url, seen) = mock(qrz_reply).await;
        let mut q = qso("W1AW");
        let comment = "a&b=c d+e %20 Grüße 73!";
        q.insert("COMMENT".into(), comment.into());
        QrzLogbook::new(&url, "GOOD").upload(&q, false).await.unwrap();
        let adif = seen.lock().unwrap()[0]["ADIF"].clone();
        assert_eq!(adif, upload_record(&q));
        assert!(adif.contains(comment));
    }

    #[tokio::test]
    async fn qrz_network_error() {
        let c = QrzLogbook::new("http://127.0.0.1:1/", "GOOD");
        assert!(matches!(c.status().await, Err(QslError::Network(_))));
    }

    fn clublog_reply(f: &HashMap<String, String>) -> (u16, String) {
        assert_eq!((f["email"].as_str(), f["callsign"].as_str(), f["api"].as_str()), ("me@x.org", "K1ABC", "APPKEY"));
        if f["password"] != "pw" {
            return (403, "Invalid login".into());
        }
        let adif = &f["adif"];
        if adif.contains("<CALL:4>DUPE") {
            (400, "Dupe".into())
        } else if adif.contains("<CALL:3>BAD") {
            (400, "Rejected: invalid QSO date".into())
        } else if adif.contains("<CALL:4>BOOM") {
            (500, "oops".into())
        } else {
            (200, "QSO OK".into())
        }
    }

    #[tokio::test]
    async fn clublog_responses() {
        let (url, seen) = mock(clublog_reply).await;
        let c = ClubLog::new(&url, "me@x.org", "pw", "K1ABC", "APPKEY");
        assert_eq!(c.upload(&qso("W1AW")).await, Ok(Upload::Added));
        assert_eq!(c.upload(&qso("DUPE")).await, Ok(Upload::Duplicate));
        assert_eq!(c.upload(&qso("BAD")).await, Ok(Upload::Rejected("Rejected: invalid QSO date".into())));
        assert!(matches!(c.upload(&qso("BOOM")).await, Err(QslError::Service(_))));
        assert_eq!(seen.lock().unwrap()[0]["adif"], upload_record(&qso("W1AW")));
        let bad = ClubLog::new(&url, "me@x.org", "nope", "K1ABC", "APPKEY");
        assert!(matches!(bad.upload(&qso("W1AW")).await, Err(QslError::Auth(_))));
    }

    #[test]
    fn upload_record_drops_non_standard_fields() {
        let mut q = qso("W1AW");
        q.insert("APP_QRZERO_ID".into(), "7".into());
        q.insert("MY_PRIVATE".into(), "x".into());
        let r = upload_record(&q);
        assert!(r.contains("<CALL:4>W1AW") && r.contains("<MODE:2>CW") && r.ends_with("<EOR>\n"));
        assert!(!r.contains("APP_QRZERO_ID") && !r.contains("MY_PRIVATE") && !r.contains("<EOH>"));
    }

    #[test]
    fn form_codec() {
        assert_eq!(form_encode(&[("a b", "x&y=z/é")]), "a+b=x%26y%3Dz%2F%C3%A9");
        assert_eq!(form_decode("RESULT=OK&reason=a+b%3Dc%"), vec![
            ("RESULT".into(), "OK".into()),
            ("REASON".into(), "a b=c%".into())
        ]);
    }

    #[test]
    fn tqsl_args_and_outcomes() {
        let job = TqslJob {
            tqsl_path: "tqsl".into(),
            station_location: "Home QTH".into(),
            use_log_qth: false,
            adif_path: "/tmp/up load.adi".into(),
        };
        let args: Vec<String> = job.args().into_iter().map(|a| a.into_string().unwrap()).collect();
        assert_eq!(args, ["-d", "-q", "-x", "-a", "compliant", "-l", "Home QTH", "-u", "/tmp/up load.adi"]);
        let job = TqslJob { use_log_qth: true, ..job };
        let args: Vec<String> = job.args().into_iter().map(|a| a.into_string().unwrap()).collect();
        assert_eq!(args[5..], ["-l", "Home QTH", "-f", "update", "-u", "/tmp/up load.adi"]);
        let o = |c| {
            let t = tqsl_outcome(c);
            (t.ok, t.uploaded)
        };
        assert_eq!(o(0), (true, true));
        assert_eq!(o(8), (true, false));
        assert_eq!(o(9), (true, true));
        for c in [1, 2, 3, 4, 5, 6, 7, 10, 11, 99, -1] {
            assert_eq!(o(c), (false, false), "code {c}");
        }
        assert_eq!(tqsl_outcome(11).message, "LoTW connection error");
        assert_eq!(tqsl_outcome(42).message, "Unknown TQSL exit code");
    }

    #[test]
    fn parses_station_data() {
        let xml = r#"<?xml version="1.0"?>
<StationDataFile>
<StationData name="Home">
<CALL>K1ABC</CALL><DXCC>291</DXCC><GRIDSQUARE>FN42</GRIDSQUARE>
</StationData>
<StationData name="Portable &amp; /P">
<CALL>K1ABC/P</CALL>
</StationData>
</StationDataFile>"#;
        assert_eq!(parse_station_data(xml), ["Home", "Portable & /P"]);
        assert!(parse_station_data("").is_empty());
        assert!(parse_station_data("not xml <<<").is_empty());
    }
}
