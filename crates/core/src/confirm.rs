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

/// How far apart the log's and the service's QSO start times may be.
const TIME_SLACK_SECS: i64 = 30 * 60;

/// A confirmation service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Lotw,
    Eqsl,
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
    };
    let mut set = Fields::new();
    // A confirmed QSO is on the service, so it counts as sent too (the date only if blank).
    set.insert(rcvd.replace("RCVD", "SENT"), "Y".into());
    set.insert(rcvd.into(), "Y".into());
    set.insert(date_key.into(), date_field(rec, date_sources).unwrap_or_else(today));
    let mut fill: Fields = fill_keys.iter().filter_map(|k| nonempty(rec, k).map(|v| (k.to_string(), v.to_string()))).collect();
    fill.insert(date_key.replace("RDATE", "SDATE"), today());
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
