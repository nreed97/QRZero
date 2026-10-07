//! DXCC entity resolution from AD1C's country files (<https://www.country-files.com>).
//!
//! Both `cty.csv` (preferred: carries ADIF DXCC entity codes) and `cty.dat` are supported.
//! Country files give longitudes positive *west*; [`Entity::lon`] is the usual east-positive.

use std::collections::HashMap;
use std::str::FromStr;

/// A DXCC (or WAE-only) entity, with any per-prefix/per-call overrides already applied.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Entity {
    /// Country name as in the country file, e.g. `United States`.
    pub name: String,
    /// The entity's primary prefix, e.g. `K` (without the WAE `*` marker).
    pub prefix: String,
    /// ADIF DXCC entity code; `None` for WAE-only entities and for `cty.dat` data.
    pub dxcc: Option<u32>,
    /// Continent abbreviation (`NA`, `EU`, ...).
    pub cont: String,
    /// CQ zone.
    pub cq: u8,
    /// ITU zone.
    pub itu: u8,
    /// Latitude, degrees north.
    pub lat: f64,
    /// Longitude, degrees east.
    pub lon: f64,
}

/// A country file that could not be parsed.
#[derive(Debug, thiserror::Error)]
pub enum CtyError {
    /// A malformed entity record.
    #[error("country file line {line}: {msg}")]
    Parse { line: usize, msg: String },
    /// The file held no entities.
    #[error("no entities found in country file")]
    Empty,
}

/// A prefix or exact call pointing at an entity, plus its overrides.
#[derive(Clone, Debug, Default)]
struct Hit {
    ent: usize,
    cq: Option<u8>,
    itu: Option<u8>,
    cont: Option<String>,
    latlon: Option<(f64, f64)>,
}

/// Callsign → entity database built from a country file.
#[derive(Clone, Debug, Default)]
pub struct CtyDb {
    entities: Vec<Entity>,
    exact: HashMap<String, Hit>,
    prefixes: HashMap<String, Hit>,
    /// Longest prefix in bytes, bounding the prefix search.
    max_prefix: usize,
}

fn num<T: FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("bad {what} {:?}", s.trim()))
}

/// Parses one prefix-list token such as `=W1AW(5)[8]` into (exact?, key, overrides).
fn parse_token(tok: &str) -> Result<(bool, String, Hit), String> {
    let (exact, tok) = match tok.strip_prefix('=') {
        Some(t) => (true, t),
        None => (false, tok),
    };
    let split = tok.find(['(', '[', '<', '{', '~']).unwrap_or(tok.len());
    let (key, mut rest) = tok.split_at(split);
    if key.is_empty() {
        return Err(format!("empty prefix in {tok:?}"));
    }
    let mut hit = Hit::default();
    while let Some(open) = rest.chars().next() {
        let close = match open {
            '(' => ')',
            '[' => ']',
            '<' => '>',
            '{' => '}',
            '~' => '~',
            _ => return Err(format!("unexpected {open:?} in {tok:?}")),
        };
        let end = rest[1..]
            .find(close)
            .ok_or_else(|| format!("unclosed {open:?} in {tok:?}"))?
            + 1;
        let body = &rest[1..end];
        match open {
            '(' => hit.cq = Some(num(body, "CQ zone")?),
            '[' => hit.itu = Some(num(body, "ITU zone")?),
            '<' => {
                let (lat, lon) = body
                    .split_once('/')
                    .ok_or_else(|| format!("bad lat/lon {body:?}"))?;
                hit.latlon = Some((num(lat, "latitude")?, -num::<f64>(lon, "longitude")?));
            }
            '{' => hit.cont = Some(body.trim().to_ascii_uppercase()),
            _ => {} // ~gmt~: not part of Entity
        }
        rest = &rest[end + 1..];
    }
    Ok((exact, key.to_ascii_uppercase(), hit))
}

impl CtyDb {
    /// Adds an entity and its prefix list (tokens separated by commas and/or whitespace).
    fn add(&mut self, entity: Entity, list: &str) -> Result<(), String> {
        let ent = self.entities.len();
        self.entities.push(entity);
        for tok in list
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .filter(|t| !t.is_empty())
        {
            let (exact, key, hit) = parse_token(tok)?;
            let hit = Hit { ent, ..hit };
            if exact {
                self.exact.insert(key, hit);
            } else {
                self.max_prefix = self.max_prefix.max(key.len());
                self.prefixes.insert(key, hit);
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<Self, CtyError> {
        if self.entities.is_empty() {
            Err(CtyError::Empty)
        } else {
            Ok(self)
        }
    }

    /// Parses `cty.csv`: `Prefix,Name,DXCC,Cont,CQ,ITU,Lat,Lon,GMT,prefix list;` per line.
    /// Entities whose primary prefix starts with `*` (WAE-only) get `dxcc: None`.
    pub fn parse_csv(text: &str) -> Result<Self, CtyError> {
        let mut db = Self::default();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() {
                continue;
            }
            let record = || -> Result<(Entity, &str), String> {
                let (pfx, rest) = line.split_once(',').ok_or("missing fields")?;
                // Split from the right so a comma in the country name cannot shift the fields.
                let mut f: Vec<&str> = rest.rsplitn(9, ',').collect();
                f.reverse();
                let &[name, dxcc, cont, cq, itu, lat, lon, _gmt, list] = f.as_slice() else {
                    return Err("expected 10 fields".into());
                };
                let wae = pfx.starts_with('*');
                let entity = Entity {
                    name: name.trim().to_string(),
                    prefix: pfx.trim_start_matches('*').trim().to_ascii_uppercase(),
                    dxcc: if wae {
                        None
                    } else {
                        Some(num(dxcc, "DXCC number")?)
                    },
                    cont: cont.trim().to_ascii_uppercase(),
                    cq: num(cq, "CQ zone")?,
                    itu: num(itu, "ITU zone")?,
                    lat: num(lat, "latitude")?,
                    lon: -num::<f64>(lon, "longitude")?,
                };
                Ok((entity, list))
            };
            let err = |msg| CtyError::Parse { line: i + 1, msg };
            let (entity, list) = record().map_err(err)?;
            db.add(entity, list).map_err(err)?;
        }
        db.finish()
    }

    /// Parses `cty.dat`: a `Name: CQ: ITU: Cont: Lat: Lon: GMT: Prefix:` header followed by
    /// comma-separated prefixes ending with `;`. This format has no DXCC numbers (`dxcc: None`).
    pub fn parse_dat(text: &str) -> Result<Self, CtyError> {
        let mut db = Self::default();
        let mut line = 1;
        for chunk in text.split(';') {
            let body = chunk.trim_start();
            let start = line + chunk[..chunk.len() - body.len()].matches('\n').count();
            line += chunk.matches('\n').count();
            let body = body.trim_end().trim_start_matches('\u{feff}');
            if body.is_empty() {
                continue;
            }
            let record = || -> Result<(Entity, &str), String> {
                let f: Vec<&str> = body.splitn(9, ':').collect();
                let &[name, cq, itu, cont, lat, lon, _gmt, pfx, list] = f.as_slice() else {
                    return Err("expected 8 header fields".into());
                };
                let entity = Entity {
                    name: name.trim().to_string(),
                    prefix: pfx.trim().trim_start_matches('*').to_ascii_uppercase(),
                    dxcc: None,
                    cont: cont.trim().to_ascii_uppercase(),
                    cq: num(cq, "CQ zone")?,
                    itu: num(itu, "ITU zone")?,
                    lat: num(lat, "latitude")?,
                    lon: -num::<f64>(lon, "longitude")?,
                };
                Ok((entity, list))
            };
            let err = |msg| CtyError::Parse { line: start, msg };
            let (entity, list) = record().map_err(err)?;
            db.add(entity, list).map_err(err)?;
        }
        db.finish()
    }

    /// Parses either format, detecting `cty.dat` by the colons in its header line.
    pub fn parse(text: &str) -> Result<Self, CtyError> {
        match text.lines().find(|l| !l.trim().is_empty()) {
            Some(first) if first.contains(':') => Self::parse_dat(text),
            Some(_) => Self::parse_csv(text),
            None => Err(CtyError::Empty),
        }
    }

    /// Number of entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether the database has no entities.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Resolves a callsign (any case) to its entity, with per-prefix/per-call zone overrides
    /// applied. Exact `=` matches win, then the longest matching prefix.
    ///
    /// Portable calls: `/P`, `/M`, `/QRP`, `/A`, `/B`, `/R`, `/LH`, `/J` and a single call-area
    /// digit are ignored; `/MM` and `/AM` have no entity (`None`); otherwise the shorter part is
    /// taken as the prefix (`EA8/G4ABC` and `G4ABC/EA8` both resolve via `EA8`).
    pub fn lookup(&self, call: &str) -> Option<Entity> {
        let call: String = call
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if call.is_empty() {
            return None;
        }
        if let Some(hit) = self.exact.get(&call) {
            return Some(self.resolve(hit));
        }
        let base = if call.contains('/') {
            let mut parts: Vec<&str> = call.split('/').filter(|p| !p.is_empty()).collect();
            while parts.len() > 1 {
                match parts[parts.len() - 1] {
                    "MM" | "AM" => return None,
                    "P" | "M" | "QRP" | "A" | "B" | "R" | "LH" | "J" => {}
                    s if s.len() == 1 && s.as_bytes()[0].is_ascii_digit() => {}
                    _ => break,
                }
                parts.pop();
            }
            parts.iter().copied().min_by_key(|p| p.len())?
        } else {
            &call
        };
        let hit = self.exact.get(base).or_else(|| {
            (1..=base.len().min(self.max_prefix))
                .rev()
                .filter(|&n| base.is_char_boundary(n))
                .find_map(|n| self.prefixes.get(&base[..n]))
        })?;
        Some(self.resolve(hit))
    }

    fn resolve(&self, hit: &Hit) -> Entity {
        let mut e = self.entities[hit.ent].clone();
        if let Some(cq) = hit.cq {
            e.cq = cq;
        }
        if let Some(itu) = hit.itu {
            e.itu = itu;
        }
        if let Some(cont) = &hit.cont {
            e.cont.clone_from(cont);
        }
        if let Some((lat, lon)) = hit.latlon {
            (e.lat, e.lon) = (lat, lon);
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "\
K,United States,291,NA,5,8,37.53,91.67,5.0,AA AB AC AD AE AF AG AI AJ AK K N W K6(3)[6] N6(3)[6] W6(3)[6] =W1AW =N7XYZ(4)[7]<45.00/120.00> =K1XYZ{EU};
VE,Canada,1,NA,5,9,44.35,78.75,5.0,CF CG CJ CK CY VA VB VC VD VE VG VO VX VY XJ XK XL XM XN XO VA7(3)[2] VE7(3)[2];
G,England,223,EU,14,27,52.77,1.47,0.0,2E G M;
DL,Fed. Rep. of Germany,230,EU,14,28,51.00,-10.00,-1.0,DA DB DC DD DE DF DG DH DI DJ DK DL DM DN DO DP DQ DR Y2 Y3 Y4 Y5 Y6 Y7 Y8 Y9;
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;
EA,Spain,281,EU,14,37,40.37,4.88,-1.0,AM AN AO EA EB EC ED EE EF EG EH;
EA8,Canary Islands,29,AF,33,36,28.32,15.85,0.0,AM8 AN8 AO8 EA8 EB8 EC8 ED8 EE8 EF8 EG8 EH8;
KL,Alaska,6,NA,1,1,61.40,148.87,8.0,AL KL NL WL =W1XX;
KH6,Hawaii,110,OC,31,61,21.12,157.48,10.0,AH6 AH7 KH6 KH7 NH6 NH7 WH6 WH7;
KG4,Guantanamo Bay,105,NA,8,11,20.00,75.00,5.0,=KG4AA =KG4AB =KG4WW;
VP2E,Anguilla,12,NA,8,11,18.23,63.05,4.0,VP2E;
*4U1V,Vienna Intl Ctr,206,EU,15,28,48.20,-16.30,-1.0,=4U1VIC =4U2U;
";

    const DAT: &str = "\
United States:            05:  08:  NA:   37.53:    91.67:     5.0:  K:
    AA,AB,K,N,W,W6(3)[6],
    =W1AW,=N7XYZ(4)[7]<45.00/120.00>;
Canada:                   05:  09:  NA:   44.35:    78.75:     5.0:  VE:
    VA,VE,VE7(3)[2];
Fed. Rep. of Germany:     14:  28:  EU:   51.00:   -10.00:    -1.0:  DL:
    DA,DJ,DK,DL;
Canary Islands:           33:  36:  AF:   28.32:    15.85:     0.0:  EA8:
    EA8,EB8;
Spain:                    14:  37:  EU:   40.37:     4.88:    -1.0:  EA:
    EA,EB;
Vienna Intl Ctr:          15:  28:  EU:   48.20:   -16.30:    -1.0:  *4U1V:
    =4U1VIC;
";

    fn csv() -> CtyDb {
        CtyDb::parse_csv(CSV).unwrap()
    }

    fn name(db: &CtyDb, call: &str) -> Option<String> {
        db.lookup(call).map(|e| e.name)
    }

    #[test]
    fn parses_csv() {
        let db = csv();
        assert_eq!(db.len(), 12);
        let us = db.lookup("K1ABC").unwrap();
        assert_eq!(
            us,
            Entity {
                name: "United States".into(),
                prefix: "K".into(),
                dxcc: Some(291),
                cont: "NA".into(),
                cq: 5,
                itu: 8,
                lat: 37.53,
                lon: -91.67,
            }
        );
        assert_eq!(db.lookup("DL1ABC").unwrap().lon, 10.0);
        assert_eq!(db.lookup("JA1ABC").unwrap().lon, 138.38);
    }

    #[test]
    fn longest_prefix() {
        let db = csv();
        assert_eq!(name(&db, "KL7ABC").as_deref(), Some("Alaska"));
        assert_eq!(name(&db, "AL7X").as_deref(), Some("Alaska"));
        assert_eq!(name(&db, "AK1A").as_deref(), Some("United States"));
        assert_eq!(name(&db, "KH6ABC").as_deref(), Some("Hawaii"));
        assert_eq!(name(&db, "EA8ABC").as_deref(), Some("Canary Islands"));
        assert_eq!(name(&db, "EA1ABC").as_deref(), Some("Spain"));
        assert_eq!(name(&db, "M0ABC").as_deref(), Some("England"));
        assert_eq!(name(&db, "VP2EAB").as_deref(), Some("Anguilla"));
    }

    #[test]
    fn exact_calls() {
        let db = csv();
        assert_eq!(name(&db, "KG4AA").as_deref(), Some("Guantanamo Bay"));
        // A 2x3 KG4 call is an ordinary US call: Guantanamo is listed only as exact calls.
        assert_eq!(name(&db, "KG4ABC").as_deref(), Some("United States"));
        assert_eq!(name(&db, "W1XX").as_deref(), Some("Alaska"));
        let vic = db.lookup("4U1VIC").unwrap();
        assert_eq!(
            (vic.name.as_str(), vic.prefix.as_str(), vic.dxcc),
            ("Vienna Intl Ctr", "4U1V", None)
        );
        assert_eq!(vic.lon, 16.30);
    }

    #[test]
    fn overrides() {
        let db = csv();
        let w6 = db.lookup("W6ABC").unwrap();
        assert_eq!((w6.cq, w6.itu), (3, 6));
        let ve7 = db.lookup("VE7ABC").unwrap();
        assert_eq!((ve7.name.as_str(), ve7.cq, ve7.itu), ("Canada", 3, 2));
        let n7 = db.lookup("N7XYZ").unwrap();
        assert_eq!((n7.cq, n7.itu, n7.lat, n7.lon), (4, 7, 45.0, -120.0));
        assert_eq!(db.lookup("K1XYZ").unwrap().cont, "EU");
        assert_eq!(db.lookup("W1AW").unwrap().cq, 5);
    }

    #[test]
    fn portable_calls() {
        let db = csv();
        assert_eq!(name(&db, "W1ABC/P").as_deref(), Some("United States"));
        assert_eq!(name(&db, "W1ABC/4").as_deref(), Some("United States"));
        assert_eq!(name(&db, "EA8/G4ABC").as_deref(), Some("Canary Islands"));
        assert_eq!(name(&db, "G4ABC/EA8").as_deref(), Some("Canary Islands"));
        assert_eq!(name(&db, "EA8/G4ABC/P").as_deref(), Some("Canary Islands"));
        assert_eq!(name(&db, "VP2E/K1ABC").as_deref(), Some("Anguilla"));
        assert_eq!(name(&db, "KH6/K1ABC").as_deref(), Some("Hawaii"));
        assert_eq!(name(&db, "K1ABC/KH6").as_deref(), Some("Hawaii"));
        assert_eq!(name(&db, "JA1ABC/QRP/P").as_deref(), Some("Japan"));
        assert_eq!(name(&db, "KG4AA/P").as_deref(), Some("Guantanamo Bay"));
        assert_eq!(db.lookup("N7XYZ/M").unwrap().cq, 4);
        assert_eq!(db.lookup("G4ABC/MM"), None);
        assert_eq!(db.lookup("DL1ABC/AM"), None);
    }

    #[test]
    fn case_whitespace_and_unknown() {
        let db = csv();
        assert_eq!(name(&db, " ea8/g4abc ").as_deref(), Some("Canary Islands"));
        assert_eq!(name(&db, "kl7abc").as_deref(), Some("Alaska"));
        assert_eq!(db.lookup("Q1ABC"), None);
        assert_eq!(db.lookup("XZ1A"), None);
        assert_eq!(db.lookup(""), None);
        assert_eq!(db.lookup("/"), None);
    }

    #[test]
    fn parses_dat_and_detects_format() {
        let db = CtyDb::parse_dat(DAT).unwrap();
        assert_eq!(db.len(), 6);
        let ve7 = db.lookup("VE7ABC").unwrap();
        assert_eq!(
            (ve7.name.as_str(), ve7.dxcc, ve7.cq, ve7.itu, ve7.lon),
            ("Canada", None, 3, 2, -78.75)
        );
        assert_eq!(db.lookup("DL1ABC").unwrap().lon, 10.0);
        assert_eq!(db.lookup("N7XYZ").unwrap().lon, -120.0);
        assert_eq!(name(&db, "G4ABC/EA8").as_deref(), Some("Canary Islands"));
        assert_eq!(db.lookup("4U1VIC").unwrap().prefix, "4U1V");
        assert_eq!(CtyDb::parse(DAT).unwrap().len(), 6);
        assert_eq!(CtyDb::parse(CSV).unwrap().len(), 12);
    }

    #[test]
    fn errors() {
        assert!(matches!(CtyDb::parse(""), Err(CtyError::Empty)));
        assert!(matches!(
            CtyDb::parse_csv("K,United States;"),
            Err(CtyError::Parse { line: 1, .. })
        ));
        let bad = "\nG,England,223,EU,14,27,52.77,1.47,0.0,G;\nK,US,291,NA,x,8,0,0,0,K;";
        assert!(matches!(
            CtyDb::parse_csv(bad),
            Err(CtyError::Parse { line: 3, .. })
        ));
        assert!(matches!(
            CtyDb::parse_dat("Canada: 5: 9: NA: 1: 2: 3: VE:\n VE(3;"),
            Err(CtyError::Parse { .. })
        ));
    }

    #[test]
    fn lookups_are_fast() {
        let db = csv();
        let calls = [
            "K1ABC",
            "EA8/G4ABC",
            "kl7xyz",
            "VE7ABC/P",
            "DL1ABC",
            "KG4AA",
            "Q1ABC",
            "JA1ABC/QRP",
        ];
        let start = std::time::Instant::now();
        let hits = (0..100_000)
            .filter(|i| db.lookup(calls[i % calls.len()]).is_some())
            .count();
        let took = start.elapsed();
        assert_eq!(hits, 100_000 / calls.len() * (calls.len() - 1));
        assert!(took.as_millis() < 500, "100k lookups took {took:?}");
    }
}
