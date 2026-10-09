//! CW club awards: SKCC (Centurion, Tribune, Senator) and CWops (ACA, CMA, ACMA,
//! DXCC, WAS, and the CWT participation medals). Pure logic, no database.
//!
//! The store feeds one [`ClubQso`] per CW QSO into a [`ClubTally`]. Member
//! numbers, not calls, identify a member, because calls change hands over the
//! years. Numbers and suffixes are taken as logged and nothing is validated
//! against the clubs' rosters, so their own award managers stay the authority
//! when you apply.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};

use crate::awards::US_STATES;

/// SKCC rank of a member, from the suffix they gave in the QSO.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    #[default]
    Member,
    Centurion,
    Tribune,
    Senator,
}

/// An SKCC number as logged ("1234", "1234S", "1234 Tx8"): the number and the rank.
pub fn parse_skcc(s: &str) -> Option<(u32, Rank)> {
    let s = s.trim();
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 7 {
        return None;
    }
    let n: u32 = s[..digits].parse().ok()?;
    let rank = match s[digits..].trim_start().bytes().next().map(|c| c.to_ascii_uppercase()) {
        Some(b'C') => Rank::Centurion,
        Some(b'T') => Rank::Tribune,
        Some(b'S') => Rank::Senator,
        _ => Rank::Member,
    };
    (n > 0).then_some((n, rank))
}

/// A CWops member number as logged (digits only).
pub fn parse_cwops(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() || s.len() > 6 || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|&n| n > 0)
}

/// One CW QSO's club facts.
#[derive(Clone, Debug, Default)]
pub struct ClubQso {
    pub call: String,
    /// Unix seconds, UTC.
    pub time: i64,
    /// ADIF band like "20m", lowercase.
    pub band: Option<String>,
    pub dxcc: Option<u32>,
    /// Upper-case STATE.
    pub state: Option<String>,
    pub skcc: Option<(u32, Rank)>,
    pub cwops: Option<u32>,
    /// CONTEST_ID names a CWops test.
    pub cwt_tagged: bool,
}

/// Progress towards one award with endorsement levels.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Level {
    pub key: String,
    pub name: String,
    /// What counts, in a sentence.
    pub rule: String,
    /// Different members counted.
    pub count: usize,
    /// Members needed per level (100 for Centurion).
    pub step: usize,
    /// Levels earned (Centurion x2 is 2), up to 10.
    pub level: usize,
    /// Members still needed for the next level; 0 at level 10.
    pub next: usize,
    /// Date of the QSO that completed the first level (YYYY-MM-DD).
    pub achieved: Option<String>,
    /// Counted members per band.
    pub bands: BTreeMap<String, usize>,
    /// Why nothing counts yet, when that is the case.
    pub note: Option<String>,
}

const MAX_LEVEL: usize = 10;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Skcc {
    /// Different SKCC members worked on CW, whatever the date.
    pub members: usize,
    pub awards: Vec<Level>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct YearRow {
    pub year: i32,
    /// ACA: different members contacted in the year.
    pub aca: usize,
    /// ACMA (from 2024): members contacted per band in the year, added up.
    pub acma: Option<usize>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Cwops {
    /// Different CWops members worked on CW, ever.
    pub members: usize,
    pub years: Vec<YearRow>,
    /// CMA: different members per band since 2010-01-03.
    pub cma_bands: BTreeMap<String, usize>,
    pub cma_total: usize,
    /// CWops DXCC: different entities with members (100 for the award), and per band.
    pub dxcc: usize,
    pub dxcc_bands: BTreeMap<String, usize>,
    /// CWops WAS: different states with members (50 for the award), and per band.
    pub was: usize,
    pub was_bands: BTreeMap<String, usize>,
}

/// Where the operator is, for CWT points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// North America or Europe: 10 contacts make a point.
    #[default]
    NaEu,
    /// Anywhere else: 5 contacts make a point.
    Other,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CwtOptions {
    pub region: Region,
    /// Count only QSOs whose CONTEST_ID names a CWops test.
    pub tagged_only: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CwtYear {
    pub year: i32,
    /// CWT hours with at least one QSO.
    pub sessions: usize,
    /// Hours with enough contacts for a point.
    pub points: usize,
    /// "gold", "silver", "bronze" or empty.
    pub medal: String,
    /// Points still needed for the next medal; 0 at gold.
    pub next: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Cwt {
    /// Contacts needed in one hour for a point (a call counts again on another band).
    pub per_point: usize,
    /// Points for bronze, silver and gold.
    pub thresholds: [usize; 3],
    pub years: Vec<CwtYear>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ClubAwards {
    pub skcc: Skcc,
    pub cwops: Cwops,
    pub cwt: Cwt,
}

// First dates the awards accept.
const CENTURION_FROM: (i32, u32, u32) = (2002, 3, 1);
const TRIBUNE_FROM: (i32, u32, u32) = (2007, 3, 1);
const SENATOR_FROM: (i32, u32, u32) = (2013, 8, 1);
const CMA_FROM: (i32, u32, u32) = (2010, 1, 3);
const ACMA_FROM_YEAR: i32 = 2024;

fn start_of(d: (i32, u32, u32)) -> i64 {
    NaiveDate::from_ymd_opt(d.0, d.1, d.2).and_then(|d| d.and_hms_opt(0, 0, 0)).map_or(0, |t| t.and_utc().timestamp())
}

fn utc(t: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(t, 0).unwrap_or_default()
}

fn date_text(t: i64) -> String {
    utc(t).format("%Y-%m-%d").to_string()
}

/// The start of the UTC day a time falls in.
fn day_start(t: i64) -> i64 {
    t.div_euclid(86_400) * 86_400
}

/// The CWT hour a time falls in, as (day number, hour): Wednesday 13 and 19 UTC,
/// Thursday 03 and 07 UTC.
fn cwt_session(t: i64) -> Option<(i64, u32)> {
    let dt = utc(t);
    match (dt.weekday().num_days_from_monday(), dt.hour()) {
        (2, h @ (13 | 19)) | (3, h @ (3 | 7)) => Some((t.div_euclid(86_400), h)),
        _ => None,
    }
}

/// Different call and band pairs per point, and points for bronze, silver and gold.
fn cwt_thresholds(region: Region) -> (usize, [usize; 3]) {
    match region {
        Region::NaEu => (10, [50, 80, 120]),
        Region::Other => (5, [24, 40, 60]),
    }
}

/// Accumulates CW QSOs into the club awards.
#[derive(Default)]
pub struct ClubTally {
    qsos: Vec<ClubQso>,
}

impl ClubTally {
    pub fn add(&mut self, q: &ClubQso) {
        self.qsos.push(q.clone());
    }

    pub fn finish(mut self, cwt: CwtOptions) -> ClubAwards {
        self.qsos.sort_by_key(|q| q.time);
        ClubAwards { skcc: skcc(&self.qsos), cwops: cwops(&self.qsos), cwt: cwt_medals(&self.qsos, cwt) }
    }
}

/// Different members counted for one SKCC award.
#[derive(Default)]
struct Counted {
    seen: HashSet<u32>,
    bands: HashMap<String, HashSet<u32>>,
    /// Time of the QSO that made `seen` reach the award's first level.
    reached: Option<i64>,
}

impl Counted {
    fn add(&mut self, q: &ClubQso, number: u32, first_level: usize) {
        if let Some(b) = &q.band {
            self.bands.entry(b.clone()).or_default().insert(number);
        }
        if self.seen.insert(number) && self.seen.len() == first_level {
            self.reached = Some(q.time);
        }
    }

    fn level(&self, key: &str, name: &str, rule: &str, step: usize, note: &str) -> Level {
        let count = self.seen.len();
        let level = (count / step).min(MAX_LEVEL);
        Level {
            key: key.into(),
            name: name.into(),
            rule: rule.into(),
            count,
            step,
            level,
            next: if level >= MAX_LEVEL { 0 } else { (level + 1) * step - count },
            achieved: self.reached.map(date_text),
            bands: band_counts(self.bands.iter().map(|(b, s)| (b.clone(), s.len()))),
            note: (count == 0).then(|| note.to_string()),
        }
    }
}

/// Band counts, skipping empty bands.
fn band_counts(it: impl Iterator<Item = (String, usize)>) -> BTreeMap<String, usize> {
    it.filter(|(_, n)| *n > 0).collect()
}

/// SKCC contacts as (QSO, member number, rank).
fn skcc_contacts(qsos: &[ClubQso]) -> impl Iterator<Item = (&ClubQso, u32, Rank)> {
    qsos.iter().filter_map(|q| q.skcc.map(|(n, r)| (q, n, r)))
}

fn skcc(qsos: &[ClubQso]) -> Skcc {
    let all: HashSet<u32> = skcc_contacts(qsos).map(|(_, n, _)| n).collect();

    // Centurion: any member, from 2002-03-01.
    let from = start_of(CENTURION_FROM);
    let mut cent = Counted::default();
    for (q, n, _) in skcc_contacts(qsos).filter(|(q, ..)| q.time >= from) {
        cent.add(q, n, 100);
    }

    // Tribune: C, T or S members, from 2007-03-01 and from the day you became a Centurion.
    let from = cent.reached.map(day_start).map(|d| d.max(start_of(TRIBUNE_FROM)));
    let mut trib = Counted::default();
    if let Some(from) = from {
        for (q, n, _) in skcc_contacts(qsos).filter(|(q, _, r)| q.time >= from && *r >= Rank::Centurion) {
            trib.add(q, n, 50);
        }
    }

    // Senator: T or S members, from 2013-08-01 and from the day you reached Tribune x8
    // (400 Tribune members). Members used for Tribune can count again.
    let tx8 = from.filter(|_| trib.seen.len() >= 400).and_then(|from| nth_tribune_day(qsos, from, 400));
    let mut sen = Counted::default();
    if let Some(day) = tx8 {
        let from = day.max(start_of(SENATOR_FROM));
        for (q, n, _) in skcc_contacts(qsos).filter(|(q, _, r)| q.time >= from && *r >= Rank::Tribune) {
            sen.add(q, n, 200);
        }
    }

    Skcc {
        members: all.len(),
        awards: vec![
            cent.level("centurion", "Centurion", "100 different SKCC members, from 2002-03-01", 100, "Log SKCC numbers on CW QSOs to start counting."),
            trib.level(
                "tribune",
                "Tribune",
                "50 different Centurions, Tribunes or Senators (by the suffix they gave), from the day you became a Centurion, from 2007-03-01",
                50,
                "Counts from the day you reach Centurion.",
            ),
            sen.level(
                "senator",
                "Senator",
                "200 different Tribunes or Senators, from the day you reached Tribune x8 (400 Tribune members), from 2013-08-01",
                200,
                "Counts from the day you reach Tribune x8.",
            ),
        ],
    }
}

/// The start of the UTC day on which the `n`th different Tribune-qualifying member was worked since `from`.
fn nth_tribune_day(qsos: &[ClubQso], from: i64, n: usize) -> Option<i64> {
    let mut seen = HashSet::new();
    skcc_contacts(qsos)
        .filter(|(q, _, r)| q.time >= from && *r >= Rank::Centurion)
        .find(|(_, num, _)| seen.insert(*num) && seen.len() == n)
        .map(|(q, ..)| day_start(q.time))
}

fn set_counts<T>(m: &HashMap<&str, HashSet<T>>) -> BTreeMap<String, usize> {
    band_counts(m.iter().map(|(b, s)| (b.to_string(), s.len())))
}

fn cwops(qsos: &[ClubQso]) -> Cwops {
    let cma_from = start_of(CMA_FROM);
    let mut all = HashSet::new();
    let mut per_year: BTreeMap<i32, HashSet<u32>> = BTreeMap::new();
    let mut per_year_band: HashMap<i32, HashMap<&str, HashSet<u32>>> = HashMap::new();
    let mut cma: HashMap<&str, HashSet<u32>> = HashMap::new();
    let mut dxcc: HashSet<u32> = HashSet::new();
    let mut dxcc_b: HashMap<&str, HashSet<u32>> = HashMap::new();
    let mut was: HashSet<usize> = HashSet::new();
    let mut was_b: HashMap<&str, HashSet<usize>> = HashMap::new();
    for q in qsos {
        let Some(n) = q.cwops else { continue };
        let band = q.band.as_deref();
        all.insert(n);
        let year = utc(q.time).year();
        per_year.entry(year).or_default().insert(n);
        if let Some(b) = band {
            per_year_band.entry(year).or_default().entry(b).or_default().insert(n);
            if q.time >= cma_from {
                cma.entry(b).or_default().insert(n);
            }
        }
        if let Some(d) = q.dxcc.filter(|&d| d != 0) {
            dxcc.insert(d);
            if let Some(b) = band {
                dxcc_b.entry(b).or_default().insert(d);
            }
        }
        let state = q.state.as_deref().and_then(|s| US_STATES.binary_search_by(|(c, _)| (*c).cmp(s)).ok());
        if let (Some(i), true) = (state, matches!(q.dxcc, None | Some(291 | 6 | 110))) {
            was.insert(i);
            if let Some(b) = band {
                was_b.entry(b).or_default().insert(i);
            }
        }
    }
    let years = per_year
        .iter()
        .rev()
        .map(|(&year, set)| YearRow {
            year,
            aca: set.len(),
            acma: (year >= ACMA_FROM_YEAR).then(|| per_year_band.get(&year).map_or(0, |b| b.values().map(HashSet::len).sum())),
        })
        .collect();
    let cma_bands = set_counts(&cma);
    Cwops {
        members: all.len(),
        years,
        cma_total: cma_bands.values().sum(),
        cma_bands,
        dxcc: dxcc.len(),
        dxcc_bands: set_counts(&dxcc_b),
        was: was.len(),
        was_bands: set_counts(&was_b),
    }
}

fn cwt_medals(qsos: &[ClubQso], opts: CwtOptions) -> Cwt {
    let (per_point, thresholds) = cwt_thresholds(opts.region);
    let mut sessions: HashMap<(i64, u32), HashSet<(&str, &str)>> = HashMap::new();
    for q in qsos.iter().filter(|q| !opts.tagged_only || q.cwt_tagged) {
        if let Some(s) = cwt_session(q.time) {
            // The same call on another band is a new contact, as in the test's own dupe rule.
            sessions.entry(s).or_default().insert((q.call.as_str(), q.band.as_deref().unwrap_or_default()));
        }
    }
    let mut years: BTreeMap<i32, (usize, usize)> = BTreeMap::new();
    for ((day, _), calls) in &sessions {
        let e = years.entry(utc(day * 86_400).year()).or_default();
        e.0 += 1;
        if calls.len() >= per_point {
            e.1 += 1;
        }
    }
    let years = years
        .into_iter()
        .rev()
        .map(|(year, (sessions, points))| {
            let medal = if points >= thresholds[2] {
                "gold"
            } else if points >= thresholds[1] {
                "silver"
            } else if points >= thresholds[0] {
                "bronze"
            } else {
                ""
            };
            let next = thresholds.iter().find(|&&t| points < t).map_or(0, |t| t - points);
            CwtYear { year, sessions, points, medal: medal.into(), next }
        })
        .collect();
    Cwt { per_point, thresholds, years }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(y: i32, m: u32, d: u32, h: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap().and_utc().timestamp()
    }

    fn skcc_qso(time: i64, number: u32, suffix: &str, band: &str) -> ClubQso {
        ClubQso { call: format!("K{number}X"), time, band: Some(band.into()), skcc: parse_skcc(&format!("{number}{suffix}")), ..Default::default() }
    }

    fn award<'a>(a: &'a ClubAwards, key: &str) -> &'a Level {
        a.skcc.awards.iter().find(|l| l.key == key).unwrap()
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_skcc("1234S"), Some((1234, Rank::Senator)));
        assert_eq!(parse_skcc(" 987 t "), Some((987, Rank::Tribune)));
        assert_eq!(parse_skcc("55Tx8"), Some((55, Rank::Tribune)));
        assert_eq!(parse_skcc("12C"), Some((12, Rank::Centurion)));
        assert_eq!(parse_skcc("4321"), Some((4321, Rank::Member)));
        assert_eq!(parse_skcc("S1234"), None);
        assert_eq!(parse_skcc(""), None);
        assert_eq!(parse_cwops(" 1234 "), Some(1234));
        assert_eq!(parse_cwops("12A"), None);
        assert_eq!(parse_cwops("0"), None);
    }

    #[test]
    fn centurion_counts_members_once() {
        let mut tally = ClubTally::default();
        // 150 members, each worked twice (the second time on another band).
        for n in 1..=150u32 {
            tally.add(&skcc_qso(t(2020, 1, 1, 0) + n as i64, n, "", "40m"));
            tally.add(&skcc_qso(t(2020, 2, 1, 0) + n as i64, n, "", "20m"));
        }
        // Before the award start date: ignored by Centurion.
        tally.add(&skcc_qso(t(2001, 1, 1, 0), 999, "", "40m"));
        let a = tally.finish(CwtOptions::default());
        let c = award(&a, "centurion");
        assert_eq!((c.count, c.level, c.next), (150, 1, 50));
        assert_eq!(c.achieved.as_deref(), Some("2020-01-01"));
        assert_eq!((c.bands["40m"], c.bands["20m"]), (150, 150));
        assert_eq!(a.skcc.members, 151);
    }

    #[test]
    fn tribune_counts_from_the_centurion_day() {
        let mut tally = ClubTally::default();
        // 60 Centurion-rank members, then 40 plain members: the 100th member is worked on 2019-06-01.
        for n in 1..=60u32 {
            tally.add(&skcc_qso(t(2019, 1, 1, 0) + n as i64, n, "C", "40m"));
        }
        for n in 1001..=1040u32 {
            tally.add(&skcc_qso(t(2019, 6, 1, 0) + n as i64, n, "", "40m"));
        }
        // Later: 30 Tribunes and 10 plain members.
        for n in 2001..=2030u32 {
            tally.add(&skcc_qso(t(2019, 7, 1, 0) + n as i64, n, "T", "20m"));
        }
        for n in 3001..=3010u32 {
            tally.add(&skcc_qso(t(2019, 7, 2, 0) + n as i64, n, "", "20m"));
        }
        let a = tally.finish(CwtOptions::default());
        assert_eq!(award(&a, "centurion").count, 140);
        assert_eq!(award(&a, "centurion").achieved.as_deref(), Some("2019-06-01"));
        let tr = award(&a, "tribune");
        // The 60 early C members were worked before becoming a Centurion; only the later 30 count.
        assert_eq!(tr.count, 30);
        assert_eq!((tr.level, tr.next), (0, 50 - 30));
        assert_eq!(tr.bands["20m"], 30);
        let sen = award(&a, "senator");
        assert_eq!(sen.count, 0);
        assert!(sen.note.is_some());
    }

    #[test]
    fn senator_counts_from_tribune_x8() {
        let mut tally = ClubTally::default();
        let mut time = t(2020, 1, 1, 0);
        let mut qso = |n: u32, suffix: &str| {
            time += 60;
            tally.add(&skcc_qso(time, n, suffix, "40m"));
        };
        for n in 1..=400 {
            qso(n, "T");
        }
        // Tribune x8 (400) is reached on 2020-01-01; Senator counts T and S members from that day,
        // and members already used for Tribune count again. Plain and C members do not count.
        for n in 1..=250 {
            qso(n, "S");
        }
        qso(9001, "C");
        qso(9002, "");
        let a = tally.finish(CwtOptions::default());
        let tr = award(&a, "tribune");
        assert_eq!((tr.count, tr.level, tr.next), (401, 8, 49)); // the extra Centurion counts for Tribune
        let sen = award(&a, "senator");
        assert_eq!(sen.count, 400);
        assert_eq!((sen.level, sen.next), (2, 200));
    }

    #[test]
    fn cwops_year_band_and_places() {
        let mut tally = ClubTally::default();
        let mut add = |time, n: u32, band: &str, dxcc, state: Option<&str>| {
            tally.add(&ClubQso {
                call: format!("K{n}"),
                time,
                band: Some(band.into()),
                dxcc: Some(dxcc),
                state: state.map(String::from),
                cwops: Some(n),
                ..Default::default()
            })
        };
        add(t(2024, 3, 1, 12), 1, "20m", 291, Some("OH"));
        add(t(2024, 3, 2, 12), 1, "40m", 291, Some("OH")); // same member, another band
        add(t(2024, 3, 3, 12), 2, "20m", 223, None);
        add(t(2023, 3, 3, 12), 2, "20m", 223, None);
        add(t(2009, 3, 3, 12), 3, "20m", 291, Some("PA")); // before CMA, still ACA 2009
        let a = tally.finish(CwtOptions::default()).cwops;
        assert_eq!(a.members, 3);
        let y24 = a.years.iter().find(|y| y.year == 2024).unwrap();
        assert_eq!((y24.aca, y24.acma), (2, Some(3)));
        assert_eq!(a.years.iter().find(|y| y.year == 2023).unwrap().acma, None);
        assert_eq!((a.cma_bands["20m"], a.cma_bands["40m"], a.cma_total), (2, 1, 3));
        assert_eq!(a.dxcc, 2);
        assert_eq!(a.was, 2);
        assert_eq!(a.was_bands["40m"], 1);
    }

    #[test]
    fn cwt_hours_points_and_medals() {
        // 2024-01-03 is a Wednesday.
        assert!(cwt_session(t(2024, 1, 3, 13)).is_some());
        assert!(cwt_session(t(2024, 1, 3, 19)).is_some());
        assert!(cwt_session(t(2024, 1, 4, 3)).is_some());
        assert!(cwt_session(t(2024, 1, 4, 7)).is_some());
        assert!(cwt_session(t(2024, 1, 3, 14)).is_none());
        assert!(cwt_session(t(2024, 1, 4, 13)).is_none());

        let mut tally = ClubTally::default();
        let mut add = |time: i64, call: &str, tagged| {
            tally.add(&ClubQso { call: call.into(), time, cwt_tagged: tagged, ..Default::default() });
        };
        // One 13Z hour with 10 different calls (one worked twice): a point. Another with 9: no point.
        for i in 0..10 {
            add(t(2024, 1, 3, 13) + i * 60, &format!("W{i}AA"), i % 2 == 0);
        }
        add(t(2024, 1, 3, 13) + 900, "W0AA", true);
        for i in 0..9 {
            add(t(2024, 1, 3, 19) + i * 60, &format!("W{i}AA"), true);
        }
        // Outside any CWT hour.
        for i in 0..10 {
            add(t(2024, 1, 3, 15) + i * 60, &format!("W{i}BB"), true);
        }
        let all = tally.finish(CwtOptions::default()).cwt;
        assert_eq!((all.per_point, all.thresholds), (10, [50, 80, 120]));
        assert_eq!(all.years.len(), 1);
        let y = &all.years[0];
        assert_eq!((y.year, y.sessions, y.points, y.medal.as_str(), y.next), (2024, 2, 1, "", 49));
    }

    #[test]
    fn cwt_same_call_counts_on_another_band_only() {
        let mut tally = ClubTally::default();
        let base = t(2024, 1, 3, 13);
        // 5 calls on 20m, then the same 5 again on 20m (dupes) and then on 15m (new contacts).
        for (n, band) in [(0, "20m"), (1, "20m"), (2, "15m")] {
            for i in 0..5 {
                tally.add(&ClubQso { call: format!("W{i}AA"), time: base + (n * 5 + i) * 60, band: Some(band.into()), ..Default::default() });
            }
        }
        let y = &tally.finish(CwtOptions::default()).cwt.years[0];
        assert_eq!(y.points, 1); // 5 on 20m + 5 on 15m = 10
        let mut tally = ClubTally::default();
        for n in 0..2 {
            for i in 0..5 {
                tally.add(&ClubQso { call: format!("W{i}AA"), time: base + (n * 5 + i) * 60, band: Some("20m".into()), ..Default::default() });
            }
        }
        assert_eq!(tally.finish(CwtOptions::default()).cwt.years[0].points, 0);
    }

    #[test]
    fn cwt_tagged_only_and_other_continents() {
        let mut tally = ClubTally::default();
        for i in 0..6 {
            // Half tagged: 3 tagged calls, 6 calls in all.
            tally.add(&ClubQso { call: format!("W{i}AA"), time: t(2024, 1, 3, 13) + i * 60, cwt_tagged: i < 3, ..Default::default() });
        }
        let other = CwtOptions { region: Region::Other, tagged_only: false };
        let strict = CwtOptions { region: Region::Other, tagged_only: true };
        let again = ClubTally { qsos: tally.qsos.clone() };
        assert_eq!(tally.finish(other).cwt.years[0].points, 1);
        assert!(again.finish(strict).cwt.years[0].points == 0);
    }

    #[test]
    fn medal_levels() {
        let mut tally = ClubTally::default();
        // Every Wednesday 13Z of 2024 with 10 different calls: 52 points is bronze for NA/EU.
        let first = t(2024, 1, 3, 13);
        for week in 0..82 {
            if first + week * 7 * 86_400 >= t(2025, 1, 1, 0) {
                break;
            }
            for i in 0..10 {
                tally.add(&ClubQso { call: format!("W{i}AA"), time: first + week * 7 * 86_400 + i * 60, ..Default::default() });
            }
        }
        let cwt = tally.finish(CwtOptions::default()).cwt;
        let y = cwt.years.iter().find(|y| y.year == 2024).unwrap();
        assert_eq!((y.points, y.medal.as_str(), y.next), (52, "bronze", 28));
    }

    #[test]
    fn tally_is_fast() {
        let mut t0 = ClubTally::default();
        for i in 0..200_000i64 {
            t0.add(&ClubQso {
                call: format!("K{}", i % 5000),
                time: t(2015, 1, 1, 0) + i * 600,
                band: Some(["40m", "20m", "15m"][i as usize % 3].into()),
                dxcc: Some((i % 300) as u32 + 1),
                skcc: Some(((i % 4000) as u32 + 1, Rank::Tribune)),
                cwops: Some((i % 1500) as u32 + 1),
                ..Default::default()
            });
        }
        let start = std::time::Instant::now();
        let a = t0.finish(CwtOptions::default());
        assert!(a.skcc.members > 0);
        assert!(start.elapsed().as_millis() < 2000, "{:?}", start.elapsed());
    }
}
