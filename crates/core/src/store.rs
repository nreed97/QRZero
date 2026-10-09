//! The log database (SQLite).
//!
//! Every QSO keeps all of its ADIF fields as JSON, which is the source of
//! truth, so nothing is lost on import and a "full" export round-trips. The
//! columns next to it (call, time, band, mode, ...) are indexed copies used
//! for fast searching and filtering.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};

use crate::adif::{self, Fields};
use crate::band;
use crate::error::{Error, Result};
use crate::model::*;
use crate::awards::{AwardIndex, AwardQso, CtyFacts};
use crate::worked::WorkedIndex;

/// Migrations in order; migration N brings the schema to user_version N.
const MIGRATIONS: &[&str] = &[SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6];
/// The newest schema this build knows (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i32 = MIGRATIONS.len() as i32;

/// Small partial indexes over QSOs not yet uploaded, so counting what's waiting
/// stays fast once most of a large log is uploaded. The WHERE clauses must match
/// `pending_sql` exactly for SQLite to use them.
const SCHEMA_V3: &str = r#"
CREATE INDEX qsos_pending_qrz ON qsos (log_id, station_callsign, time_on)
    WHERE IFNULL(json_extract(fields, '$.QRZCOM_QSO_UPLOAD_STATUS'), '') NOT IN ('Y', 'I');
CREATE INDEX qsos_pending_clublog ON qsos (log_id, station_callsign, time_on)
    WHERE IFNULL(json_extract(fields, '$.CLUBLOG_QSO_UPLOAD_STATUS'), '') NOT IN ('Y', 'I');
CREATE INDEX qsos_pending_lotw ON qsos (log_id, station_callsign, time_on)
    WHERE IFNULL(json_extract(fields, '$.LOTW_QSL_SENT'), '') NOT IN ('Y', 'I');
"#;

/// eQSL uploads, like [`SCHEMA_V3`].
const SCHEMA_V4: &str = r#"
CREATE INDEX qsos_pending_eqsl ON qsos (log_id, station_callsign, time_on)
    WHERE IFNULL(json_extract(fields, '$.EQSL_QSL_SENT'), '') NOT IN ('Y', 'I');
-- Bumped on every QSO change, so results computed from the whole log can be cached.
CREATE TABLE qso_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
INSERT INTO qso_version VALUES (1, 0);
CREATE TRIGGER qsos_version_ins AFTER INSERT ON qsos BEGIN UPDATE qso_version SET version = version + 1; END;
CREATE TRIGGER qsos_version_upd AFTER UPDATE ON qsos BEGIN UPDATE qso_version SET version = version + 1; END;
CREATE TRIGGER qsos_version_del AFTER DELETE ON qsos BEGIN UPDATE qso_version SET version = version + 1; END;
"#;

/// Station notes: one note per log and base call (DL1ABC/P shares DL1ABC's).
const SCHEMA_V5: &str = r#"
CREATE TABLE notes (
    log_id INTEGER NOT NULL REFERENCES logs(id) ON DELETE CASCADE,
    call TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (log_id, call)
) WITHOUT ROWID;
CREATE INDEX notes_log_updated ON notes (log_id, updated_at);
"#;

/// Cards received and still to be answered: one entry per log and call.
const SCHEMA_V6: &str = r#"
CREATE TABLE reply_list (
    log_id INTEGER NOT NULL REFERENCES logs(id) ON DELETE CASCADE,
    call TEXT NOT NULL,
    received TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    PRIMARY KEY (log_id, call)
) WITHOUT ROWID;
"#;

const SCHEMA_V1: &str = r#"
CREATE TABLE logs (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE station_callsigns (
    id INTEGER PRIMARY KEY,
    log_id INTEGER NOT NULL REFERENCES logs(id) ON DELETE CASCADE,
    callsign TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    UNIQUE (log_id, callsign)
);
CREATE TABLE locations (
    id INTEGER PRIMARY KEY,
    log_id INTEGER NOT NULL REFERENCES logs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    fields TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE qsos (
    id INTEGER PRIMARY KEY,
    log_id INTEGER NOT NULL REFERENCES logs(id) ON DELETE CASCADE,
    location_id INTEGER REFERENCES locations(id) ON DELETE SET NULL,
    call TEXT NOT NULL,
    time_on INTEGER NOT NULL,
    band TEXT,
    mode TEXT,
    submode TEXT,
    freq REAL,
    station_callsign TEXT,
    dxcc INTEGER,
    fields TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX qsos_log_time ON qsos (log_id, time_on);
CREATE INDEX qsos_log_call ON qsos (log_id, call, time_on);
CREATE INDEX qsos_log_band_mode ON qsos (log_id, band, mode, submode, time_on);
CREATE INDEX qsos_log_dxcc ON qsos (log_id, dxcc, band, mode);
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE lookup_cache (
    call TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    data TEXT NOT NULL
);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE equipment (
    id INTEGER PRIMARY KEY,
    location_id INTEGER NOT NULL REFERENCES locations(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    fields TEXT NOT NULL DEFAULT '{}',
    sort INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX equipment_location ON equipment (location_id, kind, sort);
"#;

/// Kinds of station equipment a location can hold.
pub const EQUIPMENT_KINDS: &[&str] = &["rig", "antenna", "amplifier", "rotator", "other"];

/// MY_* fields that a location stamps onto QSOs.
pub const LOCATION_FIELDS: &[&str] = &[
    "MY_GRIDSQUARE", "MY_CITY", "MY_STATE", "MY_CNTY", "MY_COUNTRY", "MY_DXCC", "MY_CQ_ZONE",
    "MY_ITU_ZONE", "MY_LAT", "MY_LON", "MY_IOTA", "MY_SOTA_REF", "MY_POTA_REF", "MY_WWFF_REF",
    "MY_SIG", "MY_SIG_INFO", "MY_RIG", "MY_ANTENNA",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Call,
}

pub struct Store {
    conn: Connection,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        let mut store = Store { conn };
        store.migrate()?;
        store.conn.execute_batch("PRAGMA optimize = 0x10002;")?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i32 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::Invalid(format!(
                "this log was created by a newer QRZero (schema {version})"
            )));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
            tx.commit()?;
        }
        Ok(())
    }

    // ---- logs ----------------------------------------------------------

    pub fn list_logs(&self) -> Result<Vec<Log>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.id, l.name, (SELECT COUNT(*) FROM qsos q WHERE q.log_id = l.id)
             FROM logs l ORDER BY l.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Log { id: r.get(0)?, name: r.get(1)?, qso_count: r.get(2)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn create_log(&self, name: &str) -> Result<Log> {
        let name = non_empty(name, "log name")?;
        self.conn
            .execute("INSERT INTO logs (name, created_at) VALUES (?1, ?2)", params![name, now()])?;
        Ok(Log { id: self.conn.last_insert_rowid(), name, qso_count: 0 })
    }

    pub fn rename_log(&self, id: i64, name: &str) -> Result<()> {
        let name = non_empty(name, "log name")?;
        self.expect_changed(
            self.conn.execute("UPDATE logs SET name = ?1 WHERE id = ?2", params![name, id])?,
            "log",
        )
    }

    pub fn delete_log(&self, id: i64) -> Result<()> {
        self.expect_changed(self.conn.execute("DELETE FROM logs WHERE id = ?1", [id])?, "log")
    }

    // ---- station callsigns ---------------------------------------------

    pub fn list_callsigns(&self, log_id: i64) -> Result<Vec<StationCallsign>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, log_id, callsign, is_default FROM station_callsigns
             WHERE log_id = ?1 ORDER BY is_default DESC, callsign",
        )?;
        let rows = stmt.query_map([log_id], |r| {
            Ok(StationCallsign {
                id: r.get(0)?,
                log_id: r.get(1)?,
                callsign: r.get(2)?,
                is_default: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Adds a callsign to a log. The first callsign becomes the default.
    pub fn add_callsign(&self, log_id: i64, callsign: &str) -> Result<StationCallsign> {
        let callsign = non_empty(callsign, "callsign")?.to_ascii_uppercase();
        let has_default: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM station_callsigns WHERE log_id = ?1 AND is_default = 1)",
            [log_id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO station_callsigns (log_id, callsign, is_default) VALUES (?1, ?2, ?3)
             ON CONFLICT (log_id, callsign) DO NOTHING",
            params![log_id, callsign, !has_default],
        )?;
        self.conn
            .query_row(
                "SELECT id, log_id, callsign, is_default FROM station_callsigns
                 WHERE log_id = ?1 AND callsign = ?2",
                params![log_id, callsign],
                |r| {
                    Ok(StationCallsign {
                        id: r.get(0)?,
                        log_id: r.get(1)?,
                        callsign: r.get(2)?,
                        is_default: r.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn set_default_callsign(&mut self, id: i64) -> Result<()> {
        self.set_default("station_callsigns", id)
    }

    pub fn delete_callsign(&self, id: i64) -> Result<()> {
        self.expect_changed(
            self.conn.execute("DELETE FROM station_callsigns WHERE id = ?1", [id])?,
            "callsign",
        )
    }

    // ---- locations -----------------------------------------------------

    pub fn list_locations(&self, log_id: i64) -> Result<Vec<Location>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, log_id, name, is_default, fields FROM locations
             WHERE log_id = ?1 ORDER BY is_default DESC, name",
        )?;
        let rows = stmt.query_map([log_id], row_to_location)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_location(&self, id: i64) -> Result<Location> {
        self.conn
            .query_row(
                "SELECT id, log_id, name, is_default, fields FROM locations WHERE id = ?1",
                [id],
                row_to_location,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("location {id}")))
    }

    /// Creates a location. The first location in a log becomes the default.
    pub fn create_location(&self, log_id: i64, name: &str, fields: &Fields) -> Result<Location> {
        let name = non_empty(name, "location name")?;
        let fields = clean_location_fields(fields);
        let has_default: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM locations WHERE log_id = ?1 AND is_default = 1)",
            [log_id],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO locations (log_id, name, is_default, fields) VALUES (?1, ?2, ?3, ?4)",
            params![log_id, name, !has_default, serde_json::to_string(&fields)?],
        )?;
        self.get_location(self.conn.last_insert_rowid())
    }

    pub fn update_location(&self, id: i64, name: &str, fields: &Fields) -> Result<Location> {
        let name = non_empty(name, "location name")?;
        let fields = clean_location_fields(fields);
        self.expect_changed(
            self.conn.execute(
                "UPDATE locations SET name = ?1, fields = ?2 WHERE id = ?3",
                params![name, serde_json::to_string(&fields)?, id],
            )?,
            "location",
        )?;
        self.get_location(id)
    }

    pub fn set_default_location(&mut self, id: i64) -> Result<()> {
        self.set_default("locations", id)
    }

    pub fn delete_location(&self, id: i64) -> Result<()> {
        self.expect_changed(self.conn.execute("DELETE FROM locations WHERE id = ?1", [id])?, "location")
    }

    fn set_default(&mut self, table: &str, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let log_id: i64 = tx
            .query_row(&format!("SELECT log_id FROM {table} WHERE id = ?1"), [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("{table} {id}")))?;
        tx.execute(&format!("UPDATE {table} SET is_default = (id = ?1) WHERE log_id = ?2"), params![id, log_id])?;
        tx.commit()?;
        Ok(())
    }

    // ---- QSOs ----------------------------------------------------------

    pub fn get_qso(&self, id: i64) -> Result<Qso> {
        self.conn
            .query_row(
                "SELECT id, log_id, location_id, fields FROM qsos WHERE id = ?1",
                [id],
                row_to_qso,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("QSO {id}")))
    }

    /// Logs a new QSO. Missing BAND is derived from FREQ, and the location's
    /// MY_* fields fill any the QSO doesn't have.
    pub fn insert_qso(&self, log_id: i64, location_id: Option<i64>, fields: &Fields) -> Result<Qso> {
        let mut fields = normalize(fields);
        if let Some(loc) = location_id {
            apply_location(&mut fields, &self.get_location(loc)?.fields, ApplyLocation::Fill);
        }
        let cols = Columns::from_fields(&fields)?;
        insert_row(&self.conn, log_id, location_id, &cols, &fields)?;
        self.get_qso(self.conn.last_insert_rowid())
    }

    pub fn update_qso(&self, id: i64, location_id: Option<i64>, fields: &Fields) -> Result<Qso> {
        let mut fields = normalize(fields);
        // An edit to a QSO already uploaded to QRZ or Club Log marks it for re-upload.
        if let Ok(old) = self.get_qso(id) {
            let content = |f: &Fields| f.iter().filter(|(k, _)| !is_qsl_field(k)).map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>();
            if content(&old.fields) != content(&fields) {
                for key in MODIFIED_STATUS {
                    if let Some(v) = fields.get_mut(key) {
                        if v == "Y" {
                            *v = "M".into();
                        }
                    }
                }
            }
        }
        let c = Columns::from_fields(&fields)?;
        self.expect_changed(
            self.conn.execute(
                "UPDATE qsos SET location_id = ?1, call = ?2, time_on = ?3, band = ?4, mode = ?5,
                 submode = ?6, freq = ?7, station_callsign = ?8, dxcc = ?9, fields = ?10,
                 updated_at = ?11 WHERE id = ?12",
                params![
                    location_id, c.call, c.time_on, c.band, c.mode, c.submode, c.freq,
                    c.station_callsign, c.dxcc, serde_json::to_string(&fields)?, now(), id
                ],
            )?,
            "QSO",
        )?;
        self.get_qso(id)
    }

    /// QSOs not yet sent to a QSL service: `status_key` (e.g. QRZCOM_QSO_UPLOAD_STATUS)
    /// isn't Y or I, logged as one of `callsigns`, from `since` (Unix seconds) on, oldest first.
    pub fn pending_uploads(&self, log_id: i64, status_key: &str, callsigns: &[String], location_id: Option<i64>, since: i64, limit: i64) -> Result<Vec<Qso>> {
        self.pending_uploads_between(log_id, status_key, callsigns, location_id, since, i64::MAX, limit)
    }

    /// Like `pending_uploads`, but only QSOs before `until` (Unix seconds, exclusive).
    #[allow(clippy::too_many_arguments)]
    pub fn pending_uploads_between(&self, log_id: i64, status_key: &str, callsigns: &[String], location_id: Option<i64>, since: i64, until: i64, limit: i64) -> Result<Vec<Qso>> {
        let (sql, args) = pending_sql("id, log_id, location_id, fields", log_id, status_key, callsigns, location_id, since, until)?;
        let mut stmt = self.conn.prepare(&format!("{sql} ORDER BY time_on LIMIT {}", limit.max(0)))?;
        let rows = stmt.query_map(params_from_iter(args), row_to_qso)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Matches confirmations downloaded from LoTW or eQSL to QSOs in `log_ids` and applies
    /// them: each item is (downloaded record, fields to set, fields to fill where blank).
    /// Candidates share the call and are within 30 minutes; `same` makes the final call.
    pub fn apply_confirmations<'a>(
        &mut self,
        log_ids: &[i64],
        items: impl IntoIterator<Item = (&'a Fields, Fields, Fields)>,
        same: impl Fn(&Fields, &Fields) -> bool,
    ) -> Result<ConfirmStats> {
        let mut stats = ConfirmStats::default();
        if log_ids.is_empty() {
            return Ok(stats);
        }
        let logs = log_ids.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
        let tx = self.conn.transaction()?;
        {
            let mut find = tx.prepare(&format!(
                "SELECT id, fields FROM qsos WHERE log_id IN ({logs}) AND call = ?1 AND time_on BETWEEN ?2 - 1800 AND ?2 + 1800"
            ))?;
            let mut put = tx.prepare("UPDATE qsos SET fields = ?1, dxcc = ?2, updated_at = ?3 WHERE id = ?4")?;
            for (rec, set, fill) in items {
                stats.received += 1;
                let Ok(key) = Columns::from_fields(&normalize(rec)) else { continue };
                let mut hit = false;
                let candidates: Vec<(i64, String)> =
                    find.query_map(params![key.call, key.time_on], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
                for (id, raw) in candidates {
                    let mut fields: Fields = serde_json::from_str(&raw)?;
                    if !same(&fields, rec) {
                        continue;
                    }
                    hit = true;
                    let before = fields.clone();
                    fields.extend(set.iter().map(|(k, v)| (k.clone(), v.clone())));
                    for (k, v) in &fill {
                        if fields.get(k).is_none_or(|x| x.trim().is_empty()) && !v.trim().is_empty() {
                            fields.insert(k.clone(), v.clone());
                        }
                    }
                    if fields == before {
                        stats.already += 1;
                        continue;
                    }
                    let c = Columns::from_fields(&fields)?;
                    put.execute(params![serde_json::to_string(&fields)?, c.dxcc, now(), id])?;
                    stats.new += 1;
                }
                if !hit {
                    let g = |k: &str| rec.get(k).map(String::as_str).unwrap_or("");
                    stats.unmatched.push(format!("{} {} {} {} {}", g("CALL"), g("QSO_DATE"), g("TIME_ON"), g("BAND"), g("MODE")).trim().to_string());
                }
            }
        }
        tx.commit()?;
        Ok(stats)
    }

    /// A number that changes whenever any QSO is added, edited or deleted.
    pub fn qso_version(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT version FROM qso_version", [], |r| r.get(0))?)
    }

    /// Calls `f` with the award facts of every QSO in the log, optionally only for
    /// some station callsigns. `resolve` supplies a DXCC entity for QSOs without one.
    pub fn for_each_award_qso(
        &self,
        log_id: i64,
        callsigns: &[String],
        resolve: impl Fn(&str) -> CtyFacts,
        mut f: impl FnMut(&AwardQso),
    ) -> Result<()> {
        let marks = vec!["?"; callsigns.len()].join(", ");
        let calls = if callsigns.is_empty() { String::new() } else { format!(" AND station_callsign IN ({marks})") };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, call, band, IFNULL(submode, mode), dxcc, fields FROM qsos WHERE log_id = ?{calls}"
        ))?;
        let mut args: Vec<Value> = vec![log_id.into()];
        args.extend(callsigns.iter().map(|c| Value::from(c.to_ascii_uppercase())));
        let mut rows = stmt.query(params_from_iter(args))?;
        // Parsing the JSON once in Rust, keeping only these keys, is several times
        // faster than one json_extract per key.
        #[derive(serde::Deserialize)]
        #[allow(non_snake_case)]
        struct Facts<'a> {
            #[serde(borrow)]
            STATE: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            CQZ: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            CONT: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            ITUZ: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            GRIDSQUARE: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            IOTA: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            CNTY: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            LOTW_QSL_RCVD: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            QSL_RCVD: Option<std::borrow::Cow<'a, str>>,
            #[serde(borrow)]
            EQSL_QSL_RCVD: Option<std::borrow::Cow<'a, str>>,
        }
        let yes = |v: &Option<std::borrow::Cow<str>>| matches!(v.as_deref(), Some("Y") | Some("V"));
        let mut q = AwardQso::default();
        while let Some(r) = rows.next()? {
            q.id = r.get(0)?;
            q.call = r.get(1)?;
            q.band = r.get(2)?;
            q.mode = r.get(3)?;
            let dxcc: Option<i64> = r.get(4)?;
            q.dxcc = dxcc.and_then(|d| u32::try_from(d).ok());
            let raw = r.get_ref(5)?.as_str().map_err(rusqlite::Error::from)?;
            let facts: Facts = serde_json::from_str(raw)?;
            q.state = facts.STATE.map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty());
            q.cq_zone = facts.CQZ.and_then(|z| z.trim().parse().ok());
            q.cont = facts.CONT.map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty());
            q.itu = facts.ITUZ.and_then(|z| z.trim().parse().ok());
            q.grid = facts.GRIDSQUARE.and_then(|g| crate::awards::grid4(&g));
            q.iota = facts.IOTA.and_then(|i| crate::awards::iota_ref(&i));
            q.cnty = facts.CNTY.and_then(|c| crate::awards::county(&c));
            if q.dxcc.is_none() || q.cont.is_none() || q.itu.is_none() {
                let r = resolve(&q.call);
                q.dxcc = q.dxcc.or(r.dxcc);
                q.cont = q.cont.take().or(r.cont);
                q.itu = q.itu.or(r.itu);
            }
            q.lotw = yes(&facts.LOTW_QSL_RCVD);
            q.paper = yes(&facts.QSL_RCVD);
            q.eqsl = yes(&facts.EQSL_QSL_RCVD);
            f(&q);
        }
        Ok(())
    }

    /// The award cells of a log for "what would this QSO add?", with the QSO
    /// version it reflects (see [`Store::qso_version`]).
    pub fn award_index(&self, log_id: i64, resolve: impl Fn(&str) -> CtyFacts) -> Result<(i64, AwardIndex)> {
        let version = self.qso_version()?;
        let mut idx = AwardIndex::default();
        self.for_each_award_qso(log_id, &[], resolve, |q| idx.add(q))?;
        Ok((version, idx))
    }

    /// QSOs waiting for a paper QSL card (QSL_SENT is R "requested" or Q "queued"),
    /// grouped by call for printing labels.
    pub fn paper_queue(&self, log_id: i64) -> Result<Vec<Qso>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, log_id, location_id, fields FROM qsos WHERE log_id = ?1
             AND json_extract(fields, '$.QSL_SENT') IN ('R', 'Q') ORDER BY call, time_on",
        )?;
        let rows = stmt.query_map([log_id], row_to_qso)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn count_pending(&self, log_id: i64, status_key: &str, callsigns: &[String], location_id: Option<i64>, since: i64) -> Result<i64> {
        self.count_pending_between(log_id, status_key, callsigns, location_id, since, i64::MAX)
    }

    /// Like `count_pending`, but only QSOs before `until` (Unix seconds, exclusive).
    pub fn count_pending_between(&self, log_id: i64, status_key: &str, callsigns: &[String], location_id: Option<i64>, since: i64, until: i64) -> Result<i64> {
        let (sql, args) = pending_sql("COUNT(*)", log_id, status_key, callsigns, location_id, since, until)?;
        Ok(self.conn.query_row(&sql, params_from_iter(args), |r| r.get(0))?)
    }

    /// Sets fields (QSL statuses and dates) on several QSOs at once.
    pub fn mark_qsos(&mut self, ids: &[i64], set: &Fields) -> Result<()> {
        if set.keys().any(|k| !is_qsl_field(k)) {
            return Err(Error::Invalid("only QSL fields can be set in bulk".into()));
        }
        let tx = self.conn.transaction()?;
        {
            let mut get = tx.prepare("SELECT fields FROM qsos WHERE id = ?1")?;
            let mut put = tx.prepare("UPDATE qsos SET fields = ?1, updated_at = ?2 WHERE id = ?3")?;
            for id in ids {
                let Some(raw) = get.query_row([id], |r| r.get::<_, String>(0)).optional()? else { continue };
                let mut fields: Fields = serde_json::from_str(&raw)?;
                fields.extend(set.iter().map(|(k, v)| (k.clone(), v.clone())));
                put.execute(params![serde_json::to_string(&fields)?, now(), id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_qsos(&mut self, ids: &[i64]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut stmt = tx.prepare("DELETE FROM qsos WHERE id = ?1")?;
            for id in ids {
                n += stmt.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// Returns (total matching, one page of QSOs).
    pub fn search(
        &self,
        log_id: i64,
        filter: &QsoFilter,
        sort: Sort,
        offset: i64,
        limit: i64,
    ) -> Result<(i64, Vec<Qso>)> {
        let (where_sql, mut args) = build_where(log_id, filter);
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM qsos WHERE {where_sql}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        // With an indexed filter, sorting the (smaller) match set is faster than
        // walking the whole time index; the unary + stops SQLite using that index.
        let indexed = filter.call.is_some() || filter.exact_call.is_some() || !filter.bands.is_empty() || !filter.modes.is_empty()
            || filter.dxcc.is_some() || filter.ids.is_some();
        let t = if indexed { "+time_on" } else { "time_on" };
        let order = match sort {
            Sort::Newest => format!("{t} DESC, id DESC"),
            Sort::Oldest => format!("{t} ASC, id ASC"),
            Sort::Call => "call ASC, time_on DESC".to_string(),
        };
        args.push(Value::Integer(limit.clamp(0, 10_000)));
        args.push(Value::Integer(offset.max(0)));
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, log_id, location_id, fields FROM qsos WHERE {where_sql}
             ORDER BY {order} LIMIT ? OFFSET ?"
        ))?;
        let rows = stmt.query_map(params_from_iter(args.iter()), row_to_qso)?;
        Ok((total, rows.collect::<std::result::Result<_, _>>()?))
    }

    pub fn worked_before(&self, log_id: i64, call: &str, dxcc: Option<i64>) -> Result<WorkedBefore> {
        let call = call.trim().to_ascii_uppercase();
        let mut wb = WorkedBefore { dxcc, ..Default::default() };
        wb.call_count = self.conn.query_row(
            "SELECT COUNT(*) FROM qsos WHERE log_id = ?1 AND call = ?2",
            params![log_id, call],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT id, log_id, location_id, fields FROM qsos WHERE log_id = ?1 AND call = ?2
             ORDER BY time_on DESC LIMIT 20",
        )?;
        wb.recent = stmt
            .query_map(params![log_id, call], row_to_qso)?
            .collect::<std::result::Result<_, _>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT IFNULL(band, ''), IFNULL(submode, IFNULL(mode, '')) FROM qsos
             WHERE log_id = ?1 AND call = ?2 ORDER BY 1, 2",
        )?;
        wb.call_slots = stmt
            .query_map(params![log_id, call], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        if let Some(d) = dxcc {
            wb.dxcc_count = self.conn.query_row(
                "SELECT COUNT(*) FROM qsos WHERE log_id = ?1 AND dxcc = ?2",
                params![log_id, d],
                |r| r.get(0),
            )?;
            let distinct = |col: &str| -> Result<Vec<String>> {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT DISTINCT {col} FROM qsos WHERE log_id = ?1 AND dxcc = ?2
                     AND {col} IS NOT NULL ORDER BY 1"
                ))?;
                let rows = stmt.query_map(params![log_id, d], |r| r.get(0))?;
                Ok(rows.collect::<std::result::Result<_, _>>()?)
            };
            wb.dxcc_bands = distinct("band")?;
            wb.dxcc_modes = distinct("IFNULL(submode, mode)")?;
        }
        Ok(wb)
    }

    /// Builds the worked-before sets for flagging decodes and spots. `resolve`
    /// supplies the DXCC entity for QSOs logged without one.
    pub fn worked_index(&self, log_id: i64, resolve: impl Fn(&str) -> Option<u32>) -> Result<WorkedIndex> {
        let mut idx = WorkedIndex::default();
        let mut stmt = self
            .conn
            .prepare("SELECT call, dxcc, band, IFNULL(submode, mode), json_extract(fields, '$.GRIDSQUARE') FROM qsos WHERE log_id = ?1")?;
        let mut rows = stmt.query([log_id])?;
        while let Some(r) = rows.next()? {
            let call: String = r.get(0)?;
            let dxcc: Option<i64> = r.get(1)?;
            let band: Option<String> = r.get(2)?;
            let mode: Option<String> = r.get(3)?;
            let dxcc = dxcc.and_then(|d| u32::try_from(d).ok()).or_else(|| resolve(&call));
            idx.add(&call, dxcc, band.as_deref(), mode.as_deref());
            let grid: Option<String> = r.get(4)?;
            if let (Some(g), Some(b)) = (grid, band.as_deref()) {
                idx.add_grid(&g, b);
            }
        }
        Ok(idx)
    }

    /// A QSO already in the log with the same call, band and mode within a minute of this one.
    pub fn find_duplicate(&self, log_id: i64, fields: &Fields) -> Result<Option<i64>> {
        let cols = Columns::from_fields(&normalize(fields))?;
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM qsos WHERE log_id = ?1 AND call = ?2
                 AND time_on BETWEEN ?3 - 60 AND ?3 + 60
                 AND IFNULL(band, '') = IFNULL(?4, '') AND IFNULL(mode, '') = IFNULL(?5, '') LIMIT 1",
                params![log_id, cols.call, cols.time_on, cols.band, cols.mode],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Finds a QSO by the value of one of its fields (e.g. another program's record id).
    pub fn find_qso_by_field(&self, log_id: i64, key: &str, value: &str) -> Result<Option<i64>> {
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::Invalid(format!("bad field name {key}")));
        }
        Ok(self
            .conn
            .query_row(
                &format!("SELECT id FROM qsos WHERE log_id = ?1 AND json_extract(fields, '$.{key}') = ?2 ORDER BY id DESC LIMIT 1"),
                params![log_id, value],
                |r| r.get(0),
            )
            .optional()?)
    }

    // ---- ADIF ----------------------------------------------------------

    pub fn import_adif(&mut self, log_id: i64, data: &[u8], opts: &ImportOptions) -> Result<ImportReport> {
        let file = adif::parse(data);
        let mut report = ImportReport::default();
        report.messages.extend(file.warnings.iter().take(5).cloned());
        let location = match opts.location_id {
            Some(id) => Some(self.get_location(id)?),
            None => None,
        };
        let mut known: BTreeSet<String> =
            self.list_callsigns(log_id)?.into_iter().map(|c| c.callsign).collect();

        let tx = self.conn.transaction()?;
        {
            let mut dupe = tx.prepare(
                "SELECT EXISTS(SELECT 1 FROM qsos WHERE log_id = ?1 AND call = ?2
                 AND time_on BETWEEN ?3 - 60 AND ?3 + 60
                 AND IFNULL(band, '') = IFNULL(?4, '') AND IFNULL(mode, '') = IFNULL(?5, ''))",
            )?;
            for (i, record) in file.records.iter().enumerate() {
                let mut fields = normalize(record);
                if let Some(loc) = &location {
                    apply_location(&mut fields, &loc.fields, opts.apply_location);
                }
                let cols = match Columns::from_fields(&fields) {
                    Ok(c) => c,
                    Err(e) => {
                        report.rejected += 1;
                        if report.messages.len() < 20 {
                            report.messages.push(format!("record {}: {e}", i + 1));
                        }
                        continue;
                    }
                };
                if opts.skip_duplicates {
                    let exists: bool = dupe.query_row(
                        params![log_id, cols.call, cols.time_on, cols.band, cols.mode],
                        |r| r.get(0),
                    )?;
                    if exists {
                        report.duplicates += 1;
                        continue;
                    }
                }
                if opts.add_station_callsigns {
                    if let Some(sc) = &cols.station_callsign {
                        if known.insert(sc.clone()) {
                            report.added_callsigns.push(sc.clone());
                        }
                    }
                }
                insert_row(&tx, log_id, opts.location_id, &cols, &fields)?;
                report.imported += 1;
            }
        }
        for (i, call) in report.added_callsigns.iter().enumerate() {
            let has_default: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM station_callsigns WHERE log_id = ?1 AND is_default = 1)",
                [log_id],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO station_callsigns (log_id, callsign, is_default) VALUES (?1, ?2, ?3)",
                params![log_id, call, i == 0 && !has_default],
            )?;
        }
        tx.commit()?;
        // Refresh the query planner's statistics after a bulk load.
        self.conn.execute_batch("PRAGMA optimize = 0x10002;")?;
        Ok(report)
    }

    /// Returns the ADIF text and the number of QSOs in it.
    pub fn export_adif(
        &self,
        log_id: i64,
        filter: &QsoFilter,
        profile: ExportProfile,
        program_version: &str,
    ) -> Result<(String, usize)> {
        let (where_sql, args) = build_where(log_id, filter);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT fields FROM qsos WHERE {where_sql} ORDER BY time_on ASC, id ASC"
        ))?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        let mut body = String::new();
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let fields: Fields = serde_json::from_str(&json)?;
            match profile {
                ExportProfile::Standard => adif::write_record(&mut body, &fields, adif::is_standard_field),
                ExportProfile::Full => adif::write_record(&mut body, &fields, |_| true),
            }
            count += 1;
        }
        let mut out = String::with_capacity(body.len() + 256);
        adif::write_header(&mut out, program_version, count);
        out.push_str(&body);
        Ok((out, count))
    }

    // ---- equipment -----------------------------------------------------

    /// All equipment at all locations of a log, grouped by location then kind.
    pub fn list_equipment(&self, log_id: i64) -> Result<Vec<Equipment>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.location_id, e.kind, e.name, e.fields, e.sort FROM equipment e
             JOIN locations l ON l.id = e.location_id WHERE l.log_id = ?1
             ORDER BY e.location_id, e.kind, e.sort, e.id",
        )?;
        let rows = stmt.query_map([log_id], row_to_equipment)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_equipment(&self, id: i64) -> Result<Equipment> {
        self.conn
            .query_row(
                "SELECT id, location_id, kind, name, fields, sort FROM equipment WHERE id = ?1",
                [id],
                row_to_equipment,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("equipment {id}")))
    }

    pub fn create_equipment(&self, location_id: i64, kind: &str, name: &str, fields: &Fields) -> Result<Equipment> {
        let (kind, name) = check_equipment(kind, name)?;
        self.get_location(location_id)?;
        let sort: i64 = self.conn.query_row(
            "SELECT IFNULL(MAX(sort), -1) + 1 FROM equipment WHERE location_id = ?1 AND kind = ?2",
            params![location_id, kind],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO equipment (location_id, kind, name, fields, sort) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![location_id, kind, name, serde_json::to_string(&clean_fields(fields))?, sort],
        )?;
        self.get_equipment(self.conn.last_insert_rowid())
    }

    /// Updates an item. Moving it to another location is allowed.
    pub fn update_equipment(&self, id: i64, location_id: i64, kind: &str, name: &str, fields: &Fields) -> Result<Equipment> {
        let (kind, name) = check_equipment(kind, name)?;
        self.get_location(location_id)?;
        self.expect_changed(
            self.conn.execute(
                "UPDATE equipment SET location_id = ?1, kind = ?2, name = ?3, fields = ?4 WHERE id = ?5",
                params![location_id, kind, name, serde_json::to_string(&clean_fields(fields))?, id],
            )?,
            "equipment",
        )?;
        self.get_equipment(id)
    }

    /// Moves an item up (-1) or down (+1) within its location and kind.
    pub fn move_equipment(&mut self, id: i64, delta: i64) -> Result<()> {
        let item = self.get_equipment(id)?;
        let tx = self.conn.transaction()?;
        let ids: Vec<i64> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM equipment WHERE location_id = ?1 AND kind = ?2 ORDER BY sort, id",
            )?;
            let rows = stmt.query_map(params![item.location_id, item.kind], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let mut ids = ids;
        if let Some(pos) = ids.iter().position(|x| *x == id) {
            let to = (pos as i64 + delta).clamp(0, ids.len() as i64 - 1) as usize;
            let v = ids.remove(pos);
            ids.insert(to, v);
        }
        for (i, eid) in ids.iter().enumerate() {
            tx.execute("UPDATE equipment SET sort = ?1 WHERE id = ?2", params![i as i64, eid])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_equipment(&self, id: i64) -> Result<()> {
        self.expect_changed(self.conn.execute("DELETE FROM equipment WHERE id = ?1", [id])?, "equipment")
    }

    // ---- cards to reply to ---------------------------------------------

    /// The reply list, oldest card first.
    pub fn list_replies(&self, log_id: i64) -> Result<Vec<ReplyEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT call, received, note, created_at FROM reply_list WHERE log_id = ?1 ORDER BY received, created_at, call",
        )?;
        let rows = stmt.query_map([log_id], |r| Ok(ReplyEntry { call: r.get(0)?, received: r.get(1)?, note: r.get(2)?, created_at: r.get(3)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Adds or updates a call on the reply list. A blank `received` means today.
    pub fn save_reply(&self, log_id: i64, call: &str, received: &str, note: &str) -> Result<()> {
        let call = call.trim().to_ascii_uppercase();
        if call.is_empty() {
            return Err(Error::Invalid("a reply entry needs a callsign".into()));
        }
        let received = received.trim();
        let received = if received.is_empty() { chrono::Utc::now().format("%Y-%m-%d").to_string() } else { received.to_string() };
        self.conn.execute(
            "INSERT INTO reply_list (log_id, call, received, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (log_id, call) DO UPDATE SET received = ?3, note = ?4",
            params![log_id, call, received, note, chrono::Utc::now().timestamp()],
        )?;
        Ok(())
    }

    /// Adds a call only when it isn't listed yet, so "Add to reply list" never overwrites a note.
    pub fn add_reply_if_new(&self, log_id: i64, call: &str) -> Result<()> {
        let call = call.trim().to_ascii_uppercase();
        let listed: bool = self
            .conn
            .query_row("SELECT 1 FROM reply_list WHERE log_id = ?1 AND call = ?2", params![log_id, call], |_| Ok(()))
            .optional()?
            .is_some();
        if listed { Ok(()) } else { self.save_reply(log_id, &call, "", "") }
    }

    /// Removes a call from the reply list (replied); false when it wasn't there.
    pub fn delete_reply(&self, log_id: i64, call: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM reply_list WHERE log_id = ?1 AND call = ?2", params![log_id, call.trim().to_ascii_uppercase()])? > 0)
    }

    // ---- station notes -------------------------------------------------

    /// The note for a call (its base call: DL1ABC/P reads DL1ABC's note).
    pub fn get_note(&self, log_id: i64, call: &str) -> Result<Option<Note>> {
        let call = note_call(call)?;
        Ok(self
            .conn
            .query_row(
                "SELECT call, text, created_at, updated_at FROM notes WHERE log_id = ?1 AND call = ?2",
                params![log_id, call],
                row_to_note,
            )
            .optional()?)
    }

    /// Saves the note for a call; blank text deletes it (and gives None).
    pub fn set_note(&self, log_id: i64, call: &str, text: &str) -> Result<Option<Note>> {
        let call = note_call(call)?;
        if text.trim().is_empty() {
            self.conn.execute("DELETE FROM notes WHERE log_id = ?1 AND call = ?2", params![log_id, call])?;
            return Ok(None);
        }
        let text = text.replace("\r\n", "\n");
        let t = now();
        Ok(Some(self.conn.query_row(
            "INSERT INTO notes (log_id, call, text, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT (log_id, call) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at
             RETURNING call, text, created_at, updated_at",
            params![log_id, call, text, t],
            row_to_note,
        )?))
    }

    /// Deletes a call's note; false when there was none.
    pub fn delete_note(&self, log_id: i64, call: &str) -> Result<bool> {
        let call = note_call(call)?;
        Ok(self.conn.execute("DELETE FROM notes WHERE log_id = ?1 AND call = ?2", params![log_id, call])? > 0)
    }

    pub fn has_note(&self, log_id: i64, call: &str) -> Result<bool> {
        let call = note_call(call)?;
        Ok(self
            .conn
            .query_row("SELECT 1 FROM notes WHERE log_id = ?1 AND call = ?2", params![log_id, call], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// Notes whose call contains `query` (all when empty), newest first, with the total.
    pub fn list_notes(&self, log_id: i64, query: &str, offset: i64, limit: i64) -> Result<(i64, Vec<Note>)> {
        let q = query.trim().to_ascii_uppercase();
        let filter = "log_id = ?1 AND (?2 = '' OR instr(call, ?2) > 0)";
        let total = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM notes WHERE {filter}"),
            params![log_id, q],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT call, text, created_at, updated_at FROM notes WHERE {filter}
             ORDER BY updated_at DESC, call LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = stmt.query_map(params![log_id, q, limit.clamp(0, 10_000), offset.max(0)], row_to_note)?;
        Ok((total, rows.collect::<std::result::Result<_, _>>()?))
    }

    // ---- settings and lookup cache -------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }

    /// Returns cached lookup data no older than `max_age` seconds.
    pub fn cached_lookup(&self, call: &str, max_age: i64) -> Result<Option<Fields>> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM lookup_cache WHERE call = ?1 AND fetched_at >= ?2",
                params![call.to_ascii_uppercase(), now() - max_age],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match row {
            Some(json) => Some(serde_json::from_str(&json)?),
            None => None,
        })
    }

    pub fn cache_lookup(&self, call: &str, source: &str, data: &Fields) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lookup_cache (call, source, fetched_at, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (call) DO UPDATE SET source = excluded.source,
             fetched_at = excluded.fetched_at, data = excluded.data",
            params![call.to_ascii_uppercase(), source, now(), serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    fn expect_changed(&self, n: usize, what: &str) -> Result<()> {
        if n == 0 {
            Err(Error::NotFound(what.to_string()))
        } else {
            Ok(())
        }
    }
}

/// Upload statuses ADIF says become "M" when an uploaded QSO is modified.
const MODIFIED_STATUS: [&str; 2] = ["QRZCOM_QSO_UPLOAD_STATUS", "CLUBLOG_QSO_UPLOAD_STATUS"];

/// QSL and upload bookkeeping, as opposed to what happened on the air.
fn is_qsl_field(k: &str) -> bool {
    k.contains("QSL") || k.contains("UPLOAD") || k.contains("DOWNLOAD") || k.contains("OQRS") || k.starts_with("EQSL_") || k.starts_with("LOTW_")
}

fn pending_sql(select: &str, log_id: i64, status_key: &str, callsigns: &[String], location_id: Option<i64>, since: i64, until: i64) -> Result<(String, Vec<Value>)> {
    if !status_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::Invalid(format!("bad field name {status_key}")));
    }
    let mut args: Vec<Value> = vec![log_id.into(), since.into(), until.into()];
    let marks = vec!["?"; callsigns.len()].join(", ");
    args.extend(callsigns.iter().map(|c| Value::from(c.to_ascii_uppercase())));
    let mut sql = format!(
        "SELECT {select} FROM qsos WHERE log_id = ? AND time_on >= ? AND time_on < ? AND station_callsign IN ({marks})
         AND IFNULL(json_extract(fields, '$.{status_key}'), '') NOT IN ('Y', 'I')"
    );
    if let Some(loc) = location_id {
        sql.push_str(" AND location_id = ?");
        args.push(loc.into());
    }
    Ok((sql, args))
}

/// What applying downloaded confirmations did.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ConfirmStats {
    /// Confirmations downloaded.
    pub received: usize,
    /// QSOs newly marked confirmed (or given new details).
    pub new: usize,
    /// QSOs that already showed the confirmation.
    pub already: usize,
    /// Downloaded confirmations with no matching QSO in the log.
    pub unmatched: Vec<String>,
}

/// Indexed copies of a QSO's key fields.
struct Columns {
    call: String,
    time_on: i64,
    band: Option<String>,
    mode: Option<String>,
    submode: Option<String>,
    freq: Option<f64>,
    station_callsign: Option<String>,
    dxcc: Option<i64>,
}

impl Columns {
    fn from_fields(f: &Fields) -> Result<Self> {
        let call = f
            .get("CALL")
            .cloned()
            .ok_or_else(|| Error::Invalid("missing CALL".into()))?;
        let time_on = parse_time(
            f.get("QSO_DATE").map(String::as_str).unwrap_or(""),
            f.get("TIME_ON").map(String::as_str).unwrap_or(""),
        )
        .ok_or_else(|| Error::Invalid(format!("{call}: missing or bad QSO_DATE/TIME_ON")))?;
        Ok(Columns {
            call,
            time_on,
            band: f.get("BAND").cloned(),
            mode: f.get("MODE").cloned(),
            submode: f.get("SUBMODE").cloned(),
            freq: f.get("FREQ").and_then(|v| v.parse().ok()),
            station_callsign: f.get("STATION_CALLSIGN").cloned(),
            dxcc: f.get("DXCC").and_then(|v| v.parse().ok()),
        })
    }
}

fn insert_row(conn: &Connection, log_id: i64, location_id: Option<i64>, c: &Columns, fields: &Fields) -> Result<()> {
    let t = now();
    conn.prepare_cached(
        "INSERT INTO qsos (log_id, location_id, call, time_on, band, mode, submode, freq,
         station_callsign, dxcc, fields, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
    )?
    .execute(params![
        log_id, location_id, c.call, c.time_on, c.band, c.mode, c.submode, c.freq,
        c.station_callsign, c.dxcc, serde_json::to_string(fields)?, t
    ])?;
    Ok(())
}

/// Parses ADIF QSO_DATE (YYYYMMDD) and TIME_ON (HHMM or HHMMSS) into Unix seconds (UTC).
pub fn parse_time(date: &str, time: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(date.trim(), "%Y%m%d").ok()?;
    let time = time.trim();
    let t = match time.len() {
        4 => NaiveTime::parse_from_str(time, "%H%M").ok()?,
        6 => NaiveTime::parse_from_str(time, "%H%M%S").ok()?,
        _ => return None,
    };
    Some(NaiveDateTime::new(d, t).and_utc().timestamp())
}

/// Cleans up fields: upper-case names, trimmed values, no empties, canonical
/// call/mode/band, and BAND derived from FREQ when missing.
pub fn normalize(input: &Fields) -> Fields {
    let mut f = Fields::new();
    for (k, v) in input {
        let v = v.trim();
        if !v.is_empty() {
            f.insert(k.trim().to_ascii_uppercase(), v.to_string());
        }
    }
    for key in ["CALL", "STATION_CALLSIGN", "OPERATOR", "OWNER_CALLSIGN", "MODE", "SUBMODE", "CONT"] {
        if let Some(v) = f.get_mut(key) {
            *v = v.to_ascii_uppercase();
        }
    }
    for key in ["GRIDSQUARE", "MY_GRIDSQUARE"] {
        if let Some(v) = f.get_mut(key) {
            *v = format_grid(v);
        }
    }
    for (band_key, freq_key) in [("BAND", "FREQ"), ("BAND_RX", "FREQ_RX")] {
        let known = f.get(band_key).and_then(|b| band::normalize_band(b));
        let derived = f
            .get(freq_key)
            .and_then(|v| v.parse::<f64>().ok())
            .and_then(band::band_for_freq);
        match known.or(derived) {
            Some(b) => {
                f.insert(band_key.to_string(), b.to_string());
            }
            None => {
                if let Some(b) = f.get_mut(band_key) {
                    *b = b.to_ascii_lowercase();
                }
            }
        }
    }
    f
}

/// Maidenhead grids are written "FN31pr": field upper-case, subsquare lower-case.
fn format_grid(g: &str) -> String {
    g.chars()
        .enumerate()
        .map(|(i, c)| if (4..6).contains(&(i % 8)) { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() })
        .collect()
}

pub fn apply_location(fields: &mut Fields, loc: &Fields, mode: ApplyLocation) {
    match mode {
        ApplyLocation::LinkOnly => {}
        ApplyLocation::Fill => {
            for (k, v) in loc {
                fields.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        ApplyLocation::Overwrite => {
            for k in LOCATION_FIELDS {
                fields.remove(*k);
            }
            fields.extend(loc.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }
}

fn clean_location_fields(fields: &Fields) -> Fields {
    normalize(fields)
        .into_iter()
        .filter(|(k, _)| LOCATION_FIELDS.contains(&k.as_str()))
        .collect()
}

fn build_where(log_id: i64, f: &QsoFilter) -> (String, Vec<Value>) {
    let mut sql = vec!["log_id = ?".to_string()];
    let mut args = vec![Value::Integer(log_id)];
    let in_list = |sql: &mut Vec<String>, args: &mut Vec<Value>, expr: &str, vals: Vec<Value>| {
        if vals.is_empty() {
            return;
        }
        let marks = vec!["?"; vals.len()].join(",");
        sql.push(format!("{expr} IN ({marks})"));
        args.extend(vals);
    };
    if let Some(call) = f.call.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        let call = call.to_ascii_uppercase();
        if call.contains('*') {
            let pattern = call.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_").replace('*', "%");
            sql.push("call LIKE ? ESCAPE '\\'".into());
            args.push(Value::Text(pattern));
        } else {
            // Prefix range keeps the call index usable.
            let mut upper = call.clone();
            upper.push(char::MAX);
            sql.push("call >= ? AND call < ?".into());
            args.push(Value::Text(call));
            args.push(Value::Text(upper));
        }
    }
    if let Some(call) = f.exact_call.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        let call = call.to_ascii_uppercase();
        let base = base_call(&call);
        if base.is_empty() {
            sql.push("call = ?".into());
            args.push(Value::Text(call));
        } else {
            // [BASE, BASE + "0") holds BASE itself and BASE/anything ('/' sorts just
            // below '0'): one range on the (log_id, call) index.
            args.push(Value::Text(base.to_string()));
            args.push(Value::Text(format!("{base}0")));
            if call == base || call.starts_with(&format!("{base}/")) {
                sql.push("call >= ? AND call < ?".into());
            } else {
                // A prefixed form such as EA8/DL1ABC sorts elsewhere: add it by name.
                sql.push("((call >= ? AND call < ?) OR call = ?)".into());
                args.push(Value::Text(call));
            }
        }
    }
    in_list(
        &mut sql,
        &mut args,
        "band",
        f.bands.iter().map(|b| Value::Text(b.to_ascii_lowercase())).collect(),
    );
    if !f.modes.is_empty() {
        let modes: Vec<Value> = f.modes.iter().map(|m| Value::Text(m.to_ascii_uppercase())).collect();
        let marks = vec!["?"; modes.len()].join(",");
        sql.push(format!("(mode IN ({marks}) OR submode IN ({marks}))"));
        args.extend(modes.clone());
        args.extend(modes);
    }
    if let Some(from) = f.from {
        sql.push("time_on >= ?".into());
        args.push(Value::Integer(from));
    }
    if let Some(to) = f.to {
        sql.push("time_on <= ?".into());
        args.push(Value::Integer(to));
    }
    in_list(
        &mut sql,
        &mut args,
        "station_callsign",
        f.station_callsigns.iter().map(|c| Value::Text(c.to_ascii_uppercase())).collect(),
    );
    in_list(&mut sql, &mut args, "location_id", f.location_ids.iter().map(|i| Value::Integer(*i)).collect());
    if let Some(d) = f.dxcc {
        sql.push("dxcc = ?".into());
        args.push(Value::Integer(d));
    }
    for (k, v) in &f.fields {
        // Field names go into a JSON path, so only allow ADIF-style names.
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let path = format!("$.\"{}\"", k.to_ascii_uppercase());
        if v.is_empty() {
            sql.push("IFNULL(json_extract(fields, ?), '') = ''".into());
            args.push(Value::Text(path));
        } else {
            sql.push("json_extract(fields, ?) = ?".into());
            args.push(Value::Text(path));
            args.push(Value::Text(v.clone()));
        }
    }
    if let Some(ids) = &f.ids {
        if ids.is_empty() {
            sql.push("0".into());
        } else {
            // Many ids: use a JSON array instead of thousands of placeholders.
            sql.push("id IN (SELECT value FROM json_each(?))".into());
            args.push(Value::Text(serde_json::to_string(ids).unwrap_or_default()));
        }
    }
    (sql.join(" AND "), args)
}

/// The home call inside a portable form: the longest part between slashes
/// ("EA8/DL1ABC/P" gives "DL1ABC"); the first part wins a tie.
pub fn base_call(call: &str) -> &str {
    call.split('/').fold("", |best, p| if p.len() > best.len() { p } else { best })
}

/// The key a note is stored under: the upper-case base call.
fn note_call(call: &str) -> Result<String> {
    let c = call.trim().to_ascii_uppercase();
    let base = base_call(&c);
    if base.is_empty() {
        return Err(Error::Invalid("a note needs a callsign".into()));
    }
    Ok(base.to_string())
}

fn row_to_note(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note { call: r.get(0)?, text: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)? })
}

fn row_to_qso(r: &rusqlite::Row) -> rusqlite::Result<Qso> {
    let json: String = r.get(3)?;
    Ok(Qso {
        id: r.get(0)?,
        log_id: r.get(1)?,
        location_id: r.get(2)?,
        fields: serde_json::from_str(&json).unwrap_or_default(),
    })
}

fn row_to_equipment(r: &rusqlite::Row) -> rusqlite::Result<Equipment> {
    let json: String = r.get(4)?;
    Ok(Equipment {
        id: r.get(0)?,
        location_id: r.get(1)?,
        kind: r.get(2)?,
        name: r.get(3)?,
        fields: serde_json::from_str(&json).unwrap_or_default(),
        sort: r.get(5)?,
    })
}

fn check_equipment(kind: &str, name: &str) -> Result<(String, String)> {
    let kind = kind.trim().to_ascii_lowercase();
    if !EQUIPMENT_KINDS.contains(&kind.as_str()) {
        return Err(Error::Invalid(format!("unknown equipment kind {kind}")));
    }
    Ok((kind, non_empty(name, "name")?))
}

/// Trims values and drops empty ones; keys are kept as given.
fn clean_fields(fields: &Fields) -> Fields {
    fields
        .iter()
        .filter_map(|(k, v)| {
            let (k, v) = (k.trim(), v.trim());
            (!k.is_empty() && !v.is_empty()).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn row_to_location(r: &rusqlite::Row) -> rusqlite::Result<Location> {
    let json: String = r.get(4)?;
    Ok(Location {
        id: r.get(0)?,
        log_id: r.get(1)?,
        name: r.get(2)?,
        is_default: r.get(3)?,
        fields: serde_json::from_str(&json).unwrap_or_default(),
    })
}

fn non_empty(s: &str, what: &str) -> Result<String> {
    let s = s.trim();
    if s.is_empty() {
        Err(Error::Invalid(format!("{what} is required")))
    } else {
        Ok(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_a_v1_database_in_place() {
        let dir = std::env::temp_dir().join(format!("qrzero-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1.db");
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch("PRAGMA user_version = 1;
                INSERT INTO logs (id, name, created_at) VALUES (1, 'Old', 0);
                INSERT INTO locations (id, log_id, name) VALUES (1, 1, 'Home');
                INSERT INTO qsos (log_id, call, time_on, fields, created_at, updated_at)
                VALUES (1, 'W1AW', 0, '{\"CALL\":\"W1AW\"}', 0, 0);").unwrap();
        }
        let st = Store::open(&path).unwrap();
        let v: i32 = st.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        assert_eq!(st.list_logs().unwrap()[0].qso_count, 1);
        st.create_equipment(1, "rig", "K3", &Fields::new()).unwrap();
        drop(st);
        std::fs::remove_dir_all(&dir).ok();
    }
}
