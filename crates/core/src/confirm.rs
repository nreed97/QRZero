//! Downloading QSL confirmations from LoTW and eQSL, uploading to eQSL, and
//! matching downloaded confirmations to QSOs in the log.
//!
//! Like [`crate::qsl`], the HTTP functions take their endpoint as a parameter
//! so tests can point them at a local mock server.

use std::time::Duration;

use chrono::{NaiveDate, NaiveTime};

use crate::adif::{self, Fields};
use crate::band::{band_for_freq, normalize_band};
use crate::qsl::{form_encode, http_client, upload_record, QslError, Upload, USER_AGENT};

/// Production LoTW report endpoint.
pub const LOTW_REPORT_ENDPOINT: &str = "https://lotw.arrl.org/lotwuser/lotwreport.adi";
/// Production eQSL inbox download endpoint.
pub const EQSL_INBOX_ENDPOINT: &str = "https://www.eqsl.cc/qslcard/DownloadInBox.cfm";
/// Production eQSL ADIF upload endpoint.
pub const EQSL_UPLOAD_ENDPOINT: &str = "https://www.eqsl.cc/qslcard/ImportADIF.cfm";

/// Production Club Log log-match endpoint.
pub const CLUBLOG_MATCHES_ENDPOINT: &str = "https://clublog.org/getmatches.php";

/// How far apart the log's and the service's QSO start times may be.
const TIME_SLACK_SECS: i64 = 30 * 60;

/// A confirmation service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Lotw,
    Eqsl,
    /// QRZ Logbook, which also shows the LoTW confirmations it knows of.
    Qrz,
    /// Club Log's log matches (both stations upload to Club Log).
    ClubLog,
}

/// A LoTW confirmation report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LotwReport {
    /// Confirmed QSOs as LoTW reports them.
    pub records: Vec<Fields>,
    /// The header's APP_LOTW_LASTQSL, e.g. "2026-10-01 12:34:56": pass its date
    /// as `since` next time.
    pub last_qsl: Option<String>,
}

/// Fields to apply to a log QSO matched to a downloaded confirmation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfirmUpdate {
    /// Always set (overwrite).
    pub set: Fields,
    /// Set only where the log QSO lacks the field (or has it empty).
    pub fill: Fields,
}

/// Downloads can be large (a full LoTW report), so allow more time than uploads.
fn download_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent(USER_AGENT)
        .build()
        .expect("http client")
}

/// `endpoint` with `pairs` appended as a query string.
fn with_query(endpoint: &str, pairs: &[(&str, &str)]) -> String {
    let sep = if endpoint.contains('?') { '&' } else { '?' };
    format!("{endpoint}{sep}{}", form_encode(pairs))
}

/// GETs `url` and returns the body; non-2xx is a service error.
async fn get_text(http: &reqwest::Client, url: &str, who: &str) -> Result<String, QslError> {
    let resp = http.get(url).send().await.map_err(|e| QslError::Network(format!("{who}: {}", e.without_url())))?;
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| QslError::Network(format!("{who}: {}", e.without_url())))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if !status.is_success() {
        return Err(QslError::Service(format!("{who} HTTP {status}: {}", visible_text(&text))));
    }
    Ok(text)
}

/// Downloads LoTW confirmations (QSL'd QSOs with details) received since
/// `since` (YYYY-MM-DD; empty for all). `own_call` limits the report to QSOs
/// made as that callsign.
pub async fn lotw_confirmations(
    endpoint: &str,
    username: &str,
    password: &str,
    own_call: Option<&str>,
    since: &str,
) -> Result<LotwReport, QslError> {
    let since = since.trim();
    let mut q = vec![
        ("login", username.trim()),
        ("password", password),
        ("qso_query", "1"),
        ("qso_qsl", "yes"),
        ("qso_qsldetail", "yes"),
        ("qso_withown", "yes"),
    ];
    if !since.is_empty() {
        q.push(("qso_qslsince", since));
    }
    if let Some(c) = own_call.map(str::trim).filter(|c| !c.is_empty()) {
        q.push(("qso_owncall", c));
    }
    let text = get_text(&download_client(), &with_query(endpoint, &q), "LoTW").await?;
    if !text.to_ascii_lowercase().contains("<eoh>") {
        let lower = text.to_ascii_lowercase();
        return Err(if lower.contains("password") || lower.contains("login") {
            QslError::Auth("LoTW refused the username or password".into())
        } else {
            QslError::Service(format!("LoTW: {}", visible_text(&text)))
        });
    }
    let file = adif::parse(text.as_bytes());
    let last_qsl = file.header.get("APP_LOTW_LASTQSL").filter(|v| !v.is_empty()).cloned();
    Ok(LotwReport { records: file.records, last_qsl })
}

/// Downloads eQSL inbox confirmations received since `since` (YYYY-MM-DD;
/// empty for all). eQSL replies with an HTML page linking to a generated ADIF
/// file, which is then fetched.
pub async fn eqsl_confirmations(
    endpoint: &str,
    username: &str,
    password: &str,
    qth_nickname: Option<&str>,
    since: &str,
) -> Result<Vec<Fields>, QslError> {
    let since: String = since.chars().filter(char::is_ascii_digit).collect();
    let mut q = vec![("UserName", username.trim()), ("Password", password)];
    if !since.is_empty() {
        q.push(("RcvdSince", &since));
    }
    if let Some(n) = qth_nickname.map(str::trim).filter(|n| !n.is_empty()) {
        q.push(("QTHNickname", n));
    }
    let http = download_client();
    let page = get_text(&http, &with_query(endpoint, &q), "eQSL").await?;
    let Some(href) = adi_link(&page) else {
        let text = visible_text(&page);
        let lower = text.to_ascii_lowercase();
        if lower.contains("no such username") || lower.contains("password") {
            return Err(QslError::Auth(text));
        }
        if lower.contains("no log entries") || lower.contains("no qsls") {
            return Ok(Vec::new());
        }
        return Err(QslError::Service(format!("eQSL: {text}")));
    };
    let base = reqwest::Url::parse(endpoint).map_err(|e| QslError::Service(format!("eQSL endpoint: {e}")))?;
    let url = base.join(&href).map_err(|e| QslError::Service(format!("eQSL file link {href:?}: {e}")))?;
    let adi = get_text(&http, url.as_str(), "eQSL").await?;
    Ok(adif::parse(adi.as_bytes()).records)
}

/// Downloads the QSOs QRZ Logbook reports as confirmed (QRZ.com's own match, or
/// LoTW's, which QRZ honors) for the logbook that `api_key` belongs to. `since`
/// (YYYY-MM-DD; empty for all) limits it to QSOs QRZ changed since then.
pub async fn qrz_confirmations(endpoint: &str, api_key: &str, since: &str) -> Result<Vec<Fields>, QslError> {
    const PAGE: usize = 1000;
    let http = download_client();
    let mut out: Vec<Fields> = Vec::new();
    let mut after = 0u64;
    loop {
        let mut opt = format!("TYPE:ADIF,STATUS:CONFIRMED,MAX:{PAGE}");
        if after > 0 {
            opt.push_str(&format!(",AFTERLOGID:{after}"));
        }
        if let Some(d) = Some(since.trim()).filter(|d| !d.is_empty()) {
            opt.push_str(&format!(",MODSINCE:{d}"));
        }
        let body = form_encode(&[("KEY", api_key.trim()), ("ACTION", "FETCH"), ("OPTION", &opt)]);
        let resp = http
            .post(endpoint)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| QslError::Network(format!("QRZ: {}", e.without_url())))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| QslError::Network(format!("QRZ: {}", e.without_url())))?;
        if !status.is_success() {
            return Err(QslError::Service(format!("QRZ HTTP {status}: {}", visible_text(&text))));
        }
        let (head, adi) = match text.find("ADIF=") {
            Some(i) => (&text[..i], Some(&text[i + 5..])),
            None => (text.as_str(), None),
        };
        let result = head.split('&').find_map(|p| p.strip_prefix("RESULT=")).unwrap_or("");
        let reason = head.split('&').find_map(|p| p.strip_prefix("REASON=")).unwrap_or("").replace('+', " ");
        match result {
            "OK" => {}
            // No QSOs matched.
            "FAIL" if reason.to_ascii_lowercase().contains("no log entries") || reason.to_ascii_lowercase().contains("no matching") => break,
            "AUTH" => return Err(QslError::Auth(if reason.is_empty() { "invalid QRZ API key".into() } else { reason })),
            "FAIL" if reason.to_ascii_lowercase().contains("key") => return Err(QslError::Auth(reason)),
            other => return Err(QslError::Service(format!("QRZ {other}: {reason}"))),
        }
        let records = adi.map(qrz_adif).map(|a| adif::parse(a.as_bytes()).records).unwrap_or_default();
        let n = records.len();
        let top = records.iter().filter_map(|r| r.get("APP_QRZLOG_LOGID").and_then(|v| v.trim().parse::<u64>().ok())).max().unwrap_or(0);
        out.extend(records);
        if n < PAGE || top <= after {
            break;
        }
        after = top;
    }
    Ok(out)
}

/// QRZ sends the ADIF text URL-encoded, or with its angle brackets as HTML entities.
fn qrz_adif(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if !s.contains('<') {
        s = crate::qsl::url_decode(&s);
    }
    if !s.contains('<') {
        s = s.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    }
    s
}

/// Downloads Club Log's log matches for one of the account's callsigns: QSOs Club Log
/// matched against the other station's log. `since` (YYYY-MM-DD; empty for all) is the
/// day the match was made; the day before is included in case a match landed late.
/// Each match comes back as a QSO record with a date, a band and a mode (the time of
/// day is only good to 15 minutes).
pub async fn clublog_matches(
    endpoint: &str,
    email: &str,
    password: &str,
    callsign: &str,
    app_key: &str,
    since: &str,
) -> Result<Vec<Fields>, QslError> {
    let mut q = vec![("api", app_key.trim()), ("email", email.trim()), ("password", password), ("callsign", callsign.trim())];
    let from = NaiveDate::parse_from_str(since.trim(), "%Y-%m-%d").ok().map(|d| d - chrono::Duration::days(1));
    let (y, m, d);
    if let Some(f) = from {
        use chrono::Datelike;
        (y, m, d) = (f.year().to_string(), f.month().to_string(), f.day().to_string());
        q.extend([("startyear", y.as_str()), ("startmonth", m.as_str()), ("startday", d.as_str())]);
    }
    let resp = download_client()
        .get(with_query(endpoint, &q))
        .send()
        .await
        .map_err(|e| QslError::Network(format!("Club Log: {}", e.without_url())))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| QslError::Network(format!("Club Log: {}", e.without_url())))?;
    let text = text.trim();
    if status.as_u16() == 403 || status.as_u16() == 401 {
        let why = visible_text(text);
        return Err(QslError::Auth(if why.is_empty() {
            "Club Log refused the login (matches need an application password)".into()
        } else {
            format!("{why} (matches need a Club Log application password)")
        }));
    }
    if !status.is_success() {
        return Err(QslError::Service(format!("Club Log HTTP {status}: {}", visible_text(text))));
    }
    let rows: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(text).map_err(|_| QslError::Service(format!("Club Log: {}", visible_text(text))))?;
    let col = |r: &[serde_json::Value], i: usize| r.get(i).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())));
    Ok(rows
        .iter()
        .filter_map(|r| {
            let call = col(r, 0)?;
            let when = chrono::NaiveDateTime::parse_from_str(&col(r, 2)?, "%Y-%m-%d %H:%M:%S").ok()?;
            let mut f = Fields::new();
            f.insert("CALL".into(), call);
            f.insert("QSO_DATE".into(), when.format("%Y%m%d").to_string());
            f.insert("TIME_ON".into(), when.format("%H%M%S").to_string());
            f.insert("BAND".into(), clublog_band(&col(r, 3)?)?);
            // A match with no mode (JSON false) can't be matched to a QSO, so it is left out.
            f.insert("MODE".into(), col(r, 4)?);
            Some(f)
        })
        .collect())
}

/// Club Log's band id (a wavelength in metres, or centimetres for the UHF bands) as an ADIF band.
fn clublog_band(id: &str) -> Option<String> {
    let n: u32 = id.trim().parse().ok()?;
    Some(match n {
        222 => "1.25m".to_string(),
        70 | 33 | 23 | 13 => format!("{n}cm"),
        _ => format!("{n}m"),
    })
}

/// Uploads one QSO to eQSL.
pub async fn eqsl_upload(
    endpoint: &str,
    username: &str,
    password: &str,
    qth_nickname: Option<&str>,
    qso: &Fields,
) -> Result<Upload, QslError> {
    let username = username.trim();
    let mut doc = String::from("QRZero upload ");
    push_tag(&mut doc, "PROGRAMID", "QRZero");
    push_tag(&mut doc, "EQSL_USER", username);
    push_tag(&mut doc, "EQSL_PSWD", password);
    doc.push_str("<EOH>\n");
    let mut record = upload_record(qso);
    if let Some(n) = qth_nickname.map(str::trim).filter(|n| !n.is_empty()) {
        let mut tag = String::new();
        push_tag(&mut tag, "APP_EQSL_QTH_NICKNAME", n);
        let at = record.rfind("<EOR>").unwrap_or(record.len());
        record.insert_str(at, &tag);
    }
    doc.push_str(&record);

    let resp = http_client()
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_encode(&[("ADIFData", &doc)]))
        .send()
        .await
        .map_err(|e| QslError::Network(format!("eQSL: {}", e.without_url())))?;
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| QslError::Network(format!("eQSL: {}", e.without_url())))?;
    let text = visible_text(&String::from_utf8_lossy(&bytes));
    if !status.is_success() {
        return Err(QslError::Service(format!("eQSL HTTP {status}: {text}")));
    }
    eqsl_upload_result(&text)
}

/// Interprets the visible text of an eQSL ImportADIF reply.
fn eqsl_upload_result(text: &str) -> Result<Upload, QslError> {
    let lower = text.to_ascii_lowercase();
    if records_added(&lower).is_some_and(|n| n > 0) {
        return Ok(Upload::Added);
    }
    if lower.contains("duplicate") {
        return Ok(Upload::Duplicate);
    }
    if lower.contains("no match on eqsl_user") || lower.contains("password") || lower.contains("eqsl_pswd") {
        return Err(QslError::Auth(cap(text)));
    }
    let reason_at = match lower.find("error:") {
        Some(i) => Some(i),
        None if lower.contains("bad record") => Some(lower.find("warning:").unwrap_or(0)),
        None => None,
    };
    if let Some(i) = reason_at {
        return Ok(Upload::Rejected(cap(&text[i..])));
    }
    Err(QslError::Service(format!("eQSL: {}", cap(text))))
}

/// N from "result: N out of M records added" (lower-cased text).
fn records_added(lower: &str) -> Option<u32> {
    let rest = &lower[lower.find("result:")? + "result:".len()..];
    let mut words = rest.split_whitespace();
    let n = words.next()?.parse().ok()?;
    (words.next()? == "out" && words.next()? == "of").then_some(n)
}

fn push_tag(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("<{name}:{}>{value} ", value.len()));
}

/// The first `href` in `html` that names a `.adi` file.
fn adi_link(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("href") {
        let mut p = from + i + 4;
        from = p;
        let rest = &lower[p..];
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('=') {
            continue;
        }
        p += rest.len() - trimmed.len() + 1;
        let after = &lower[p..];
        p += after.len() - after.trim_start().len();
        let (start, end) = match lower[p..].chars().next()? {
            q @ ('"' | '\'') => (p + 1, lower[p + 1..].find(q).map(|e| p + 1 + e)?),
            _ => (p, lower[p..].find(|c: char| c.is_ascii_whitespace() || c == '>').map_or(lower.len(), |e| p + e)),
        };
        let link = html[start..end].trim();
        if link.to_ascii_lowercase().ends_with(".adi") {
            return Some(link.to_string());
        }
    }
    None
}

/// The text of an HTML page: tags (and script/style contents) removed, common
/// entities decoded, whitespace collapsed, capped at about 200 characters.
fn visible_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        out.push(' ');
        let tag_end = rest[i..].find('>').map_or(rest.len(), |e| i + e + 1);
        let tag = rest[i..tag_end].to_ascii_lowercase();
        rest = &rest[tag_end..];
        for skip in ["script", "style"] {
            if tag.starts_with(&format!("<{skip}")) {
                let close = format!("</{skip}");
                let lower = rest.to_ascii_lowercase();
                rest = lower.find(&close).map_or("", |e| &rest[e..]);
            }
        }
    }
    out.push_str(rest);
    let out = out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&");
    cap(&out.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn cap(s: &str) -> String {
    let s = s.trim();
    match s.char_indices().nth(200) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

// ---- Matching ---------------------------------------------------------------

/// Mode group used for matching: "CW", "PHONE" or "DIGITAL". SUBMODE wins
/// over MODE when present.
fn mode_group(f: &Fields) -> Option<&'static str> {
    let m = nonempty(f, "SUBMODE").or_else(|| nonempty(f, "MODE"))?.to_ascii_uppercase();
    Some(match m.as_str() {
        "CW" | "PCW" => "CW",
        "SSB" | "USB" | "LSB" | "AM" | "FM" | "PHONE" => "PHONE",
        _ => "DIGITAL",
    })
}

fn nonempty<'a>(f: &'a Fields, k: &str) -> Option<&'a str> {
    f.get(k).map(|v| v.trim()).filter(|v| !v.is_empty())
}

/// BAND (normalized, lower-case), else derived from FREQ (MHz).
fn band_of(f: &Fields) -> Option<String> {
    if let Some(b) = nonempty(f, "BAND") {
        return Some(normalize_band(b).map_or_else(|| b.to_ascii_lowercase(), str::to_string));
    }
    nonempty(f, "FREQ")?.parse::<f64>().ok().and_then(band_for_freq).map(str::to_string)
}

/// Unix seconds of QSO_DATE (YYYYMMDD) + TIME_ON (HHMM or HHMMSS), UTC.
fn qso_time(f: &Fields) -> Option<i64> {
    let date = NaiveDate::parse_from_str(nonempty(f, "QSO_DATE")?, "%Y%m%d").ok()?;
    let t = nonempty(f, "TIME_ON")?;
    let time = match t.len() {
        4 => NaiveTime::parse_from_str(t, "%H%M").ok()?,
        6 => NaiveTime::parse_from_str(t, "%H%M%S").ok()?,
        _ => return None,
    };
    Some(date.and_time(time).and_utc().timestamp())
}

/// (CALL upper-case, band lower-case, QSO start in unix seconds, mode group),
/// or None when any part is missing or unparseable.
pub fn confirmation_key(f: &Fields) -> Option<(String, String, i64, &'static str)> {
    let call = nonempty(f, "CALL")?.to_ascii_uppercase();
    Some((call, band_of(f)?, qso_time(f)?, mode_group(f)?))
}

/// Whether `a` and `b` describe the same QSO: same call, band and mode group,
/// with start times within 30 minutes.
pub fn same_qso(a: &Fields, b: &Fields) -> bool {
    match (confirmation_key(a), confirmation_key(b)) {
        (Some((ca, ba, ta, ma)), Some((cb, bb, tb, mb))) => {
            ca == cb && ba == bb && ma == mb && (ta - tb).abs() <= TIME_SLACK_SECS
        }
        _ => false,
    }
}

/// First of `keys` in `f` that holds a YYYYMMDD date.
fn date_field(f: &Fields, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|k| nonempty(f, k))
        .find(|v| NaiveDate::parse_from_str(v, "%Y%m%d").is_ok())
        .map(str::to_string)
}

/// The fields to apply to the log QSO matched to downloaded record `rec`.
pub fn confirmation_updates(service: Service, rec: &Fields) -> ConfirmUpdate {
    let today = || chrono::Utc::now().format("%Y%m%d").to_string();
    let (rcvd, date_key, date_sources, fill_keys): (_, _, &[&str], &[&str]) = match service {
        Service::Lotw => (
            "LOTW_QSL_RCVD",
            "LOTW_QSLRDATE",
            &["QSLRDATE"],
            &["DXCC", "STATE", "CQZ", "ITUZ", "GRIDSQUARE", "CNTY", "IOTA", "PFX"],
        ),
        Service::Eqsl => ("EQSL_QSL_RCVD", "EQSL_QSLRDATE", &["QSLRDATE", "EQSL_QSLRDATE", "RCVD_DATE"], &["GRIDSQUARE"]),
        Service::Qrz => ("QRZCOM_QSO_DOWNLOAD_STATUS", "QRZCOM_QSO_DOWNLOAD_DATE", &["QRZCOM_QSO_DOWNLOAD_DATE"], &["GRIDSQUARE", "STATE", "CQZ", "ITUZ"]),
        Service::ClubLog => ("APP_QRZERO_CLUBLOG_RCVD", "APP_QRZERO_CLUBLOG_RDATE", &[], &[]),
    };
    let mut set = Fields::new();
    // A confirmed QSO is on the service, so it counts as sent too (the date only if blank).
    let (sent, sent_date) = match service {
        Service::Qrz => ("QRZCOM_QSO_UPLOAD_STATUS".to_string(), "QRZCOM_QSO_UPLOAD_DATE".to_string()),
        Service::ClubLog => ("CLUBLOG_QSO_UPLOAD_STATUS".to_string(), "CLUBLOG_QSO_UPLOAD_DATE".to_string()),
        _ => (rcvd.replace("RCVD", "SENT"), date_key.replace("RDATE", "SDATE")),
    };
    set.insert(sent, "Y".into());
    set.insert(rcvd.into(), "Y".into());
    set.insert(date_key.into(), date_field(rec, date_sources).unwrap_or_else(today));
    let mut fill: Fields = fill_keys.iter().filter_map(|k| nonempty(rec, k).map(|v| (k.to_string(), v.to_string()))).collect();
    fill.insert(sent_date, today());
    ConfirmUpdate { set, fill }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_adi_links() {
        assert_eq!(adi_link(r#"<a href="x.html">x</a> <A HREF="../downloadedfiles/ab12.ADI">.ADI file</A>"#).as_deref(), Some("../downloadedfiles/ab12.ADI"));
        assert_eq!(adi_link("<a href = 'f/a.adi'>").as_deref(), Some("f/a.adi"));
        assert_eq!(adi_link("<a href=f/a.adi>").as_deref(), Some("f/a.adi"));
        assert_eq!(adi_link("<a href=\"a.html\">"), None);
        assert_eq!(adi_link("href"), None);
    }

    #[test]
    fn visible_text_strips_markup() {
        let html = "<html><head><style>p{}</style><script>var a='<b>';</script></head><body><P>Error:&nbsp; No  such\nUsername</P></body></html>";
        assert_eq!(visible_text(html), "Error: No such Username");
        assert_eq!(visible_text(&"x ".repeat(300)).chars().count(), 201);
    }

    #[test]
    fn qrz_adif_comes_either_way() {
        assert_eq!(qrz_adif("&lt;CALL:4&gt;W1AW &lt;EOR&gt;"), "<CALL:4>W1AW <EOR>");
        assert_eq!(qrz_adif("%3CCALL%3A4%3EW1AW+%3CEOR%3E"), "<CALL:4>W1AW <EOR>");
        assert_eq!(qrz_adif("<CALL:4>W1AW <EOR>"), "<CALL:4>W1AW <EOR>");
    }

    #[test]
    fn club_log_band_ids() {
        assert_eq!(clublog_band("20").as_deref(), Some("20m"));
        assert_eq!(clublog_band("70").as_deref(), Some("70cm"));
        assert_eq!(clublog_band("222").as_deref(), Some("1.25m"));
        assert_eq!(clublog_band("x"), None);
    }

    #[test]
    fn upload_reply_parsing() {
        assert_eq!(eqsl_upload_result("Result: 1 out of 1 records added"), Ok(Upload::Added));
        assert_eq!(eqsl_upload_result("Result: 0 out of 1 records added Error: Duplicate"), Ok(Upload::Duplicate));
        assert!(matches!(eqsl_upload_result("Error: No match on eQSL_User/eQSL_Pswd"), Err(QslError::Auth(_))));
        assert_eq!(
            eqsl_upload_result("Result: 0 out of 1 records added Warning: Y=2024 Bad record: Bad band"),
            Ok(Upload::Rejected("Warning: Y=2024 Bad record: Bad band".into()))
        );
        assert_eq!(eqsl_upload_result("Error: Bad date"), Ok(Upload::Rejected("Error: Bad date".into())));
        assert!(matches!(eqsl_upload_result("Something else"), Err(QslError::Service(_))));
    }
}
