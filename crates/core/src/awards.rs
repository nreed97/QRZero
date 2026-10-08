//! Award tracking (DXCC, WAS, WAZ, WPX): pure logic, no database.
//!
//! The store feeds one [`AwardQso`] per QSO into a [`Tally`], which produces an
//! [`AwardTable`]: rows (entities, states, zones, prefixes) by columns (mixed,
//! mode groups, bands), each cell worked or confirmed.

use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Award {
    Dxcc,
    Was,
    Waz,
    Wpx,
}

/// Which confirmation sources count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Counts {
    pub lotw: bool,
    pub paper: bool,
    pub eqsl: bool,
}

/// One QSO's award-relevant facts (already extracted from its ADIF fields).
#[derive(Clone, Debug, Default)]
pub struct AwardQso {
    pub call: String,
    /// ADIF band like "20m", lowercase.
    pub band: Option<String>,
    /// ADIF MODE or SUBMODE, e.g. "CW", "SSB", "USB", "FT8", "RTTY".
    pub mode: Option<String>,
    /// ADIF DXCC entity number.
    pub dxcc: Option<u32>,
    /// ADIF STATE.
    pub state: Option<String>,
    /// ADIF CQZ.
    pub cq_zone: Option<u32>,
    /// LOTW_QSL_RCVD == Y (or V).
    pub lotw: bool,
    /// QSL_RCVD == Y (or V).
    pub paper: bool,
    /// EQSL_QSL_RCVD == Y (or V).
    pub eqsl: bool,
    /// QSO row id, so the UI can list QSOs for a cell.
    pub id: i64,
}

/// Cell state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Worked,
    Confirmed,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Row {
    pub key: String,
    pub name: String,
    pub cells: BTreeMap<String, Status>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Column {
    pub key: String,
    pub worked: usize,
    pub confirmed: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AwardTable {
    pub award: Award,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// How many exist to work (340 DXCC, 50 WAS, 40 WAZ); 0 for WPX (open-ended).
    pub total: usize,
}

/// Bands that get their own column, in display order.
pub const AWARD_BANDS: &[&str] = &["160m", "80m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m"];

/// All column keys, in display order.
pub const COLUMNS: &[&str] = &[
    "mixed", "cw", "phone", "digital", "160m", "80m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m",
];

const NCOLS: usize = 14;
const COL_CW: usize = 1;
const COL_PHONE: usize = 2;
const COL_DIGITAL: usize = 3;
const COL_BAND0: usize = 4;

/// The 50 US states (no DC), as (code, name), sorted by code.
pub const US_STATES: &[(&str, &str)] = &[
    ("AK", "Alaska"),
    ("AL", "Alabama"),
    ("AR", "Arkansas"),
    ("AZ", "Arizona"),
    ("CA", "California"),
    ("CO", "Colorado"),
    ("CT", "Connecticut"),
    ("DE", "Delaware"),
    ("FL", "Florida"),
    ("GA", "Georgia"),
    ("HI", "Hawaii"),
    ("IA", "Iowa"),
    ("ID", "Idaho"),
    ("IL", "Illinois"),
    ("IN", "Indiana"),
    ("KS", "Kansas"),
    ("KY", "Kentucky"),
    ("LA", "Louisiana"),
    ("MA", "Massachusetts"),
    ("MD", "Maryland"),
    ("ME", "Maine"),
    ("MI", "Michigan"),
    ("MN", "Minnesota"),
    ("MO", "Missouri"),
    ("MS", "Mississippi"),
    ("MT", "Montana"),
    ("NC", "North Carolina"),
    ("ND", "North Dakota"),
    ("NE", "Nebraska"),
    ("NH", "New Hampshire"),
    ("NJ", "New Jersey"),
    ("NM", "New Mexico"),
    ("NV", "Nevada"),
    ("NY", "New York"),
    ("OH", "Ohio"),
    ("OK", "Oklahoma"),
    ("OR", "Oregon"),
    ("PA", "Pennsylvania"),
    ("RI", "Rhode Island"),
    ("SC", "South Carolina"),
    ("SD", "South Dakota"),
    ("TN", "Tennessee"),
    ("TX", "Texas"),
    ("UT", "Utah"),
    ("VA", "Virginia"),
    ("VT", "Vermont"),
    ("WA", "Washington"),
    ("WI", "Wisconsin"),
    ("WV", "West Virginia"),
    ("WY", "Wyoming"),
];

const DXCC_USA: u32 = 291;
const DXCC_ALASKA: u32 = 6;
const DXCC_HAWAII: u32 = 110;

/// Index into `US_STATES` for a state code (case-insensitive, surrounding spaces ignored).
fn state_index(code: &str) -> Option<usize> {
    let code = code.trim();
    if code.len() != 2 {
        return None;
    }
    let b = code.as_bytes();
    let up = [b[0].to_ascii_uppercase(), b[1].to_ascii_uppercase()];
    US_STATES.binary_search_by(|(c, _)| c.as_bytes().cmp(&up[..])).ok()
}

/// Groups an ADIF mode or submode into "CW", "PHONE" or "DIGITAL". Case-insensitive.
pub fn mode_group(mode: &str) -> Option<&'static str> {
    const PHONE: &[&str] = &["SSB", "USB", "LSB", "AM", "FM", "DIGITALVOICE", "C4FM", "DSTAR", "DMR"];
    let mode = mode.trim();
    if mode.is_empty() {
        None
    } else if mode.eq_ignore_ascii_case("CW") {
        Some("CW")
    } else if PHONE.iter().any(|p| p.eq_ignore_ascii_case(mode)) {
        Some("PHONE")
    } else {
        Some("DIGITAL")
    }
}

fn mode_col(mode: &str) -> Option<usize> {
    match mode_group(mode)? {
        "CW" => Some(COL_CW),
        "PHONE" => Some(COL_PHONE),
        _ => Some(COL_DIGITAL),
    }
}

fn band_col(band: &str) -> Option<usize> {
    let band = band.trim();
    AWARD_BANDS
        .iter()
        .position(|b| b.eq_ignore_ascii_case(band))
        .map(|i| COL_BAND0 + i)
}

/// Suffixes that say nothing about location.
const IGNORED_SUFFIXES: &[&str] = &["P", "M", "MM", "AM", "QRP", "A", "R", "B"];

/// Prefix of a single call or designator: everything up to and including the
/// first run of digits after the first character. `None` when there is no such
/// digit.
fn digit_prefix(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let start = b.iter().skip(1).position(u8::is_ascii_digit)? + 1;
    let len = b[start..].iter().take_while(|c| c.is_ascii_digit()).count();
    Some(&s[..start + len])
}

/// CQ WPX prefix of a callsign, uppercase. `None` for empty or garbage input.
///
/// Examples: K1ABC -> K1, WA2XYZ -> WA2, 2E0ABC -> 2E0, RAEM -> RA0,
/// DL/K1ABC -> DL0, K1ABC/VE3 -> VE3, K1ABC/4 -> K4, K1ABC/P -> K1.
pub fn wpx_prefix(call: &str) -> Option<String> {
    let call = call.trim();
    if call.is_empty() || !call.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'/') {
        return None;
    }
    let upper = call.to_ascii_uppercase();
    let parts: Vec<&str> = upper.split('/').filter(|p| !p.is_empty()).collect();
    // The call itself is the longest part that has a digit (ties: first).
    let (base_idx, base) = parts
        .iter()
        .enumerate()
        .max_by(|(ia, a), (ib, b)| {
            let ka = (a.bytes().any(|c| c.is_ascii_digit()), a.len());
            let kb = (b.bytes().any(|c| c.is_ascii_digit()), b.len());
            ka.cmp(&kb).then(ib.cmp(ia))
        })
        .map(|(i, p)| (i, *p))?;
    if base.len() < 2 || !base.bytes().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    let mut area: Option<char> = None;
    for (i, part) in parts.iter().enumerate() {
        // Longer parts and pure-letter parts of 3+ (LGT, QRP, ROVER) say nothing about location.
        let wordy = part.len() >= 3 && part.bytes().all(|c| c.is_ascii_alphabetic());
        if i == base_idx || IGNORED_SUFFIXES.contains(part) || part.len() > 4 || wordy {
            continue;
        }
        if part.len() == 1 && part.as_bytes()[0].is_ascii_digit() {
            area.get_or_insert(part.chars().next().unwrap_or('0'));
            continue;
        }
        // A country prefix: its own prefix, or the whole thing plus 0.
        return Some(match digit_prefix(part) {
            Some(p) => p.to_string(),
            None => format!("{part}0"),
        });
    }

    let mut prefix = match digit_prefix(base) {
        Some(p) => p.to_string(),
        None => format!("{}0", &base[..2]),
    };
    if let Some(d) = area {
        let keep = prefix.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        prefix.truncate(keep);
        prefix.push(d);
    }
    Some(prefix)
}

/// Internal row key: numbers for DXCC/WAZ, state index for WAS, text for WPX.
enum RowId {
    Num(u32),
    Text(String),
}

fn row_id(award: Award, q: &AwardQso) -> Option<RowId> {
    match award {
        Award::Dxcc => q.dxcc.filter(|&d| d != 0).map(RowId::Num),
        Award::Was => {
            let idx = match q.dxcc {
                Some(DXCC_ALASKA) => state_index("AK"),
                Some(DXCC_HAWAII) => state_index("HI"),
                Some(DXCC_USA) | None => state_index(q.state.as_deref()?),
                Some(_) => None,
            }?;
            Some(RowId::Num(idx as u32))
        }
        Award::Waz => q.cq_zone.filter(|z| (1..=40).contains(z)).map(RowId::Num),
        Award::Wpx => wpx_prefix(&q.call).map(RowId::Text),
    }
}

fn num_key(award: Award, n: u32) -> String {
    match award {
        Award::Was => US_STATES[n as usize].0.to_string(),
        _ => n.to_string(),
    }
}

/// The row key a QSO counts toward for an award (DXCC number, state code, zone
/// number or WPX prefix), or `None` if it does not count.
pub fn row_key(award: Award, q: &AwardQso) -> Option<String> {
    Some(match row_id(award, q)? {
        RowId::Num(n) => num_key(award, n),
        RowId::Text(s) => s,
    })
}

/// Whether a QSO belongs to a column ("mixed", "cw", "phone", "digital" or a band).
pub fn in_column(q: &AwardQso, column: &str) -> bool {
    match column {
        "mixed" => true,
        "cw" | "phone" | "digital" => q
            .mode
            .as_deref()
            .and_then(mode_group)
            .is_some_and(|g| g.eq_ignore_ascii_case(column)),
        band => q.band.as_deref().is_some_and(|b| b.trim().eq_ignore_ascii_case(band)),
    }
}

/// Whether a QSO is part of one cell of an award table (for listing a cell's QSOs).
pub fn in_cell(award: Award, q: &AwardQso, row: &str, column: &str) -> bool {
    in_column(q, column) && row_key(award, q).is_some_and(|k| k == row)
}

/// Cell values: 0 = nothing, 1 = worked, 2 = confirmed.
type Cells = [u8; NCOLS];

/// Accumulates QSOs into an award table.
pub struct Tally {
    award: Award,
    counts: Counts,
    names: BTreeMap<String, String>,
    nums: HashMap<u32, Cells>,
    texts: HashMap<String, Cells>,
}

impl Tally {
    /// `names` gives display names for row keys the caller knows (e.g. DXCC
    /// number -> entity name). With `include_unworked`, every row the caller
    /// names is listed even if unworked (DXCC: every entity in `names`; WAS: the
    /// 50 states; WAZ: zones 1..=40; WPX: never).
    pub fn new(award: Award, counts: Counts, names: BTreeMap<String, String>) -> Self {
        Tally {
            award,
            counts,
            names,
            nums: HashMap::new(),
            texts: HashMap::new(),
        }
    }

    pub fn add(&mut self, q: &AwardQso) {
        let Some(id) = row_id(self.award, q) else { return };
        let c = &self.counts;
        let v = if (c.lotw && q.lotw) || (c.paper && q.paper) || (c.eqsl && q.eqsl) { 2 } else { 1 };
        let cells = match id {
            RowId::Num(n) => self.nums.entry(n).or_insert([0; NCOLS]),
            RowId::Text(s) => self.texts.entry(s).or_insert([0; NCOLS]),
        };
        let mut set = |i: usize| cells[i] = cells[i].max(v);
        set(0);
        if let Some(i) = q.mode.as_deref().and_then(mode_col) {
            set(i);
        }
        if let Some(i) = q.band.as_deref().and_then(band_col) {
            set(i);
        }
    }

    pub fn finish(self, include_unworked: bool) -> AwardTable {
        let award = self.award;
        let names = self.names;
        let mut entries: Vec<(String, Cells)> = self
            .nums
            .into_iter()
            .map(|(n, c)| (num_key(award, n), c))
            .chain(self.texts)
            .collect();

        if include_unworked {
            let all: Vec<String> = match award {
                Award::Dxcc => names.keys().cloned().collect(),
                Award::Was => US_STATES.iter().map(|(c, _)| c.to_string()).collect(),
                Award::Waz => (1..=40).map(|z: u32| z.to_string()).collect(),
                Award::Wpx => Vec::new(),
            };
            let have: std::collections::HashSet<String> = entries.iter().map(|(k, _)| k.clone()).collect();
            entries.extend(all.into_iter().filter(|k| !have.contains(k)).map(|k| (k, [0; NCOLS])));
        }

        let mut columns: Vec<Column> = COLUMNS
            .iter()
            .map(|k| Column {
                key: k.to_string(),
                worked: 0,
                confirmed: 0,
            })
            .collect();

        let mut rows: Vec<Row> = entries
            .into_iter()
            .map(|(key, cells)| {
                let name = names.get(&key).cloned().unwrap_or_else(|| default_name(award, &key));
                let mut map = BTreeMap::new();
                for (i, &v) in cells.iter().enumerate() {
                    if v == 0 {
                        continue;
                    }
                    columns[i].worked += 1;
                    let status = if v >= 2 {
                        columns[i].confirmed += 1;
                        Status::Confirmed
                    } else {
                        Status::Worked
                    };
                    map.insert(COLUMNS[i].to_string(), status);
                }
                Row { key, name, cells: map }
            })
            .collect();

        match award {
            Award::Dxcc => rows.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| num_order(&a.key).cmp(&num_order(&b.key)))
            }),
            Award::Waz => rows.sort_by(|a, b| num_order(&a.key).cmp(&num_order(&b.key)).then(a.key.cmp(&b.key))),
            Award::Was | Award::Wpx => rows.sort_by(|a, b| a.key.cmp(&b.key)),
        }

        let total = match award {
            Award::Dxcc if names.is_empty() => 340,
            Award::Dxcc => names.len(),
            Award::Was => US_STATES.len(),
            Award::Waz => 40,
            Award::Wpx => 0,
        };
        AwardTable {
            award,
            columns,
            rows,
            total,
        }
    }
}

fn num_order(key: &str) -> u64 {
    key.parse().unwrap_or(u64::MAX)
}

fn default_name(award: Award, key: &str) -> String {
    match award {
        Award::Dxcc => format!("DXCC {key}"),
        Award::Was => US_STATES
            .iter()
            .find(|(c, _)| *c == key)
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| key.to_string()),
        Award::Waz => format!("Zone {key}"),
        Award::Wpx => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(call: &str, band: &str, mode: &str) -> AwardQso {
        AwardQso {
            call: call.into(),
            band: Some(band.into()),
            mode: Some(mode.into()),
            ..AwardQso::default()
        }
    }

    fn all_counts() -> Counts {
        Counts {
            lotw: true,
            paper: true,
            eqsl: true,
        }
    }

    fn col<'a>(t: &'a AwardTable, key: &str) -> &'a Column {
        t.columns.iter().find(|c| c.key == key).unwrap()
    }

    fn row<'a>(t: &'a AwardTable, key: &str) -> &'a Row {
        t.rows.iter().find(|r| r.key == key).unwrap()
    }

    #[test]
    fn wpx_examples() {
        let cases = [
            ("K1ABC", "K1"),
            ("N8BJQ", "N8"),
            ("WA2XYZ", "WA2"),
            ("9A1A", "9A1"),
            ("4X6ZZ", "4X6"),
            ("VP2EAA", "VP2"),
            ("2E0ABC", "2E0"),
            ("JA1ABC", "JA1"),
            ("OH2BH", "OH2"),
            ("RAEM", "RA0"),
            ("DL/K1ABC", "DL0"),
            ("K1ABC/VE3", "VE3"),
            ("PJ4/K1ABC", "PJ4"),
            ("K1ABC/4", "K4"),
            ("WA2XYZ/3", "WA3"),
            ("K1ABC/P", "K1"),
            ("K1ABC/M", "K1"),
            ("K1ABC/MM", "K1"),
            ("K1ABC/AM", "K1"),
            ("K1ABC/QRP", "K1"),
            ("K1ABC/A", "K1"),
            ("K1ABC/R", "K1"),
            ("K1ABC/B", "K1"),
            ("K1ABC/ROVER", "K1"),
            ("K1ABC/LGT", "K1"),
            ("VE3/K1ABC/P", "VE3"),
            ("k1abc", "K1"),
            (" oh2bh ", "OH2"),
            ("E73A", "E73"),
            ("T88AB", "T88"),
            ("4X/K1ABC", "4X0"),
            ("KH6/W1AW", "KH6"),
            ("3DA0XX", "3DA0"),
            ("4U1ITU", "4U1"),
        ];
        for (call, want) in cases {
            assert_eq!(wpx_prefix(call).as_deref(), Some(want), "{call}");
        }
    }

    #[test]
    fn wpx_garbage() {
        for call in ["", "  ", "/", "K1-ABC", "123", "K", "ÄÖ1X"] {
            assert_eq!(wpx_prefix(call), None, "{call:?}");
        }
    }

    #[test]
    fn mode_groups() {
        assert_eq!(mode_group("cw"), Some("CW"));
        for m in ["SSB", "usb", "LSB", "AM", "FM", "DIGITALVOICE", "C4FM", "DSTAR", "DMR"] {
            assert_eq!(mode_group(m), Some("PHONE"), "{m}");
        }
        for m in ["FT8", "ft4", "RTTY", "PSK31", "PSK", "JT65", "JS8", "MFSK", "OLIVIA", "DATA", "Q65", "MSK144", "SSTV"] {
            assert_eq!(mode_group(m), Some("DIGITAL"), "{m}");
        }
        assert_eq!(mode_group(""), None);
        assert_eq!(mode_group("  "), None);
    }

    #[test]
    fn us_states_are_50_and_sorted() {
        assert_eq!(US_STATES.len(), 50);
        assert!(US_STATES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(!US_STATES.iter().any(|(c, _)| *c == "DC"));
        assert_eq!(state_index("ny"), US_STATES.iter().position(|(c, _)| *c == "NY"));
        assert_eq!(state_index("DC"), None);
    }

    #[test]
    fn dxcc_counts_only_enabled_sources() {
        let names: BTreeMap<String, String> = [("291", "United States"), ("230", "Fed. Rep. of Germany"), ("1", "Canada")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let counts = Counts {
            lotw: true,
            paper: false,
            eqsl: false,
        };
        let mut t = Tally::new(Award::Dxcc, counts, names);
        // Germany: paper and eQSL only -> worked.
        t.add(&AwardQso {
            dxcc: Some(230),
            paper: true,
            eqsl: true,
            ..q("DL1ABC", "20m", "CW")
        });
        // USA: LoTW on 40m FT8, plain worked on 20m SSB.
        t.add(&AwardQso {
            dxcc: Some(291),
            lotw: true,
            ..q("K1ABC", "40m", "FT8")
        });
        t.add(&AwardQso {
            dxcc: Some(291),
            ..q("W1AW", "20m", "SSB")
        });
        // Skipped: no dxcc, and dxcc 0.
        t.add(&q("XX1X", "20m", "CW"));
        t.add(&AwardQso {
            dxcc: Some(0),
            ..q("K1ABC/MM", "20m", "CW")
        });
        // Unknown name.
        t.add(&AwardQso {
            dxcc: Some(999),
            ..q("ZZ1ZZ", "2m", "FM")
        });

        let tab = t.finish(false);
        assert_eq!(tab.total, 3);
        assert_eq!(tab.rows.len(), 3);
        // Sorted by name.
        let keys: Vec<&str> = tab.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["999", "230", "291"]);
        assert_eq!(row(&tab, "999").name, "DXCC 999");

        let de = row(&tab, "230");
        assert_eq!(de.cells.get("mixed"), Some(&Status::Worked));
        assert_eq!(de.cells.get("cw"), Some(&Status::Worked));
        assert_eq!(de.cells.get("20m"), Some(&Status::Worked));
        assert_eq!(de.cells.len(), 3);

        let us = row(&tab, "291");
        assert_eq!(us.cells.get("mixed"), Some(&Status::Confirmed));
        assert_eq!(us.cells.get("digital"), Some(&Status::Confirmed));
        assert_eq!(us.cells.get("40m"), Some(&Status::Confirmed));
        assert_eq!(us.cells.get("phone"), Some(&Status::Worked));
        assert_eq!(us.cells.get("20m"), Some(&Status::Worked));
        assert_eq!(us.cells.get("cw"), None);

        // 2m is not a column, but the QSO counts for mixed and phone.
        let zz = row(&tab, "999");
        assert_eq!(zz.cells.len(), 2);
        assert_eq!(zz.cells.get("phone"), Some(&Status::Worked));

        let keys: Vec<&str> = tab.columns.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, COLUMNS);
        assert_eq!((col(&tab, "mixed").worked, col(&tab, "mixed").confirmed), (3, 1));
        assert_eq!((col(&tab, "20m").worked, col(&tab, "20m").confirmed), (2, 0));
        assert_eq!((col(&tab, "40m").worked, col(&tab, "40m").confirmed), (1, 1));
        assert_eq!((col(&tab, "phone").worked, col(&tab, "phone").confirmed), (2, 0));
        assert_eq!((col(&tab, "cw").worked, col(&tab, "cw").confirmed), (1, 0));
        assert_eq!(col(&tab, "6m").worked, 0);
    }

    #[test]
    fn confirmation_upgrades_but_never_downgrades() {
        let mut t = Tally::new(Award::Dxcc, all_counts(), BTreeMap::new());
        let base = AwardQso {
            dxcc: Some(1),
            ..q("VE3ABC", "20m", "CW")
        };
        t.add(&AwardQso { eqsl: true, ..base.clone() });
        t.add(&base);
        let tab = t.finish(false);
        assert_eq!(tab.total, 340);
        assert_eq!(row(&tab, "1").cells.get("20m"), Some(&Status::Confirmed));
    }

    #[test]
    fn no_sources_counted_means_worked_only() {
        let mut t = Tally::new(Award::Dxcc, Counts::default(), BTreeMap::new());
        t.add(&AwardQso {
            dxcc: Some(1),
            lotw: true,
            paper: true,
            eqsl: true,
            ..q("VE3ABC", "20m", "CW")
        });
        let tab = t.finish(false);
        assert_eq!(row(&tab, "1").cells.get("mixed"), Some(&Status::Worked));
        assert_eq!(col(&tab, "mixed").confirmed, 0);
    }

    #[test]
    fn dxcc_include_unworked() {
        let names: BTreeMap<String, String> = [("1", "Canada"), ("230", "Germany")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut t = Tally::new(Award::Dxcc, all_counts(), names);
        t.add(&AwardQso {
            dxcc: Some(1),
            ..q("VE3ABC", "20m", "CW")
        });
        let tab = t.finish(true);
        assert_eq!(tab.rows.len(), 2);
        assert!(row(&tab, "230").cells.is_empty());
        assert_eq!(col(&tab, "mixed").worked, 1);
    }

    #[test]
    fn was_alaska_hawaii_and_filters() {
        let mut t = Tally::new(Award::Was, all_counts(), BTreeMap::new());
        // Alaska and Hawaii by DXCC, no STATE.
        t.add(&AwardQso {
            dxcc: Some(6),
            ..q("KL7ABC", "20m", "SSB")
        });
        t.add(&AwardQso {
            dxcc: Some(110),
            lotw: true,
            ..q("KH6ABC", "15m", "FT8")
        });
        // USA with state, lowercase.
        t.add(&AwardQso {
            dxcc: Some(291),
            state: Some("ny".into()),
            ..q("W2ABC", "40m", "CW")
        });
        // No DXCC but valid state.
        t.add(&AwardQso {
            state: Some("TX".into()),
            ..q("W5ABC", "10m", "CW")
        });
        // Skipped: DC, Canadian province with a "state", USA without state.
        t.add(&AwardQso {
            dxcc: Some(291),
            state: Some("DC".into()),
            ..q("W3ABC", "20m", "CW")
        });
        t.add(&AwardQso {
            dxcc: Some(1),
            state: Some("ON".into()),
            ..q("VE3ABC", "20m", "CW")
        });
        t.add(&AwardQso {
            dxcc: Some(1),
            state: Some("ME".into()),
            ..q("VE1ABC", "20m", "CW")
        });
        t.add(&AwardQso {
            dxcc: Some(291),
            ..q("W9ABC", "20m", "CW")
        });

        let tab = t.finish(false);
        assert_eq!(tab.total, 50);
        let keys: Vec<&str> = tab.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["AK", "HI", "NY", "TX"]);
        assert_eq!(row(&tab, "AK").name, "Alaska");
        assert_eq!(row(&tab, "HI").cells.get("15m"), Some(&Status::Confirmed));
        assert_eq!(row(&tab, "AK").cells.get("phone"), Some(&Status::Worked));

        let mut t = Tally::new(Award::Was, all_counts(), BTreeMap::new());
        t.add(&AwardQso {
            dxcc: Some(6),
            ..q("KL7ABC", "20m", "SSB")
        });
        let tab = t.finish(true);
        assert_eq!(tab.rows.len(), 50);
        assert_eq!(tab.rows[0].key, "AK");
        assert_eq!(tab.rows[49].key, "WY");
        assert_eq!(col(&tab, "mixed").worked, 1);
    }

    #[test]
    fn waz_zones() {
        let mut t = Tally::new(Award::Waz, all_counts(), BTreeMap::new());
        for (z, call) in [(5, "K1ABC"), (14, "DL1ABC"), (40, "OX1A"), (0, "X"), (41, "Y")] {
            t.add(&AwardQso {
                cq_zone: Some(z),
                ..q(call, "20m", "CW")
            });
        }
        t.add(&q("NOZONE", "20m", "CW"));
        let tab = t.finish(false);
        assert_eq!(tab.total, 40);
        let keys: Vec<&str> = tab.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["5", "14", "40"]);
        assert_eq!(row(&tab, "14").name, "Zone 14");

        let tab = Tally::new(Award::Waz, all_counts(), BTreeMap::new()).finish(true);
        let keys: Vec<String> = tab.rows.iter().map(|r| r.key.clone()).collect();
        let want: Vec<String> = (1..=40).map(|z: u32| z.to_string()).collect();
        assert_eq!(keys, want);
    }

    #[test]
    fn wpx_tally() {
        let mut t = Tally::new(Award::Wpx, all_counts(), BTreeMap::new());
        for call in ["K1ABC", "K1XYZ", "WA2XYZ", "DL/K1ABC", "", "???"] {
            t.add(&q(call, "20m", "CW"));
        }
        let tab = t.finish(true);
        assert_eq!(tab.total, 0);
        let keys: Vec<&str> = tab.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["DL0", "K1", "WA2"]);
        assert_eq!(row(&tab, "K1").name, "K1");
        assert_eq!(col(&tab, "20m").worked, 3);
    }

    #[test]
    fn cell_membership() {
        let qso = AwardQso {
            dxcc: Some(291),
            state: Some("ny".into()),
            ..q("W2ABC", "40m", "USB")
        };
        assert_eq!(row_key(Award::Was, &qso).as_deref(), Some("NY"));
        assert_eq!(row_key(Award::Dxcc, &qso).as_deref(), Some("291"));
        assert_eq!(row_key(Award::Wpx, &qso).as_deref(), Some("W2"));
        assert_eq!(row_key(Award::Waz, &qso), None);
        assert!(in_cell(Award::Was, &qso, "NY", "phone"));
        assert!(in_cell(Award::Was, &qso, "NY", "40m"));
        assert!(in_cell(Award::Was, &qso, "NY", "mixed"));
        assert!(!in_cell(Award::Was, &qso, "NY", "cw"));
        assert!(!in_cell(Award::Was, &qso, "NJ", "40m"));
    }

    #[test]
    fn serde_names() {
        assert_eq!(serde_json::to_string(&Award::Dxcc).unwrap(), "\"dxcc\"");
        assert_eq!(serde_json::to_string(&Status::Confirmed).unwrap(), "\"confirmed\"");
        let c: Counts = serde_json::from_str(r#"{"lotw":true}"#).unwrap();
        assert_eq!(
            c,
            Counts {
                lotw: true,
                paper: false,
                eqsl: false
            }
        );
    }

    #[test]
    fn tally_is_fast() {
        let mut t = Tally::new(Award::Dxcc, all_counts(), BTreeMap::new());
        let modes = ["CW", "SSB", "FT8", "RTTY"];
        let bands = ["160m", "80m", "40m", "20m", "15m", "10m", "2m"];
        let qsos: Vec<AwardQso> = (0..200_000u32)
            .map(|i| AwardQso {
                dxcc: Some(i % 340 + 1),
                lotw: i % 3 == 0,
                ..q("K1ABC", bands[i as usize % bands.len()], modes[i as usize % modes.len()])
            })
            .collect();
        let start = std::time::Instant::now();
        for qso in &qsos {
            t.add(qso);
        }
        let tab = t.finish(true);
        let elapsed = start.elapsed();
        assert_eq!(tab.rows.len(), 340);
        // Generous bound so debug builds on slow CI pass.
        assert!(elapsed.as_millis() < 2000, "{elapsed:?}");
    }
}
