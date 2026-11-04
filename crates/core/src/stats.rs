//! Log statistics: DXCC progress by year, confirmation rates by service, the most-worked
//! entities and callsigns, activity by band, mode, continent and hour, and the places worked
//! (for the map). Pure logic, no database; the store feeds one [`StatsQso`] per QSO.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike};

/// How many rows the most-worked lists hold.
pub const TOP: usize = 10;

/// One QSO's facts for the statistics.
#[derive(Clone, Debug, Default)]
pub struct StatsQso {
    pub call: String,
    /// Unix seconds, UTC.
    pub time_on: i64,
    pub dxcc: Option<u32>,
    /// Whether the QSO is inside the chosen period. The others only feed the by-year progress,
    /// which needs the whole history to know when an entity was first worked.
    pub in_range: bool,
    pub band: String,
    /// The submode when there is one, else the mode.
    pub mode: String,
    /// GRIDSQUARE (the other station) and MY_GRIDSQUARE.
    pub grid: String,
    pub my_grid: String,
    pub lotw_sent: bool,
    pub lotw: bool,
    pub paper_sent: bool,
    pub paper: bool,
    pub qrz_sent: bool,
    pub qrz: bool,
    pub clublog_sent: bool,
    pub clublog: bool,
    pub eqsl_sent: bool,
    pub eqsl: bool,
}

/// Which QSOs the statistics cover.
#[derive(Clone, Debug)]
pub struct StatsFilter {
    /// First second counted (UTC).
    pub from: i64,
    /// First second not counted.
    pub to: i64,
    pub band: Option<String>,
    /// "CW", "PHONE" or "DIGITAL" (see [`crate::awards::mode_group`]).
    pub group: Option<&'static str>,
}

impl Default for StatsFilter {
    fn default() -> Self {
        Self { from: i64::MIN, to: i64::MAX, band: None, group: None }
    }
}

/// What the tally needs to know about an entity.
#[derive(Clone, Debug, Default)]
pub struct EntityInfo {
    pub name: String,
    pub cont: String,
    /// Degrees north and east.
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct YearRow {
    pub year: i32,
    pub qsos: usize,
    /// Entities first worked this year.
    pub new_entities: usize,
    /// Entities worked up to and including this year.
    pub total_entities: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ServiceRow {
    pub name: String,
    /// QSOs sent to the service (a confirmed QSO counts as sent).
    pub sent: usize,
    pub confirmed: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Ranked {
    pub name: String,
    pub qsos: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct BandRow {
    pub band: String,
    pub qsos: usize,
    pub entities: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ModeRow {
    pub mode: String,
    /// "CW", "Phone" or "Digital".
    pub group: String,
    pub qsos: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ContinentRow {
    pub cont: String,
    pub qsos: usize,
    pub entities: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct DayRow {
    /// "YYYY-MM-DD".
    pub date: String,
    pub qsos: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Farthest {
    pub call: String,
    pub km: f64,
    pub time_on: i64,
}

/// A place worked, for the map: a 4-character grid square, or the middle of the entity when the
/// QSO has no grid.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MapPoint {
    pub lat: f64,
    pub lon: f64,
    pub qsos: usize,
    pub label: String,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Stats {
    pub total_qsos: usize,
    pub unique_calls: usize,
    pub entities_worked: usize,
    /// Entities with a LoTW or paper confirmation (the awards' default sources).
    pub entities_confirmed: usize,
    /// First and last QSO in the period (unix seconds).
    pub first_qso: Option<i64>,
    pub last_qso: Option<i64>,
    /// Days with at least one QSO.
    pub active_days: usize,
    pub busiest_day: Option<DayRow>,
    /// The longest run of days in a row with QSOs: its length and first day.
    pub streak_days: usize,
    pub streak_from: Option<String>,
    pub farthest: Option<Farthest>,
    pub years: Vec<YearRow>,
    pub bands: Vec<BandRow>,
    pub modes: Vec<ModeRow>,
    pub continents: Vec<ContinentRow>,
    /// QSOs per UTC hour, 0 to 23.
    pub hours: Vec<usize>,
    pub points: Vec<MapPoint>,
    pub services: Vec<ServiceRow>,
    pub top_entities: Vec<Ranked>,
    pub top_calls: Vec<Ranked>,
    /// The summary as plain text, ready to copy.
    pub text: String,
}

/// Centre of a Maidenhead locator of 4, 6 or 8 characters, as (lat, lon).
pub fn grid_center(grid: &str) -> Option<(f64, f64)> {
    let g = grid.trim().as_bytes();
    if !matches!(g.len(), 4 | 6 | 8) {
        return None;
    }
    let g: Vec<u8> = g.iter().map(u8::to_ascii_uppercase).collect();
    let field = |c: u8| (b'A'..=b'R').contains(&c).then(|| f64::from(c - b'A'));
    let digit = |c: u8| c.is_ascii_digit().then(|| f64::from(c - b'0'));
    let sub = |c: u8| (b'A'..=b'X').contains(&c).then(|| f64::from(c - b'A'));
    let mut lon = -180.0 + field(g[0])? * 20.0 + digit(g[2])? * 2.0;
    let mut lat = -90.0 + field(g[1])? * 10.0 + digit(g[3])?;
    let (mut w, mut h) = (2.0, 1.0);
    if g.len() >= 6 {
        lon += sub(g[4])? * (5.0 / 60.0);
        lat += sub(g[5])? * (2.5 / 60.0);
        (w, h) = (5.0 / 60.0, 2.5 / 60.0);
    }
    if g.len() == 8 {
        lon += digit(g[6])? * (0.5 / 60.0);
        lat += digit(g[7])? * (0.25 / 60.0);
        (w, h) = (0.5 / 60.0, 0.25 / 60.0);
    }
    Some((lat + h / 2.0, lon + w / 2.0))
}

/// Great-circle distance in kilometres.
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la, lb) = (a.0.to_radians(), b.0.to_radians());
    let dl = (b.1 - a.1).to_radians();
    let h = ((lb - la) / 2.0).sin().powi(2) + la.cos() * lb.cos() * (dl / 2.0).sin().powi(2);
    6371.0088 * 2.0 * h.sqrt().min(1.0).asin()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PointKey {
    Grid(String),
    Dxcc(u32),
}

fn day_text(day: i64) -> String {
    DateTime::from_timestamp(day * 86_400, 0).map_or_else(String::new, |d| d.format("%Y-%m-%d").to_string())
}

fn group_name(mode: &str) -> &'static str {
    match crate::awards::mode_group(mode) {
        Some("CW") => "CW",
        Some("PHONE") => "Phone",
        _ => "Digital",
    }
}

/// Counts QSOs as they stream by.
pub struct StatsTally {
    entities_info: BTreeMap<u32, EntityInfo>,
    /// The years the chosen period touches; the by-year table shows only these.
    year_span: (i32, i32),
    total: usize,
    calls: HashMap<String, usize>,
    /// Year each entity was first worked, over the whole (band and mode filtered) history.
    first_year: HashMap<u32, i32>,
    /// QSOs per year over the same history.
    all_years: BTreeMap<i32, usize>,
    /// In the period, per entity: QSO count and whether confirmed.
    entities: HashMap<u32, (usize, bool)>,
    /// (sent, confirmed) for LoTW, paper, QRZ, Club Log and eQSL.
    services: [(usize, usize); 5],
    first: Option<i64>,
    last: Option<i64>,
    days: HashMap<i64, usize>,
    hours: [usize; 24],
    bands: HashMap<String, (usize, HashSet<u32>)>,
    modes: HashMap<String, usize>,
    conts: HashMap<String, (usize, HashSet<u32>)>,
    farthest: Option<Farthest>,
    points: HashMap<PointKey, usize>,
}

impl StatsTally {
    /// `entities` maps DXCC numbers to their names, continents and middles.
    pub fn new(entities: BTreeMap<u32, EntityInfo>, filter: &StatsFilter) -> Self {
        let year = |t: i64| DateTime::from_timestamp(t, 0).map_or(1970, |d| d.year());
        let year_span = (if filter.from == i64::MIN { i32::MIN } else { year(filter.from) }, if filter.to == i64::MAX { i32::MAX } else { year(filter.to - 1) });
        Self {
            entities_info: entities,
            year_span,
            total: 0,
            calls: HashMap::new(),
            first_year: HashMap::new(),
            all_years: BTreeMap::new(),
            entities: HashMap::new(),
            services: [(0, 0); 5],
            first: None,
            last: None,
            days: HashMap::new(),
            hours: [0; 24],
            bands: HashMap::new(),
            modes: HashMap::new(),
            conts: HashMap::new(),
            farthest: None,
            points: HashMap::new(),
        }
    }

    pub fn add(&mut self, q: &StatsQso) {
        let when = DateTime::from_timestamp(q.time_on, 0);
        let year = when.map_or(1970, |d| d.year());
        *self.all_years.entry(year).or_default() += 1;
        if let Some(d) = q.dxcc {
            let y = self.first_year.entry(d).or_insert(year);
            *y = (*y).min(year);
        }
        if !q.in_range {
            return;
        }
        self.total += 1;
        self.first = Some(self.first.map_or(q.time_on, |f| f.min(q.time_on)));
        self.last = Some(self.last.map_or(q.time_on, |l| l.max(q.time_on)));
        *self.days.entry(q.time_on.div_euclid(86_400)).or_default() += 1;
        if let Some(d) = when {
            use chrono::Timelike;
            self.hours[d.hour() as usize] += 1;
        }
        if !q.call.is_empty() {
            match self.calls.get_mut(&q.call) {
                Some(n) => *n += 1,
                None => {
                    self.calls.insert(q.call.clone(), 1);
                }
            }
        }
        let info = q.dxcc.and_then(|d| self.entities_info.get(&d));
        if let Some(d) = q.dxcc {
            let e = self.entities.entry(d).or_insert((0, false));
            e.0 += 1;
            e.1 |= q.lotw || q.paper;
        }
        if !q.band.is_empty() {
            let b = self.bands.entry(q.band.clone()).or_default();
            b.0 += 1;
            b.1.extend(q.dxcc);
        }
        if !q.mode.is_empty() {
            *self.modes.entry(q.mode.to_ascii_uppercase()).or_default() += 1;
        }
        if let Some(i) = info.filter(|i| !i.cont.is_empty()) {
            let c = self.conts.entry(i.cont.clone()).or_default();
            c.0 += 1;
            c.1.extend(q.dxcc);
        }
        // Where the other station is: its grid, else the middle of its entity.
        let grid4 = q.grid.get(..4).filter(|g| grid_center(g).is_some()).map(str::to_ascii_uppercase);
        let there = grid_center(&q.grid).or_else(|| info.map(|i| (i.lat, i.lon)));
        if let Some(g) = grid4 {
            *self.points.entry(PointKey::Grid(g)).or_default() += 1;
        } else if let Some(d) = q.dxcc.filter(|d| self.entities_info.contains_key(d)) {
            *self.points.entry(PointKey::Dxcc(d)).or_default() += 1;
        }
        if let (Some(a), Some(b)) = (grid_center(&q.my_grid), there) {
            let km = distance_km(a, b);
            if self.farthest.as_ref().is_none_or(|f| km > f.km) {
                self.farthest = Some(Farthest { call: q.call.clone(), km, time_on: q.time_on });
            }
        }
        let flags = [(q.lotw_sent, q.lotw), (q.paper_sent, q.paper), (q.qrz_sent, q.qrz), (q.clublog_sent, q.clublog), (q.eqsl_sent, q.eqsl)];
        for (s, (sent, got)) in self.services.iter_mut().zip(flags) {
            s.0 += usize::from(sent || got);
            s.1 += usize::from(got);
        }
    }

    /// The finished statistics. `label` describes the filters, for the head of the text.
    pub fn finish(self, label: &str) -> Stats {
        let mut new_by_year: BTreeMap<i32, usize> = BTreeMap::new();
        for year in self.first_year.values() {
            *new_by_year.entry(*year).or_default() += 1;
        }
        let mut total_entities = 0;
        let years: Vec<YearRow> = self
            .all_years
            .iter()
            .map(|(&year, &qsos)| {
                let new_entities = new_by_year.get(&year).copied().unwrap_or(0);
                total_entities += new_entities;
                YearRow { year, qsos, new_entities, total_entities }
            })
            .filter(|y| y.year >= self.year_span.0 && y.year <= self.year_span.1)
            .collect();
        let labels = ["LoTW", "Paper QSL", "QRZ Logbook", "Club Log", "eQSL"];
        let services = labels
            .iter()
            .zip(self.services)
            .map(|(n, (sent, confirmed))| ServiceRow { name: (*n).into(), sent, confirmed })
            .collect();
        let name_of = |d: &u32| self.entities_info.get(d).map_or_else(|| format!("DXCC {d}"), |i| i.name.clone());
        let top = |mut v: Vec<Ranked>| {
            v.sort_by(|a, b| b.qsos.cmp(&a.qsos).then_with(|| a.name.cmp(&b.name)));
            v.truncate(TOP);
            v
        };
        let top_entities = top(self.entities.iter().map(|(d, (n, _))| Ranked { name: name_of(d), qsos: *n }).collect());
        let top_calls = top(self.calls.iter().map(|(c, n)| Ranked { name: c.clone(), qsos: *n }).collect());

        let mut bands: Vec<BandRow> = self.bands.iter().map(|(b, (n, e))| BandRow { band: b.clone(), qsos: *n, entities: e.len() }).collect();
        let rank = |b: &str| crate::band::BANDS.iter().position(|x| x.0.eq_ignore_ascii_case(b)).unwrap_or(usize::MAX);
        bands.sort_by(|a, b| rank(&a.band).cmp(&rank(&b.band)).then_with(|| a.band.cmp(&b.band)));
        let mut modes: Vec<ModeRow> = self.modes.iter().map(|(m, n)| ModeRow { mode: m.clone(), group: group_name(m).into(), qsos: *n }).collect();
        modes.sort_by(|a, b| b.qsos.cmp(&a.qsos).then_with(|| a.mode.cmp(&b.mode)));
        let mut continents: Vec<ContinentRow> = self.conts.iter().map(|(c, (n, e))| ContinentRow { cont: c.clone(), qsos: *n, entities: e.len() }).collect();
        continents.sort_by(|a, b| b.qsos.cmp(&a.qsos).then_with(|| a.cont.cmp(&b.cont)));

        let busiest_day = self.days.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(d, n)| DayRow { date: day_text(*d), qsos: *n });
        let mut days: Vec<i64> = self.days.keys().copied().collect();
        days.sort_unstable();
        let (mut best, mut best_from, mut run, mut run_from) = (0usize, 0i64, 0usize, 0i64);
        let mut prev = i64::MIN;
        for d in &days {
            if *d == prev + 1 {
                run += 1;
            } else {
                run = 1;
                run_from = *d;
            }
            if run > best {
                best = run;
                best_from = run_from;
            }
            prev = *d;
        }

        let mut points: Vec<MapPoint> = self
            .points
            .iter()
            .filter_map(|(k, n)| match k {
                PointKey::Grid(g) => grid_center(g).map(|(lat, lon)| MapPoint { lat, lon, qsos: *n, label: g.clone() }),
                PointKey::Dxcc(d) => self.entities_info.get(d).map(|i| MapPoint { lat: i.lat, lon: i.lon, qsos: *n, label: i.name.clone() }),
            })
            .collect();
        points.sort_by(|a, b| b.qsos.cmp(&a.qsos).then_with(|| a.label.cmp(&b.label)));

        let mut s = Stats {
            total_qsos: self.total,
            unique_calls: self.calls.len(),
            entities_worked: self.entities.len(),
            entities_confirmed: self.entities.values().filter(|e| e.1).count(),
            first_qso: self.first,
            last_qso: self.last,
            active_days: self.days.len(),
            busiest_day,
            streak_days: best,
            streak_from: (best > 0).then(|| day_text(best_from)),
            farthest: self.farthest,
            years,
            bands,
            modes,
            continents,
            hours: self.hours.to_vec(),
            points,
            services,
            top_entities,
            top_calls,
            text: String::new(),
        };
        s.text = s.render(label);
        s
    }
}

impl Stats {
    fn render(&self, label: &str) -> String {
        use std::fmt::Write;
        let date = |t: i64| day_text(t.div_euclid(86_400));
        let mut o = String::new();
        let _ = writeln!(o, "{label}\n");
        let _ = writeln!(o, "QSOs: {}   Callsigns: {}", self.total_qsos, self.unique_calls);
        if let (Some(a), Some(b)) = (self.first_qso, self.last_qso) {
            let _ = writeln!(o, "First QSO: {}   Last QSO: {}   Days on the air: {}", date(a), date(b), self.active_days);
        }
        if let Some(d) = &self.busiest_day {
            let _ = write!(o, "Busiest day: {} ({} QSOs)", d.date, d.qsos);
            if let (true, Some(from)) = (self.streak_days > 1, &self.streak_from) {
                let _ = write!(o, "   Longest streak: {} days from {}", self.streak_days, from);
            }
            let _ = writeln!(o);
        }
        if let Some(f) = &self.farthest {
            let _ = writeln!(o, "Farthest: {} at {:.0} km ({:.0} mi), {}", f.call, f.km, f.km / 1.609_344, date(f.time_on));
        }
        let _ = writeln!(o, "DXCC entities: {} worked, {} confirmed (LoTW or paper)\n", self.entities_worked, self.entities_confirmed);
        let _ = writeln!(o, "DXCC progress by year\n{:<6} {:>8} {:>6} {:>7}", "Year", "QSOs", "New", "Total");
        for y in &self.years {
            let _ = writeln!(o, "{:<6} {:>8} {:>6} {:>7}", y.year, y.qsos, y.new_entities, y.total_entities);
        }
        if !self.bands.is_empty() {
            let _ = writeln!(o, "\nBy band\n{:<6} {:>8} {:>9}", "Band", "QSOs", "Entities");
            for b in &self.bands {
                let _ = writeln!(o, "{:<6} {:>8} {:>9}", b.band, b.qsos, b.entities);
            }
        }
        if !self.modes.is_empty() {
            let _ = writeln!(o, "\nBy mode\n{:<10} {:<8} {:>8}", "Mode", "Group", "QSOs");
            for m in &self.modes {
                let _ = writeln!(o, "{:<10} {:<8} {:>8}", m.mode, m.group, m.qsos);
            }
        }
        if !self.continents.is_empty() {
            let _ = writeln!(o, "\nBy continent\n{:<6} {:>8} {:>9}", "Cont", "QSOs", "Entities");
            for c in &self.continents {
                let _ = writeln!(o, "{:<6} {:>8} {:>9}", c.cont, c.qsos, c.entities);
            }
        }
        let mut hours: Vec<(usize, usize)> = self.hours.iter().copied().enumerate().filter(|h| h.1 > 0).collect();
        hours.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if !hours.is_empty() {
            let top: Vec<String> = hours.iter().take(5).map(|(h, n)| format!("{h:02}Z ({n})")).collect();
            let _ = writeln!(o, "\nBusiest UTC hours: {}", top.join(", "));
        }
        let _ = writeln!(o, "\nConfirmation rates\n{:<12} {:>8} {:>10} {:>6}", "Service", "Sent", "Confirmed", "Rate");
        for s in &self.services {
            let rate = if s.sent == 0 { "-".to_string() } else { format!("{:.1}%", s.confirmed as f64 * 100.0 / s.sent as f64) };
            let _ = writeln!(o, "{:<12} {:>8} {:>10} {:>6}", s.name, s.sent, s.confirmed, rate);
        }
        let _ = writeln!(o, "\nMost worked entities");
        for (i, r) in self.top_entities.iter().enumerate() {
            let _ = writeln!(o, "{:>2}. {:<32} {:>7}", i + 1, r.name, r.qsos);
        }
        let _ = writeln!(o, "\nMost worked callsigns");
        for (i, r) in self.top_calls.iter().enumerate() {
            let _ = writeln!(o, "{:>2}. {:<32} {:>7}", i + 1, r.name, r.qsos);
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ymd: (i32, u32, u32), hour: u32) -> i64 {
        chrono::NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).unwrap().and_hms_opt(hour, 0, 0).unwrap().and_utc().timestamp()
    }

    fn q(call: &str, ymd: (i32, u32, u32), dxcc: u32) -> StatsQso {
        StatsQso { call: call.into(), time_on: at(ymd, 12), dxcc: Some(dxcc), in_range: true, band: "20m".into(), mode: "CW".into(), ..Default::default() }
    }

    fn geo() -> BTreeMap<u32, EntityInfo> {
        BTreeMap::from([
            (291, EntityInfo { name: "United States".into(), cont: "NA".into(), lat: 37.5, lon: -91.0 }),
            (230, EntityInfo { name: "Germany".into(), cont: "EU".into(), lat: 51.0, lon: 10.0 }),
        ])
    }

    #[test]
    fn counts_years_entities_and_rates() {
        let mut t = StatsTally::new(geo(), &StatsFilter::default());
        let mut a = q("K1ABC", (2023, 5, 1), 291);
        a.lotw_sent = true;
        a.lotw = true;
        let mut b = q("K1ABC", (2024, 5, 1), 291);
        b.lotw_sent = true;
        let mut c = q("DL1X", (2024, 6, 1), 230);
        c.paper = true;
        c.clublog_sent = true;
        for x in [&a, &b, &c] {
            t.add(x);
        }
        let s = t.finish("All");
        assert_eq!(s.years.iter().map(|y| (y.year, y.qsos, y.new_entities, y.total_entities)).collect::<Vec<_>>(), [(2023, 1, 1, 1), (2024, 2, 1, 2)]);
        assert_eq!((s.entities_worked, s.entities_confirmed, s.unique_calls), (2, 2, 2));
        assert_eq!((s.services[0].sent, s.services[0].confirmed), (2, 1));
        assert_eq!((s.services[1].sent, s.services[1].confirmed), (1, 1));
        assert_eq!((s.services[3].sent, s.services[3].confirmed), (1, 0));
        assert_eq!(s.top_entities[0], Ranked { name: "United States".into(), qsos: 2 });
        assert_eq!(s.top_calls[0], Ranked { name: "K1ABC".into(), qsos: 2 });
        assert_eq!(s.continents.iter().map(|c| (c.cont.as_str(), c.qsos)).collect::<Vec<_>>(), [("NA", 2), ("EU", 1)]);
        assert!(s.text.contains("50.0%") && s.text.contains("2024") && s.text.contains("By band"));
    }

    #[test]
    fn the_period_limits_counts_but_not_the_history() {
        // 2024 only: the 2023 QSO still decides that 2024's K1ABC QSO is not a new entity.
        let filter = StatsFilter { from: at((2024, 1, 1), 0), to: at((2025, 1, 1), 0), ..Default::default() };
        let mut t = StatsTally::new(geo(), &filter);
        let mut old = q("K1ABC", (2023, 5, 1), 291);
        old.in_range = false;
        t.add(&old);
        t.add(&q("K1ABC", (2024, 5, 1), 291));
        t.add(&q("DL1X", (2024, 6, 1), 230));
        let s = t.finish("2024");
        assert_eq!(s.total_qsos, 2);
        assert_eq!(s.years.iter().map(|y| (y.year, y.qsos, y.new_entities, y.total_entities)).collect::<Vec<_>>(), [(2024, 2, 1, 2)]);
    }

    #[test]
    fn activity_streaks_bands_and_places() {
        let mut t = StatsTally::new(geo(), &StatsFilter::default());
        for (d, call) in [(1, "A1"), (2, "A2"), (3, "A3"), (3, "A4"), (7, "A5")] {
            t.add(&q(call, (2024, 3, d), 291));
        }
        let mut b = q("DL1X", (2024, 3, 8), 230);
        b.band = "40m".into();
        b.grid = "jo62mm".into();
        b.my_grid = "EM38".into();
        t.add(&b);
        let s = t.finish("");
        assert_eq!((s.active_days, s.streak_days, s.streak_from.as_deref()), (5, 3, Some("2024-03-01")));
        assert_eq!(s.busiest_day, Some(DayRow { date: "2024-03-03".into(), qsos: 2 }));
        // Longest wavelength first.
        assert_eq!(s.bands.iter().map(|b| (b.band.as_str(), b.qsos)).collect::<Vec<_>>(), [("40m", 1), ("20m", 5)]);
        assert_eq!(s.hours[12], 6);
        // Five US QSOs share the entity's middle; the German one has its grid.
        assert_eq!(s.points.iter().map(|p| (p.label.as_str(), p.qsos)).collect::<Vec<_>>(), [("United States", 5), ("JO62", 1)]);
        let f = s.farthest.unwrap();
        assert_eq!(f.call, "DL1X");
        assert!((7_000.0..8_500.0).contains(&f.km), "{}", f.km);
    }

    #[test]
    fn grids_and_distances() {
        let (lat, lon) = grid_center("FN31pr").unwrap();
        assert!((lat - 41.73).abs() < 0.05 && (lon + 72.7).abs() < 0.05);
        assert!(grid_center("FN3").is_none() && grid_center("ZZ99").is_none());
        assert!((distance_km((0.0, 0.0), (0.0, 90.0)) - 10_007.5).abs() < 5.0);
    }

    #[test]
    fn empty_log_renders() {
        let s = StatsTally::new(BTreeMap::new(), &StatsFilter::default()).finish("");
        assert_eq!(s.total_qsos, 0);
        assert!(s.text.contains("Most worked callsigns"));
    }
}
