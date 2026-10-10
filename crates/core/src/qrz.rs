//! QRZ.com XML callsign lookup (https://xmldata.qrz.com).
//!
//! Requires a QRZ XML subscription. Results are mapped to ADIF field names so
//! they can be dropped straight into a QSO.

use std::collections::BTreeMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::adif::Fields;
use crate::error::{Error, Result};

pub const DEFAULT_ENDPOINT: &str = "https://xmldata.qrz.com/xml/current/";

pub struct QrzClient {
    http: reqwest::Client,
    endpoint: String,
    username: String,
    password: String,
    session_key: Option<String>,
    /// When and why the last login was refused. Until `LOGIN_BACKOFF` has passed, lookups fail with
    /// the same message without asking QRZ again, so a wrong password isn't retried on every call.
    login_failure: Option<(std::time::Instant, String)>,
}

/// How long a refused login is remembered. A new client (made when the credentials change) starts clean.
const LOGIN_BACKOFF: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Default)]
struct Response {
    key: Option<String>,
    error: Option<String>,
    callsign: BTreeMap<String, String>,
}

impl QrzClient {
    pub fn new(endpoint: &str, username: &str, password: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent(concat!("QRZero/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client");
        QrzClient {
            http,
            endpoint: endpoint.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            session_key: None,
            login_failure: None,
        }
    }

    pub fn same_credentials(&self, username: &str, password: &str) -> bool {
        self.username == username && self.password == password
    }

    async fn login(&mut self) -> Result<String> {
        if let Some((at, msg)) = &self.login_failure {
            if at.elapsed() < LOGIN_BACKOFF {
                return Err(Error::Lookup(msg.clone()));
            }
        }
        let resp = self
            .get(&[
                ("username", self.username.as_str()),
                ("password", self.password.as_str()),
                ("agent", concat!("QRZero-", env!("CARGO_PKG_VERSION"))),
            ])
            .await?;
        match resp.key {
            Some(key) => {
                self.session_key = Some(key.clone());
                self.login_failure = None;
                Ok(key)
            }
            None => {
                let msg = resp.error.unwrap_or_else(|| "QRZ login failed".into());
                self.login_failure = Some((std::time::Instant::now(), msg.clone()));
                Err(Error::Lookup(msg))
            }
        }
    }

    /// Checks the username and password by logging in.
    pub async fn test_login(&mut self) -> Result<()> {
        self.session_key = None;
        self.login_failure = None;
        self.login().await.map(|_| ())
    }

    /// Looks up a callsign. Returns Ok(None) when QRZ has no record of it.
    pub async fn lookup(&mut self, call: &str) -> Result<Option<Fields>> {
        for attempt in 0..2 {
            let key = match &self.session_key {
                Some(k) => k.clone(),
                None => self.login().await?,
            };
            let resp = self.get(&[("s", key.as_str()), ("callsign", call)]).await?;
            if !resp.callsign.is_empty() {
                return Ok(Some(to_adif(&resp.callsign)));
            }
            let err = resp.error.unwrap_or_default();
            if err.starts_with("Not found") {
                return Ok(None);
            }
            // Expired or invalid session: log in again once.
            if resp.key.is_none() || err.contains("Session") || err.contains("Invalid session") {
                self.session_key = None;
                if attempt == 0 {
                    continue;
                }
            }
            return Err(Error::Lookup(if err.is_empty() { "QRZ returned no data".into() } else { err }));
        }
        unreachable!()
    }

    async fn get(&self, query: &[(&str, &str)]) -> Result<Response> {
        let text = self
            .http
            .get(&self.endpoint)
            .query(query)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::Lookup(format!("QRZ request failed: {e}")))?
            .text()
            .await
            .map_err(|e| Error::Lookup(format!("QRZ response: {e}")))?;
        parse_response(&text)
    }
}

fn parse_response(xml: &str) -> Result<Response> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Response::default();
    let mut path: Vec<String> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => path.push(String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase()),
            Ok(Event::End(_)) => {
                path.pop();
            }
            Ok(Event::Text(t)) => {
                let text = t
                    .decode()
                    .map(|c| c.into_owned())
                    .unwrap_or_default();
                let text = quick_xml::escape::unescape(&text)
                    .map(|c| c.into_owned())
                    .unwrap_or(text);
                let n = path.len();
                if n >= 2 {
                    let (parent, name) = (&path[n - 2], &path[n - 1]);
                    match (parent.as_str(), name.as_str()) {
                        ("session", "key") => out.key = Some(text),
                        ("session", "error") => out.error = Some(text),
                        ("callsign", field) => {
                            out.callsign.insert(field.to_string(), text);
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Lookup(format!("bad QRZ XML: {e}"))),
            _ => {}
        }
    }
    Ok(out)
}

/// Maps QRZ's XML field names to ADIF fields.
fn to_adif(q: &BTreeMap<String, String>) -> Fields {
    let mut f = Fields::new();
    let mut put = |k: &str, v: Option<&String>| {
        if let Some(v) = v.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            f.insert(k.to_string(), v.to_string());
        }
    };
    put("CALL", q.get("call"));
    let full_name = q.get("name_fmt").cloned().or_else(|| {
        let joined = format!(
            "{} {}",
            q.get("fname").map(String::as_str).unwrap_or(""),
            q.get("name").map(String::as_str).unwrap_or("")
        );
        Some(joined.trim().to_string())
    });
    put("NAME", full_name.as_ref());
    put("QTH", q.get("addr2"));
    put("STATE", q.get("state"));
    put("CNTY", q.get("county"));
    put("COUNTRY", q.get("country").or(q.get("land")));
    put("DXCC", q.get("dxcc"));
    put("CQZ", q.get("cqzone"));
    put("ITUZ", q.get("ituzone"));
    put("GRIDSQUARE", q.get("grid"));
    put("IOTA", q.get("iota"));
    put("EMAIL", q.get("email"));
    put("QSL_VIA", q.get("qslmgr"));
    // Extra details for display only; they start with "QRZ_" and are not ADIF.
    for (k, v) in [("QRZ_NICKNAME", "nickname"), ("QRZ_IMAGE", "image"), ("QRZ_LOTW", "lotw"), ("QRZ_EQSL", "eqsl"), ("QRZ_MQSL", "mqsl")] {
        put(k, q.get(v));
    }
    if let Some(lat) = q.get("lat").and_then(|v| v.parse::<f64>().ok()) {
        f.insert("LAT".into(), adif_coord(lat, 'N', 'S'));
    }
    if let Some(lon) = q.get("lon").and_then(|v| v.parse::<f64>().ok()) {
        f.insert("LON".into(), adif_coord(lon, 'E', 'W'));
    }
    f
}

/// ADIF location format: XDDD MM.MMM
fn adif_coord(v: f64, pos: char, neg: char) -> String {
    let hemi = if v < 0.0 { neg } else { pos };
    let v = v.abs();
    let deg = v.trunc();
    let min = (v - deg) * 60.0;
    format!("{hemi}{:03} {:06.3}", deg as i64, min)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
  <Callsign>
    <call>AA7BQ</call><fname>FRED L</fname><name>LLOYD</name>
    <addr2>MESA</addr2><state>AZ</state><country>United States</country>
    <lat>34.23456</lat><lon>-112.34356</lon><grid>DM32af</grid><county>Maricopa</county>
    <dxcc>291</dxcc><cqzone>3</cqzone><ituzone>6</ituzone><lotw>1</lotw>
  </Callsign>
  <Session><Key>2331uf894c4bd29f3923f3bacf02c532d7bd9</Key><Count>123</Count></Session>
</QRZDatabase>"#;

    #[test]
    fn parses_callsign_record() {
        let r = parse_response(SAMPLE).unwrap();
        assert_eq!(r.key.as_deref(), Some("2331uf894c4bd29f3923f3bacf02c532d7bd9"));
        let f = to_adif(&r.callsign);
        assert_eq!(f["CALL"], "AA7BQ");
        assert_eq!(f["NAME"], "FRED L LLOYD");
        assert_eq!(f["QTH"], "MESA");
        assert_eq!(f["DXCC"], "291");
        assert_eq!(f["GRIDSQUARE"], "DM32af");
        assert_eq!(f["LAT"], "N034 14.074");
        assert_eq!(f["LON"], "W112 20.614");
    }

    #[test]
    fn parses_error() {
        let r = parse_response(
            r#"<QRZDatabase><Session><Error>Not found: XX1XX</Error></Session></QRZDatabase>"#,
        )
        .unwrap();
        assert_eq!(r.error.as_deref(), Some("Not found: XX1XX"));
        assert!(r.key.is_none());
    }
}
