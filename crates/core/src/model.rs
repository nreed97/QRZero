use serde::{Deserialize, Serialize};

use crate::adif::Fields;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub id: i64,
    pub name: String,
    pub qso_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationCallsign {
    pub id: i64,
    pub log_id: i64,
    pub callsign: String,
    pub is_default: bool,
}

/// An operating location. `fields` holds the MY_* ADIF fields (MY_GRIDSQUARE,
/// MY_POTA_REF, ...) that get stamped onto QSOs logged from it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: i64,
    pub log_id: i64,
    pub name: String,
    pub is_default: bool,
    pub fields: Fields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qso {
    pub id: i64,
    pub log_id: i64,
    pub location_id: Option<i64>,
    pub fields: Fields,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QsoFilter {
    /// Callsign search. Plain text matches the start of the call; `*` is a wildcard.
    pub call: Option<String>,
    pub bands: Vec<String>,
    /// Matches either MODE or SUBMODE, so "FT4" and "MFSK" both work.
    pub modes: Vec<String>,
    /// Unix seconds, inclusive.
    pub from: Option<i64>,
    /// Unix seconds, inclusive.
    pub to: Option<i64>,
    pub station_callsigns: Vec<String>,
    pub location_ids: Vec<i64>,
    pub dxcc: Option<i64>,
    /// Exact ADIF field values, e.g. {"LOTW_QSL_SENT": "N"}. An empty value
    /// matches a missing or empty field.
    pub fields: Fields,
    /// Restrict to these QSO ids (the user's selection).
    pub ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyLocation {
    /// Fill MY_* fields only where the record has none.
    #[default]
    Fill,
    /// Replace the record's MY_* fields with the location's.
    Overwrite,
    /// Link the location but leave the record's fields alone.
    LinkOnly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportOptions {
    pub location_id: Option<i64>,
    pub apply_location: ApplyLocation,
    /// Skip records that match an existing QSO (same call, band, mode, within a minute).
    pub skip_duplicates: bool,
    /// Add STATION_CALLSIGN values that the log doesn't know yet to its callsign list.
    pub add_station_callsigns: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub added_callsigns: Vec<String>,
    /// First few problems, for display.
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportProfile {
    #[default]
    Standard,
    Full,
}

/// What the log already holds for a callsign, shown while entering a QSO.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkedBefore {
    pub call_count: i64,
    pub recent: Vec<Qso>,
    /// Distinct band/mode pairs worked with this call.
    pub call_slots: Vec<(String, String)>,
    /// For the call's DXCC entity, when known. Modes are SUBMODE where set (FT4), else MODE.
    pub dxcc: Option<i64>,
    pub dxcc_count: i64,
    pub dxcc_bands: Vec<String>,
    pub dxcc_modes: Vec<String>,
}
