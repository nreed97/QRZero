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
}

/// What is new about a station, relative to a log.
#[derive(Debug, Default, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Needed {
    pub new_call: bool,
    pub new_call_band: bool,
    pub new_dxcc: bool,
    pub new_band: bool,
    pub new_mode: bool,
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

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    pub fn needed(&self, call: &str, dxcc: Option<u32>, band: Option<&str>, mode: Option<&str>) -> Needed {
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
        n
    }
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
}
