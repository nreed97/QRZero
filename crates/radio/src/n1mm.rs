//! N1MM Logger+ UDP broadcasts (`contactinfo`, `contactreplace`, `contactdelete`, `RadioInfo`, `spot`).
//!
//! Each datagram is one small XML document whose root element names the message. Frequencies in
//! contacts and RadioInfo are integers in units of 10 Hz; spot frequencies are kHz.

use quick_xml::events::Event;
use std::collections::BTreeMap;

/// Decoding failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Malformed XML.
    #[error("invalid N1MM XML: {0}")]
    Xml(String),
    /// The datagram has no root element.
    #[error("empty N1MM datagram")]
    Empty,
}

/// A datagram broadcast by N1MM Logger+.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "type")]
pub enum N1mmMessage {
    /// `contactinfo` (new QSO) and `contactreplace` (edited QSO). `fields` are ADIF-named
    /// (CALL, BAND, FREQ, FREQ_RX, MODE, SUBMODE, QSO_DATE, TIME_ON, RST_SENT, ...); empty values are
    /// omitted. `id` is N1MM's QSO id, used to match later replaces and deletes.
    Contact {
        replace: bool,
        id: String,
        fields: BTreeMap<String, String>,
    },
    /// `contactdelete`: the QSO with this id was deleted.
    ContactDelete { id: String, call: String },
    /// `RadioInfo`: the state of one radio, sent periodically and on change.
    RadioInfo {
        station: String,
        radio_nr: u32,
        freq_hz: u64,
        tx_freq_hz: u64,
        mode: String,
        op_call: String,
        is_running: bool,
        focus_radio_nr: u32,
        active_radio_nr: u32,
        is_transmitting: bool,
    },
    /// `spot`: a DX spot added to or deleted from the band map (`action` is `add` or `delete`).
    Spot {
        call: String,
        freq_hz: u64,
        mode: String,
        spotter: String,
        comment: String,
        action: String,
    },
    /// Any other root element, by name (e.g. `AppInfo`, `lookupinfo`, `dynamicresults`).
    Other(String),
}

/// Root element name plus its direct children's text, keyed by lowercased element name.
fn flatten(xml: &[u8]) -> Result<(String, BTreeMap<String, String>), Error> {
    let xml_err = |e: &dyn std::fmt::Display| Error::Xml(e.to_string());
    let mut reader = quick_xml::Reader::from_reader(xml);
    let (mut root, mut map) = (None::<String>, BTreeMap::new());
    let (mut depth, mut current, mut text) = (0usize, None::<String>, String::new());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).map_err(|e| xml_err(&e))? {
            Event::Start(e) => {
                depth += 1;
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                match depth {
                    1 => root = Some(name),
                    2 => (current, text) = (Some(name.to_ascii_lowercase()), String::new()),
                    _ => {}
                }
            }
            Event::Empty(e) if depth == 0 => {
                return Ok((
                    String::from_utf8_lossy(e.local_name().as_ref()).into_owned(),
                    map,
                ));
            }
            Event::Text(t) if depth == 2 => text.push_str(&t.decode().map_err(|e| xml_err(&e))?),
            Event::CData(t) if depth == 2 => text.push_str(&quick_xml::escape::escape(
                t.decode().map_err(|e| xml_err(&e))?,
            )),
            Event::GeneralRef(r) if depth == 2 => {
                text.push('&');
                text.push_str(&r.decode().map_err(|e| xml_err(&e))?);
                text.push(';');
            }
            Event::End(_) => {
                if depth == 2 {
                    if let Some(name) = current.take() {
                        let value = quick_xml::escape::unescape(&text).map_err(|e| xml_err(&e))?;
                        map.insert(name, value.trim().to_owned());
                    }
                }
                depth = depth.saturating_sub(1);
                if depth == 0 && root.is_some() {
                    break;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    root.map(|r| (r, map)).ok_or(Error::Empty)
}

/// Parses one N1MM Logger+ UDP datagram.
pub fn parse(xml: &[u8]) -> Result<N1mmMessage, Error> {
    let (root, m) = flatten(xml)?;
    let get = |k: &str| m.get(k).cloned().unwrap_or_default();
    let num = |k: &str| m.get(k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let flag = |k: &str| {
        m.get(k)
            .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1")
    };
    Ok(match root.to_ascii_lowercase().as_str() {
        r @ ("contactinfo" | "contactreplace") => N1mmMessage::Contact {
            replace: r == "contactreplace",
            id: get("id"),
            fields: contact_fields(&m),
        },
        "contactdelete" => N1mmMessage::ContactDelete {
            id: get("id"),
            call: get("call"),
        },
        "radioinfo" => N1mmMessage::RadioInfo {
            station: get("stationname"),
            radio_nr: num("radionr") as u32,
            freq_hz: num("freq") * 10,
            tx_freq_hz: num("txfreq") * 10,
            mode: get("mode"),
            op_call: get("opcall"),
            is_running: flag("isrunning"),
            focus_radio_nr: num("focusradionr") as u32,
            active_radio_nr: num("activeradionr") as u32,
            is_transmitting: flag("istransmitting"),
        },
        "spot" => N1mmMessage::Spot {
            call: get("dxcall"),
            freq_hz: m
                .get("frequency")
                .and_then(|v| v.parse::<f64>().ok())
                .map_or(0, |khz| (khz * 1000.0).round() as u64),
            mode: get("mode"),
            spotter: get("spottercall"),
            comment: get("comment"),
            action: get("action"),
        },
        _ => N1mmMessage::Other(root),
    })
}

/// Maps a contactinfo/contactreplace element map to ADIF fields.
fn contact_fields(m: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut f = BTreeMap::new();
    let val = |k: &str| m.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let nonzero = |k: &str| val(k).filter(|v| !v.trim_start_matches('0').is_empty());
    for (src, adif) in [
        ("call", "CALL"),
        ("snt", "RST_SENT"),
        ("rcv", "RST_RCVD"),
        ("gridsquare", "GRIDSQUARE"),
        ("name", "NAME"),
        ("comment", "COMMENT"),
        ("operator", "OPERATOR"),
        ("mycall", "STATION_CALLSIGN"),
        ("contestname", "CONTEST_ID"),
        ("power", "TX_PWR"),
        ("wpxprefix", "PFX"),
        ("state", "STATE"),
    ] {
        if let Some(v) = val(src) {
            f.insert(adif.to_owned(), v.to_owned());
        }
    }
    for (src, adif) in [
        ("sntnr", "STX_STRING"),
        ("rcvnr", "SRX_STRING"),
        ("zone", "CQZ"),
    ] {
        if let Some(v) = nonzero(src) {
            f.insert(adif.to_owned(), v.to_owned());
        }
    }
    let freq = |k: &str| {
        val(k)
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&v| v > 0)
    };
    let (rx, tx) = (freq("rxfreq"), freq("txfreq"));
    if let Some(t) = tx.or(rx) {
        f.insert("FREQ".into(), mhz(t * 10));
    }
    if let Some(r) = rx.filter(|&r| Some(r) != tx) {
        f.insert("FREQ_RX".into(), mhz(r * 10));
    }
    let band = rx
        .or(tx)
        .and_then(|v| band_for_mhz(v as f64 / 100_000.0))
        .or_else(|| val("band").and_then(band_for_label));
    if let Some(b) = band {
        f.insert("BAND".into(), b.into());
    }
    if let Some(mode) = val("mode") {
        let (mode, sub) = adif_mode(mode);
        f.insert("MODE".into(), mode);
        if let Some(s) = sub {
            f.insert("SUBMODE".into(), s);
        }
    }
    if let Some((date, time)) = val("timestamp").and_then(|t| t.split_once(' ')) {
        let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
        let (date, time) = (digits(date), digits(time));
        if date.len() == 8 && time.len() >= 4 {
            f.insert("QSO_DATE".into(), date);
            f.insert("TIME_ON".into(), format!("{time:0<6}")[..6].to_owned());
        }
    }
    f
}

/// Formats Hz as an ADIF MHz string without trailing zeros.
fn mhz(hz: u64) -> String {
    let s = format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// (N1MM band label in MHz, ADIF lower edge MHz, upper edge MHz, ADIF band).
const BANDS: &[(f64, f64, f64, &str)] = &[
    (0.136, 0.1357, 0.1378, "2190m"),
    (0.472, 0.472, 0.479, "630m"),
    (1.8, 1.8, 2.0, "160m"),
    (3.5, 3.5, 4.0, "80m"),
    (5.0, 5.06, 5.45, "60m"),
    (7.0, 7.0, 7.3, "40m"),
    (10.0, 10.1, 10.15, "30m"),
    (14.0, 14.0, 14.35, "20m"),
    (18.0, 18.068, 18.168, "17m"),
    (21.0, 21.0, 21.45, "15m"),
    (24.0, 24.89, 24.99, "12m"),
    (28.0, 28.0, 29.7, "10m"),
    (50.0, 50.0, 54.0, "6m"),
    (70.0, 70.0, 71.0, "4m"),
    (144.0, 144.0, 148.0, "2m"),
    (222.0, 222.0, 225.0, "1.25m"),
    (420.0, 420.0, 450.0, "70cm"),
    (902.0, 902.0, 928.0, "33cm"),
    (1240.0, 1240.0, 1300.0, "23cm"),
    (2300.0, 2300.0, 2450.0, "13cm"),
    (3300.0, 3300.0, 3500.0, "9cm"),
    (5650.0, 5650.0, 5925.0, "6cm"),
    (10000.0, 10000.0, 10500.0, "3cm"),
    (24000.0, 24000.0, 24250.0, "1.25cm"),
    (47000.0, 47000.0, 47200.0, "6mm"),
    (76000.0, 75500.0, 81000.0, "4mm"),
];

fn band_for_mhz(f: f64) -> Option<&'static str> {
    BANDS.iter().find(|b| (b.1..=b.2).contains(&f)).map(|b| b.3)
}

fn band_for_label(label: &str) -> Option<&'static str> {
    let f: f64 = label.trim().parse().ok()?;
    BANDS
        .iter()
        .find(|b| (b.0 - f).abs() < 1e-6)
        .map(|b| b.3)
        .or_else(|| band_for_mhz(f))
}

/// Maps an N1MM mode to ADIF MODE and SUBMODE.
fn adif_mode(mode: &str) -> (String, Option<String>) {
    let m = mode.trim().to_ascii_uppercase();
    let (mode, sub) = match m.as_str() {
        "USB" | "LSB" => ("SSB", Some(m.as_str())),
        "FT4" | "Q65" | "JS8" | "FST4" => ("MFSK", Some(m.as_str())),
        s if s.starts_with("PSK") && s.len() > 3 => ("PSK", Some(s)),
        "PSK" => ("PSK", None),
        s => (s, None),
    };
    (mode.to_owned(), sub.map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTACT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<contactinfo>
	<app>N1MM</app>
	<contestname>CWOPS</contestname>
	<contestnr>73</contestnr>
	<timestamp>2020-01-17 16:43:38</timestamp>
	<mycall>W2XYZ</mycall>
	<band>3.5</band>
	<rxfreq>352519</rxfreq>
	<txfreq>352519</txfreq>
	<operator>K2ABC</operator>
	<mode>CW</mode>
	<call>W1AW</call>
	<countryprefix>K</countryprefix>
	<wpxprefix>W1</wpxprefix>
	<stationprefix>W2XYZ</stationprefix>
	<continent>NA</continent>
	<snt>599</snt>
	<sntnr>5</sntnr>
	<rcv>599</rcv>
	<rcvnr>0</rcvnr>
	<gridsquare>FN31</gridsquare>
	<exchange1></exchange1>
	<section></section>
	<comment>Tom &amp; Jerry</comment>
	<qth></qth>
	<name>HIRAM</name>
	<power>100</power>
	<misctext></misctext>
	<zone>5</zone>
	<prec></prec>
	<ck>0</ck>
	<ismultiplier1>0</ismultiplier1>
	<ismultiplier2>0</ismultiplier2>
	<ismultiplier3>0</ismultiplier3>
	<points>1</points>
	<radionr>1</radionr>
	<run1run2>1</run1run2>
	<RoverLocation></RoverLocation>
	<RadioInterfaced>1</RadioInterfaced>
	<NetworkedCompNr>0</NetworkedCompNr>
	<IsOriginal>True</IsOriginal>
	<NetBiosName></NetBiosName>
	<IsRunQSO>0</IsRunQSO>
	<StationName>CONTEST-PC</StationName>
	<ID>f9ffac4fcd3e479ca86e137df1338531</ID>
	<IsClaimedQso>1</IsClaimedQso>
</contactinfo>"#;

    #[test]
    fn contact_info() {
        let N1mmMessage::Contact {
            replace,
            id,
            fields,
        } = parse(CONTACT.as_bytes()).unwrap()
        else {
            panic!()
        };
        assert!(!replace);
        assert_eq!(id, "f9ffac4fcd3e479ca86e137df1338531");
        let want: BTreeMap<String, String> = [
            ("BAND", "80m"),
            ("CALL", "W1AW"),
            ("COMMENT", "Tom & Jerry"),
            ("CONTEST_ID", "CWOPS"),
            ("CQZ", "5"),
            ("FREQ", "3.52519"),
            ("GRIDSQUARE", "FN31"),
            ("MODE", "CW"),
            ("NAME", "HIRAM"),
            ("OPERATOR", "K2ABC"),
            ("PFX", "W1"),
            ("QSO_DATE", "20200117"),
            ("RST_RCVD", "599"),
            ("RST_SENT", "599"),
            ("STATION_CALLSIGN", "W2XYZ"),
            ("STX_STRING", "5"),
            ("TIME_ON", "164338"),
            ("TX_PWR", "100"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        assert_eq!(fields, want);
    }

    #[test]
    fn contact_replace_modes_and_split() {
        let xml = |mode: &str| {
            format!(
                "<contactreplace><timestamp>2023-06-24 18:00:05</timestamp><band>14</band><rxfreq>1407400</rxfreq>\
                 <txfreq>1407550</txfreq><mode>{mode}</mode><call>DL1ABC</call><rcvnr>0012</rcvnr><zone>0</zone>\
                 <state>MA</state><ID>abc</ID></contactreplace>"
            )
        };
        let fields = |mode: &str| match parse(xml(mode).as_bytes()).unwrap() {
            N1mmMessage::Contact {
                replace: true,
                id,
                fields,
            } if id == "abc" => fields,
            other => panic!("{other:?}"),
        };
        let f = fields("USB");
        assert_eq!(
            (
                f["MODE"].as_str(),
                f["SUBMODE"].as_str(),
                f["BAND"].as_str()
            ),
            ("SSB", "USB", "20m")
        );
        assert_eq!(
            (f["FREQ"].as_str(), f["FREQ_RX"].as_str()),
            ("14.0755", "14.074")
        );
        assert_eq!(
            (f["SRX_STRING"].as_str(), f["STATE"].as_str()),
            ("0012", "MA")
        );
        assert!(!f.contains_key("CQZ") && !f.contains_key("STX_STRING"));
        assert_eq!(f["TIME_ON"], "180005");
        assert_eq!(fields("FT8")["MODE"], "FT8");
        assert!(!fields("FT8").contains_key("SUBMODE"));
        assert_eq!(
            (
                fields("FT4")["MODE"].clone(),
                fields("FT4")["SUBMODE"].clone()
            ),
            ("MFSK".into(), "FT4".into())
        );
        assert_eq!(
            (
                fields("PSK31")["MODE"].clone(),
                fields("PSK31")["SUBMODE"].clone()
            ),
            ("PSK".into(), "PSK31".into())
        );
        assert_eq!(fields("RTTY")["MODE"], "RTTY");
    }

    #[test]
    fn band_from_label_when_no_freq() {
        let N1mmMessage::Contact { fields, .. } = parse(
            b"<contactinfo><band>10</band><rxfreq>0</rxfreq><call>K1ABC</call></contactinfo>",
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(fields["BAND"], "30m");
        assert!(!fields.contains_key("FREQ"));
    }

    #[test]
    fn contact_delete() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<contactdelete>
	<app>N1MM</app>
	<timestamp>2020-01-17 16:43:38</timestamp>
	<call>W1AW</call>
	<contestnr>18</contestnr>
	<StationName>sbdcomputer</StationName>
	<ID>f9ffac4fcd3e479ca86e137df1338531</ID>
</contactdelete>"#;
        assert_eq!(
            parse(xml).unwrap(),
            N1mmMessage::ContactDelete {
                id: "f9ffac4fcd3e479ca86e137df1338531".into(),
                call: "W1AW".into()
            }
        );
    }

    #[test]
    fn radio_info() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<RadioInfo>
	<app>N1MM</app>
	<StationName>CW-PC</StationName>
	<RadioNr>2</RadioNr>
	<Freq>2120000</Freq>
	<TXFreq>2120500</TXFreq>
	<Mode>CW</Mode>
	<OpCall>PY2NY</OpCall>
	<IsRunning>True</IsRunning>
	<FocusEntry>00000</FocusEntry>
	<EntryWindowHwnd>264342</EntryWindowHwnd>
	<Antenna>2</Antenna>
	<Rotors>tribander</Rotors>
	<FocusRadioNr>1</FocusRadioNr>
	<IsStereo>False</IsStereo>
	<IsSplit>True</IsSplit>
	<ActiveRadioNr>2</ActiveRadioNr>
	<IsTransmitting>False</IsTransmitting>
	<FunctionKeyCaption></FunctionKeyCaption>
	<RadioName>IC-7610</RadioName>
	<AuxAntSelected>-1</AuxAntSelected>
	<AuxAntSelectedName></AuxAntSelectedName>
	<IsConnected>True</IsConnected>
</RadioInfo>"#;
        assert_eq!(
            parse(xml).unwrap(),
            N1mmMessage::RadioInfo {
                station: "CW-PC".into(),
                radio_nr: 2,
                freq_hz: 21_200_000,
                tx_freq_hz: 21_205_000,
                mode: "CW".into(),
                op_call: "PY2NY".into(),
                is_running: true,
                focus_radio_nr: 1,
                active_radio_nr: 2,
                is_transmitting: false,
            }
        );
    }

    #[test]
    fn spot_and_other() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<spot>
	<app>N1MM</app>
	<StationName>CONTEST-PC</StationName>
	<dxcall>N1MM</dxcall>
	<frequency>14027.5</frequency>
	<spottercall>K1TTT</spottercall>
	<comment>CQ up 2</comment>
	<action>add</action>
	<mode>CW</mode>
	<status></status>
	<statuslist></statuslist>
	<timestamp>2020-01-17 16:43:38</timestamp>
</spot>"#;
        assert_eq!(
            parse(xml).unwrap(),
            N1mmMessage::Spot {
                call: "N1MM".into(),
                freq_hz: 14_027_500,
                mode: "CW".into(),
                spotter: "K1TTT".into(),
                comment: "CQ up 2".into(),
                action: "add".into(),
            }
        );
        assert_eq!(
            parse(b"<AppInfo><app>N1MM</app><dbname>x.s3db</dbname></AppInfo>").unwrap(),
            N1mmMessage::Other("AppInfo".into())
        );
        assert!(matches!(parse(b""), Err(Error::Empty)));
        assert!(matches!(
            parse(b"<contactinfo><call>W1AW</cal></contactinfo>"),
            Err(Error::Xml(_))
        ));
    }
}
