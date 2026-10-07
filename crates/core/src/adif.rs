//! ADIF (.adi) reading and writing.
//!
//! The reader is deliberately tolerant: real-world files disagree on whether a
//! field length counts bytes or characters, and older Windows loggers write
//! Windows-1252 instead of UTF-8. Both cases are handled.

use std::collections::BTreeMap;

/// ADIF fields of one record, keyed by upper-case field name.
pub type Fields = BTreeMap<String, String>;

#[derive(Debug, Default)]
pub struct AdifFile {
    pub header: Fields,
    pub records: Vec<Fields>,
    pub warnings: Vec<String>,
}

/// Fields written by a "standard" export: what other loggers and QSL services
/// commonly understand. A "full" export writes every stored field.
pub const STANDARD_FIELDS: &[&str] = &[
    "CALL", "QSO_DATE", "TIME_ON", "QSO_DATE_OFF", "TIME_OFF", "BAND", "BAND_RX", "FREQ",
    "FREQ_RX", "MODE", "SUBMODE", "RST_SENT", "RST_RCVD", "STATION_CALLSIGN", "OPERATOR",
    "OWNER_CALLSIGN", "NAME", "QTH", "STATE", "CNTY", "COUNTRY", "DXCC", "CQZ", "ITUZ", "CONT",
    "GRIDSQUARE", "LAT", "LON", "IOTA", "SOTA_REF", "POTA_REF", "WWFF_REF", "SIG", "SIG_INFO",
    "TX_PWR", "RX_PWR", "PROP_MODE", "SAT_NAME", "SAT_MODE", "CONTEST_ID", "SRX", "STX",
    "SRX_STRING", "STX_STRING", "COMMENT", "NOTES", "EMAIL", "QSL_VIA", "QSL_SENT", "QSL_RCVD",
    "QSL_SENT_VIA", "QSL_RCVD_VIA", "QSLSDATE", "QSLRDATE", "LOTW_QSL_SENT", "LOTW_QSL_RCVD",
    "LOTW_QSLSDATE", "LOTW_QSLRDATE", "EQSL_QSL_SENT", "EQSL_QSL_RCVD", "QRZCOM_QSO_UPLOAD_STATUS",
    "QRZCOM_QSO_UPLOAD_DATE", "CLUBLOG_QSO_UPLOAD_STATUS", "CLUBLOG_QSO_UPLOAD_DATE",
    "MY_GRIDSQUARE", "MY_CITY", "MY_STATE", "MY_CNTY", "MY_COUNTRY", "MY_DXCC", "MY_CQ_ZONE",
    "MY_ITU_ZONE", "MY_LAT", "MY_LON", "MY_IOTA", "MY_SOTA_REF", "MY_POTA_REF", "MY_WWFF_REF",
    "MY_SIG", "MY_SIG_INFO", "MY_RIG", "MY_ANTENNA",
];

/// Preferred output order; anything else follows alphabetically.
const FIELD_ORDER: &[&str] = &[
    "CALL", "QSO_DATE", "TIME_ON", "QSO_DATE_OFF", "TIME_OFF", "BAND", "FREQ", "MODE", "SUBMODE",
    "RST_SENT", "RST_RCVD", "STATION_CALLSIGN", "OPERATOR",
];

pub fn is_standard_field(name: &str) -> bool {
    STANDARD_FIELDS.contains(&name)
}

pub fn parse(input: &[u8]) -> AdifFile {
    let utf8 = std::str::from_utf8(input).is_ok();
    let mut file = AdifFile::default();
    let decode = |bytes: &[u8]| -> String {
        if utf8 {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned()
        }
    };

    // A header exists when the file does not start with '<'. Some writers break
    // that rule, so the presence of <EOH> is what really decides it.
    let eoh = find_ci(input, b"<eoh>");
    let eor = find_ci(input, b"<eor>");
    let mut in_header = matches!((eoh, eor), (Some(h), Some(r)) if h < r) || (eoh.is_some() && eor.is_none());
    let mut pos = 0usize;
    let mut current = Fields::new();

    while let Some(rel) = input[pos..].iter().position(|&b| b == b'<') {
        let tag_start = pos + rel + 1;
        let Some(tag_len) = input[tag_start..].iter().position(|&b| b == b'>') else {
            break;
        };
        let tag = &input[tag_start..tag_start + tag_len];
        let value_start = tag_start + tag_len + 1;
        let tag_str = String::from_utf8_lossy(tag);
        let mut parts = tag_str.split(':');
        let name = parts.next().unwrap_or("").trim().to_ascii_uppercase();
        let len_part = parts.next();

        if len_part.is_none() {
            match name.as_str() {
                "EOH" => {
                    in_header = false;
                    current.clear();
                }
                "EOR" => {
                    if !current.is_empty() {
                        file.records.push(std::mem::take(&mut current));
                    }
                }
                _ => {}
            }
            pos = value_start;
            continue;
        }

        let Ok(len) = len_part.unwrap().trim().parse::<usize>() else {
            file.warnings.push(format!("bad length in tag <{tag_str}>"));
            pos = value_start;
            continue;
        };
        let end = value_end(input, value_start, len, utf8);
        let value = decode(&input[value_start..end]);
        if !name.is_empty() {
            let target = if in_header { &mut file.header } else { &mut current };
            target.insert(name, value.trim().to_string());
        }
        pos = end;
    }

    if !current.is_empty() {
        file.warnings.push("last record had no <EOR>; imported anyway".into());
        file.records.push(current);
    }
    file
}

/// Works out where a value ends. The length should count bytes for ASCII data,
/// but writers disagree once UTF-8 is involved, so when byte and character
/// counts differ we pick the one that lands just before the next tag.
fn value_end(input: &[u8], start: usize, len: usize, utf8: bool) -> usize {
    let byte_end = (start + len).min(input.len());
    if !utf8 {
        return byte_end;
    }
    let mut char_end = start;
    let mut chars = 0;
    while char_end < input.len() && chars < len {
        let b = input[char_end];
        let w = if b < 0x80 {
            1
        } else if b >> 5 == 0b110 {
            2
        } else if b >> 4 == 0b1110 {
            3
        } else {
            4
        };
        char_end = (char_end + w).min(input.len());
        chars += 1;
    }
    if char_end == byte_end {
        return byte_end;
    }
    let next_is_tag = |at: usize| {
        input[at..]
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_none_or(|&b| b == b'<')
    };
    if next_is_tag(byte_end) && input.is_char_boundary_at(byte_end) {
        byte_end
    } else if next_is_tag(char_end) {
        char_end
    } else {
        byte_end
    }
}

trait CharBoundary {
    fn is_char_boundary_at(&self, at: usize) -> bool;
}

impl CharBoundary for [u8] {
    fn is_char_boundary_at(&self, at: usize) -> bool {
        at >= self.len() || (self[at] as i8) >= -0x40
    }
}

fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
}

pub fn write_header(out: &mut String, program_version: &str, records: usize) {
    let now = chrono::Utc::now().format("%Y%m%d %H%M%S").to_string();
    out.push_str(&format!(
        "Exported by QRZero {program_version} - {records} QSOs\n"
    ));
    push_field(out, "ADIF_VER", "3.1.5");
    push_field(out, "PROGRAMID", "QRZero");
    push_field(out, "PROGRAMVERSION", program_version);
    push_field(out, "CREATED_TIMESTAMP", &now);
    out.push_str("<EOH>\n\n");
}

/// Writes one record. `include` decides which fields are written.
pub fn write_record(out: &mut String, fields: &Fields, include: impl Fn(&str) -> bool) {
    for name in FIELD_ORDER {
        if let Some(v) = fields.get(*name) {
            if include(name) {
                push_field(out, name, v);
            }
        }
    }
    for (name, v) in fields {
        if !FIELD_ORDER.contains(&name.as_str()) && include(name) {
            push_field(out, name, v);
        }
    }
    out.push_str("<EOR>\n");
}

fn push_field(out: &mut String, name: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    // ADIF counts the length of the data; we count characters, which equals
    // bytes for the ASCII data the spec expects.
    out.push_str(&format!("<{}:{}>{} ", name, value.chars().count(), value));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_records() {
        let src = b"Generated by test\n<ADIF_VER:5>3.1.4 <EOH>\n<call:4>W1AW<qso_date:8>20240101<time_on:4>1200<band:3>20M<mode:2>CW<eor>\n<CALL:5>K1ABC <QSO_DATE:8>20240102 <TIME_ON:6>130000 <EOR>";
        let f = parse(src);
        assert_eq!(f.header.get("ADIF_VER").unwrap(), "3.1.4");
        assert_eq!(f.records.len(), 2);
        assert_eq!(f.records[0]["CALL"], "W1AW");
        assert_eq!(f.records[0]["BAND"], "20M");
        assert_eq!(f.records[1]["TIME_ON"], "130000");
    }

    #[test]
    fn no_header_and_value_containing_angle_bracket() {
        let f = parse(b"<CALL:4>W1AW<COMMENT:5>a<b>c<EOR>");
        assert_eq!(f.records.len(), 1);
        assert_eq!(f.records[0]["COMMENT"], "a<b>c");
    }

    #[test]
    fn utf8_length_counted_in_chars_or_bytes() {
        // "José" is 4 characters, 5 bytes.
        let chars = "<NAME:4>José<CALL:4>EA1A<EOR>";
        let bytes = "<NAME:5>José<CALL:4>EA1A<EOR>";
        assert_eq!(parse(chars.as_bytes()).records[0]["NAME"], "José");
        assert_eq!(parse(bytes.as_bytes()).records[0]["NAME"], "José");
        assert_eq!(parse(chars.as_bytes()).records[0]["CALL"], "EA1A");
    }

    #[test]
    fn windows_1252_input() {
        let mut src = b"<NAME:4>Jos".to_vec();
        src.push(0xE9);
        src.extend_from_slice(b"<EOR>");
        assert_eq!(parse(&src).records[0]["NAME"], "José");
    }

    #[test]
    fn roundtrip() {
        let mut fields = Fields::new();
        fields.insert("CALL".into(), "W1AW".into());
        fields.insert("QSO_DATE".into(), "20240101".into());
        fields.insert("APP_TEST_X".into(), "1".into());
        let mut out = String::new();
        write_header(&mut out, "0.1.0", 1);
        write_record(&mut out, &fields, |_| true);
        let f = parse(out.as_bytes());
        assert_eq!(f.records, vec![fields]);
    }
}
