//! In-memory "have I worked it?" sets for one log, for flagging FTx decodes and
//! cluster spots without a database query per message.

use std::collections::HashSet;

#[derive(Debug, Default, Clone)]
pub struct WorkedIndex {
    calls: HashSet<String>,
    call_bands: HashSet<(String, String)>,
    dxcc: HashSet<u32>,
    dxcc_bands: HashSet<(u32, String)>,
    dxcc_modes: HashSet<(u32, String)>,
    /// Four-character grids worked on the VHF-and-up bands (VUCC style).
    grid_bands: HashSet<(String, String)>,
}

/// What is new about a station, relative to a log.
#[derive(Debug, Default, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Needed {
    pub new_call: bool,
    pub new_call_band: bool,
    pub new_dxcc: bool,
    pub new_band: bool,
    pub new_mode: bool,
    /// A four-character grid not yet worked on this band. Only flagged on 6 m and up.
    pub new_grid: bool,
}

/// What is already worked for one DXCC entity.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DxccProfile {
    pub worked: bool,
    pub bands: Vec<String>,
    /// "CW", "PHONE" or "DIGITAL".
    pub mode_groups: Vec<&'static str>,
}

impl WorkedIndex {
    /// Adds one QSO. `mode` is the submode when there is one (FT4), else the mode.
    pub fn add(&mut self, call: &str, dxcc: Option<u32>, band: Option<&str>, mode: Option<&str>) {
        let call = call.to_ascii_uppercase();
        if let Some(b) = band {
            self.call_bands.insert((call.clone(), b.to_string()));
        }
        self.calls.insert(call);
        if let Some(d) = dxcc {
            self.dxcc.insert(d);
            if let Some(b) = band {
                self.dxcc_bands.insert((d, b.to_string()));
            }
            if let Some(m) = mode {
                self.dxcc_modes.insert((d, m.to_string()));
            }
        }
    }

    /// Adds the grid of one QSO. Only the VHF-and-up bands are kept; on HF nobody chases grids.
    pub fn add_grid(&mut self, grid: &str, band: &str) {
        if let (true, Some(g)) = (is_vhf_up(band), crate::awards::grid4(grid)) {
            self.grid_bands.insert((g, band.to_string()));
        }
    }

    /// Which bands and mode groups (CW, PHONE, DIGITAL) are worked for an entity.
    pub fn dxcc_profile(&self, dxcc: u32) -> DxccProfile {
        let mut p = DxccProfile { worked: self.dxcc.contains(&dxcc), ..DxccProfile::default() };
        p.bands = self.dxcc_bands.iter().filter(|(d, _)| *d == dxcc).map(|(_, b)| b.clone()).collect();
        for (_, m) in self.dxcc_modes.iter().filter(|(d, _)| *d == dxcc) {
            if let Some(g) = crate::awards::mode_group(m) {
                if !p.mode_groups.contains(&g) {
                    p.mode_groups.push(g);
                }
            }
        }
        p
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    pub fn needed(&self, call: &str, dxcc: Option<u32>, band: Option<&str>, mode: Option<&str>) -> Needed {
        self.needed_in(call, dxcc, band, mode, None)
    }

    /// Like `needed`, also flagging a grid (any length of 4 or more) not worked on this band.
    pub fn needed_in(&self, call: &str, dxcc: Option<u32>, band: Option<&str>, mode: Option<&str>, grid: Option<&str>) -> Needed {
        let call = call.to_ascii_uppercase();
        let mut n = Needed {
            new_call: !self.calls.contains(&call),
            ..Needed::default()
        };
        if let Some(b) = band {
            n.new_call_band = !self.call_bands.contains(&(call, b.to_string()));
        }
        if let Some(d) = dxcc {
            n.new_dxcc = !self.dxcc.contains(&d);
            if !n.new_dxcc {
                n.new_band = band.is_some_and(|b| !self.dxcc_bands.contains(&(d, b.to_string())));
                n.new_mode = mode.is_some_and(|m| !self.dxcc_modes.contains(&(d, m.to_string())));
            }
        }
        if let (Some(b), Some(g)) = (band, grid.and_then(crate::awards::grid4)) {
            n.new_grid = is_vhf_up(b) && !self.grid_bands.contains(&(g, b.to_string()));
        }
        n
    }
}

/// 6 m and up: the bands where grids are chased.
fn is_vhf_up(band: &str) -> bool {
    let b = band.to_ascii_lowercase();
    if b.ends_with("cm") || b.ends_with("mm") {
        return true;
    }
    b.strip_suffix('m').and_then(|n| n.parse::<f64>().ok()).is_some_and(|m| m <= 6.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_what_is_new() {
        let mut w = WorkedIndex::default();
        w.add("w1aw", Some(291), Some("20m"), Some("CW"));
        let n = w.needed("W1AW", Some(291), Some("20m"), Some("CW"));
        assert_eq!(n, Needed::default());
        let n = w.needed("K1ABC", Some(291), Some("40m"), Some("FT8"));
        assert!(n.new_call && n.new_call_band && !n.new_dxcc && n.new_band && n.new_mode);
        let n = w.needed("JA1ABC", Some(339), Some("20m"), Some("CW"));
        assert!(n.new_dxcc && !n.new_band, "a new DXCC isn't also flagged as a new band");
    }

    #[test]
    fn flags_new_grids_on_vhf_only() {
        let mut w = WorkedIndex::default();
        w.add_grid("FN31pr", "2m");
        w.add_grid("FN31pr", "20m");
        let n = |g: &str, b: &str| w.needed_in("K1ABC", Some(291), Some(b), Some("FT8"), Some(g)).new_grid;
        assert!(!n("FN31", "2m"), "worked grid on this band");
        assert!(n("FN31", "70cm"), "same grid, other band");
        assert!(n("FN32", "2m"));
        assert!(n("FN32", "6m"));
        assert!(!n("FN32", "20m"), "HF grids are not flagged");
        assert!(!w.needed_in("K1ABC", None, Some("2m"), None, Some("--")).new_grid, "junk grid");
    }
}
