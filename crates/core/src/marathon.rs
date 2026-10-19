//! CQ DX Marathon: the countries (DXCC entities plus the WAE-only ones) and CQ zones worked
//! in one calendar year. No confirmations are needed; the score is countries plus zones.

use std::collections::{BTreeMap, HashMap};

use crate::cty::CtyDb;

/// The first QSO of the year with one country or zone.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Hit {
    /// Country key (the country file's primary prefix, or `DXCC n` without one) or zone number.
    pub key: String,
    pub name: String,
    pub call: String,
    /// YYYY-MM-DD, UTC.
    pub date: String,
    /// QSO row id.
    pub id: i64,
    #[serde(skip)]
    time_on: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Marathon {
    pub year: i32,
    /// Every year the log has QSOs in, newest first.
    pub years: Vec<i32>,
    /// In the order they were first worked.
    pub countries: Vec<Hit>,
    pub zones: Vec<Hit>,
    /// CQ zones not yet worked this year.
    pub missing_zones: Vec<u32>,
    /// Countries plus zones.
    pub score: usize,
}

/// The Marathon country for a QSO: the country file's entity for `call` (WAE-only ones
/// included), unless the QSO's DXCC field names another DXCC entity (LoTW's, say).
/// Keyed by the entity's primary prefix, so DXCC and WAE entities share one list.
pub fn country(cty: Option<&CtyDb>, call: &str, dxcc: Option<u32>) -> Option<(String, String)> {
    let by_dxcc = |d: u32| {
        cty.and_then(|c| c.entities().iter().find(|e| e.dxcc == Some(d)))
            .map(|e| (e.prefix.clone(), e.name.clone()))
            .unwrap_or_else(|| (format!("DXCC {d}"), crate::awards::deleted_dxcc(d).map_or_else(|| format!("DXCC {d}"), str::to_string)))
    };
    match (cty.and_then(|c| c.lookup(call)), dxcc) {
        (Some(e), Some(d)) if e.dxcc.is_some_and(|x| x != d) => Some(by_dxcc(d)),
        (Some(e), _) => Some((e.prefix, e.name)),
        (None, Some(d)) => Some(by_dxcc(d)),
        (None, None) => None,
    }
}

/// The CQ zone of a QSO: its CQZ field, else the country file's.
pub fn zone(cty: Option<&CtyDb>, call: &str, cqz: Option<u32>) -> Option<u32> {
    cqz.or_else(|| cty.and_then(|c| c.lookup(call)).map(|e| u32::from(e.cq))).filter(|z| (1..=40).contains(z))
}

/// Collects one year's QSOs, keeping the first QSO with each country and zone.
#[derive(Default)]
pub struct MarathonTally {
    countries: HashMap<String, Hit>,
    zones: BTreeMap<u32, Hit>,
}

impl MarathonTally {
    pub fn add(&mut self, id: i64, call: &str, time_on: i64, country: Option<(String, String)>, zone: Option<u32>) {
        let hit = |key: String, name: String| Hit { key, name, call: call.to_string(), date: date(time_on), id, time_on };
        let earlier = |h: &Hit| time_on < h.time_on || (time_on == h.time_on && id < h.id);
        if let Some((key, name)) = country {
            match self.countries.get(&key) {
                Some(h) if !earlier(h) => {}
                _ => {
                    self.countries.insert(key.clone(), hit(key, name));
                }
            }
        }
        if let Some(z) = zone {
            match self.zones.get(&z) {
                Some(h) if !earlier(h) => {}
                _ => {
                    self.zones.insert(z, hit(z.to_string(), format!("Zone {z}")));
                }
            }
        }
    }

    pub fn finish(self, year: i32, years: Vec<i32>) -> Marathon {
        let mut countries: Vec<Hit> = self.countries.into_values().collect();
        countries.sort_by_key(|h| (h.time_on, h.id));
        let missing_zones = (1..=40).filter(|z| !self.zones.contains_key(z)).collect();
        let zones: Vec<Hit> = self.zones.into_values().collect();
        Marathon { year, years, score: countries.len() + zones.len(), countries, zones, missing_zones }
    }
}

/// The countries and zones already worked this year, for flagging spots the Marathon needs.
#[derive(Clone, Debug, Default)]
pub struct MarathonIndex {
    pub year: i32,
    countries: std::collections::HashSet<String>,
    zones: std::collections::HashSet<u32>,
}

/// What a spot would add to this year's Marathon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MarathonNeed {
    pub country: bool,
    pub zone: bool,
}

impl MarathonIndex {
    pub fn new(year: i32) -> Self {
        MarathonIndex { year, ..Self::default() }
    }

    pub fn add(&mut self, country: Option<(String, String)>, zone: Option<u32>) {
        if let Some((key, _)) = country {
            self.countries.insert(key);
        }
        if let Some(z) = zone {
            self.zones.insert(z);
        }
    }

    /// `None` when the spot adds nothing.
    pub fn need(&self, country: Option<&str>, zone: Option<u32>) -> Option<MarathonNeed> {
        let n = MarathonNeed {
            country: country.is_some_and(|c| !self.countries.contains(c)),
            zone: zone.is_some_and(|z| !self.zones.contains(&z)),
        };
        (n.country || n.zone).then_some(n)
    }
}

fn date(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0).map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string())
}

/// Unix seconds of 1 January `year`, 00:00 UTC.
pub fn year_start(year: i32) -> i64 {
    chrono::NaiveDate::from_ymd_opt(year, 1, 1).map_or(0, |d| d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTY: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,K W =W1AW(4);
I,Italy,248,EU,15,28,42.82,-12.58,-1.0,I;
*IT9,Sicily,248,EU,15,28,37.50,-14.00,-1.0,IT9;
";

    #[test]
    fn countries_include_wae_and_keep_the_first_qso() {
        let cty = CtyDb::parse(CTY).unwrap();
        let c = |call: &str, dxcc| country(Some(&cty), call, dxcc).map(|(k, _)| k);
        assert_eq!(c("IT9ABC", Some(248)).as_deref(), Some("IT9"), "Sicily counts apart from Italy");
        assert_eq!(c("I1ABC", None).as_deref(), Some("I"));
        assert_eq!(c("XX1XX", Some(229)).as_deref(), Some("DXCC 229"));
        assert_eq!(country(Some(&cty), "XX1XX", Some(229)).unwrap().1, "German Democratic Republic");
        assert_eq!(c("XX1XX", None), None);
        assert_eq!(c("W1AW", Some(248)).as_deref(), Some("I"), "the QSO's own DXCC wins over the call's");
        assert_eq!(zone(Some(&cty), "W1AW", None), Some(4));
        assert_eq!(zone(Some(&cty), "K1ABC", None), Some(5));
        assert_eq!(zone(Some(&cty), "W1AW", Some(3)), Some(3));
        assert_eq!(zone(None, "W1AW", Some(41)), None);

        let jan = year_start(2026);
        assert_eq!(jan, 1_767_225_600);
        let mut t = MarathonTally::default();
        t.add(2, "IT9ABC", jan + 7200, country(Some(&cty), "IT9ABC", None), zone(Some(&cty), "IT9ABC", None));
        t.add(1, "I1ABC", jan + 3600, country(Some(&cty), "I1ABC", None), Some(15));
        t.add(3, "IT9XYZ", jan + 60, country(Some(&cty), "IT9XYZ", None), Some(15));
        let m = t.finish(2026, vec![2026, 2025]);
        assert_eq!(m.countries.iter().map(|h| h.call.as_str()).collect::<Vec<_>>(), ["IT9XYZ", "I1ABC"]);
        assert_eq!(m.zones.len(), 1);
        assert_eq!((m.zones[0].call.as_str(), m.zones[0].date.as_str()), ("IT9XYZ", "2026-01-01"));
        assert_eq!(m.missing_zones.len(), 39);
        assert_eq!(m.score, 3);

        let mut idx = MarathonIndex::new(2026);
        idx.add(country(Some(&cty), "I1ABC", None), Some(15));
        assert_eq!(idx.need(Some("I"), Some(15)), None);
        assert_eq!(idx.need(Some("IT9"), Some(15)), Some(MarathonNeed { country: true, zone: false }));
        assert_eq!(idx.need(Some("I"), Some(14)), Some(MarathonNeed { country: false, zone: true }));
    }
}
