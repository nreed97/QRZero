//! Backups of the log database and restoring from them.
//!
//! Backups are made with `VACUUM INTO`, which gives a consistent, compact copy
//! even while QRZero is writing to the log. They go in the `backups` folder of
//! the data folder, named by local date and time:
//! `qrzero-2026-10-08-153012-auto.db`.
//!
//! A restore is staged rather than done in place, because much of what QRZero
//! keeps in memory is built from the log: the file is checked and copied to
//! `restore-pending.db`, and the next start ([`apply_pending`]) first backs up
//! the current log, then swaps the staged file in before the log is opened.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::store::SCHEMA_VERSION;

/// The log database file in the data folder.
pub const DB_FILE: &str = "qrzero.db";
/// The backups folder inside the data folder.
pub const BACKUP_DIR: &str = "backups";
const PENDING_DB: &str = "restore-pending.db";
const PENDING_INFO: &str = "restore-pending.json";
const STAGING: &str = "restore-staging.tmp";
const NAME_TIME: &str = "%Y-%m-%d-%H%M%S";
/// Tables every QRZero log has had since the first schema.
const REQUIRED_TABLES: &[&str] = &["logs", "station_callsigns", "locations", "qsos", "settings"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Made when QRZero starts, at most once a day.
    Auto,
    /// Made from Settings (or any other .db file put in the folder).
    Manual,
    /// The log as it was just before a restore replaced it.
    BeforeRestore,
}

impl Kind {
    fn suffix(self) -> &'static str {
        match self {
            Kind::Auto => "auto",
            Kind::Manual => "manual",
            Kind::BeforeRestore => "before-restore",
        }
    }

    fn of(name: &str) -> Kind {
        let stem = name.strip_suffix(".db").unwrap_or(name);
        if stem.ends_with("-auto") {
            Kind::Auto
        } else if stem.ends_with("-before-restore") {
            Kind::BeforeRestore
        } else {
            Kind::Manual
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Backup {
    /// File name inside the backups folder.
    pub name: String,
    pub size: u64,
    /// When it was made (unix seconds).
    pub created: i64,
    pub kind: Kind,
}

/// What a database file holds, from [`inspect`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbSummary {
    pub schema: i32,
    pub logs: i64,
    pub qsos: i64,
}

/// A restore waiting for the next start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pending {
    /// The backup's name, or the uploaded file's name.
    pub source: String,
    /// When it was staged (unix seconds).
    pub staged_at: i64,
    pub summary: DbSummary,
}

pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DB_FILE)
}

pub fn backup_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_DIR)
}

/// Copies the live log into the backups folder. Safe while QRZero is running:
/// it uses its own connection, so the log stays usable meanwhile.
pub fn create(data_dir: &Path, kind: Kind) -> Result<Backup> {
    create_at(data_dir, kind, Local::now().naive_local())
}

fn create_at(data_dir: &Path, kind: Kind, at: NaiveDateTime) -> Result<Backup> {
    let dir = backup_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let stamp = at.format(NAME_TIME);
    let mut name = format!("qrzero-{stamp}-{}.db", kind.suffix());
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("qrzero-{stamp}-{n}-{}.db", kind.suffix());
        n += 1;
    }
    // Write under a temporary name so a half-written file is never listed.
    let tmp = dir.join(format!(".{name}.tmp"));
    let _ = fs::remove_file(&tmp);
    let result = (|| -> Result<()> {
        let src = Connection::open_with_flags(db_path(data_dir), OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        src.busy_timeout(std::time::Duration::from_secs(10))?;
        src.execute("VACUUM INTO ?1", [tmp.to_string_lossy()])?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, dir.join(&name))?;
    let size = fs::metadata(dir.join(&name))?.len();
    Ok(Backup { created: local_ts(at), name, size, kind })
}

fn local_ts(at: NaiveDateTime) -> i64 {
    Local.from_local_datetime(&at).earliest().map_or_else(|| at.and_utc().timestamp(), |t| t.timestamp())
}

/// The time in a backup's name, if it has one.
fn name_time(name: &str) -> Option<NaiveDateTime> {
    let stamp = name.strip_prefix("qrzero-")?.get(..17)?;
    NaiveDateTime::parse_from_str(stamp, NAME_TIME).ok()
}

/// Backups in the folder, newest first.
pub fn list(data_dir: &Path) -> Result<Vec<Backup>> {
    let dir = backup_dir(data_dir);
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = entry.metadata()?;
        if !meta.is_file() || !valid_name(&name) {
            continue;
        }
        let created = name_time(&name).map(local_ts).unwrap_or_else(|| {
            meta.modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64)
        });
        out.push(Backup { kind: Kind::of(&name), name, size: meta.len(), created });
    }
    out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.name.cmp(&a.name)));
    Ok(out)
}

/// A plain .db file name: nothing that could reach outside the folder.
fn valid_name(name: &str) -> bool {
    name.len() <= 200
        && name.to_ascii_lowercase().ends_with(".db")
        && !name.starts_with('.')
        && !name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
}

/// The path of a listed backup.
pub fn file_path(data_dir: &Path, name: &str) -> Result<PathBuf> {
    if !valid_name(name) {
        return Err(Error::Invalid("not a backup file name".into()));
    }
    let path = backup_dir(data_dir).join(name);
    if !path.is_file() {
        return Err(Error::NotFound(format!("backup {name}")));
    }
    Ok(path)
}

pub fn delete(data_dir: &Path, name: &str) -> Result<()> {
    fs::remove_file(file_path(data_dir, name)?)?;
    Ok(())
}

/// True when no automatic backup has been made on `today` (local date).
pub fn auto_due(data_dir: &Path, today: NaiveDate) -> Result<bool> {
    Ok(!list(data_dir)?
        .iter()
        .filter(|b| b.kind == Kind::Auto)
        .any(|b| name_time(&b.name).is_some_and(|t| t.date() == today)))
}

/// Deletes all but the newest `keep` automatic backups. Manual ones are kept.
pub fn prune(data_dir: &Path, keep: usize) -> Result<usize> {
    let mut removed = 0;
    for b in list(data_dir)?.into_iter().filter(|b| b.kind == Kind::Auto).skip(keep) {
        fs::remove_file(backup_dir(data_dir).join(&b.name))?;
        removed += 1;
    }
    Ok(removed)
}

/// Checks that a file is a QRZero log this build can open, without changing it.
pub fn inspect(path: &Path) -> Result<DbSummary> {
    let bad = |e: rusqlite::Error| Error::Invalid(format!("this isn't a QRZero log ({e})"));
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(bad)?;
    let schema: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(bad)?;
    for table in REQUIRED_TABLES {
        let found: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", [table], |r| r.get(0))
            .map_err(bad)?;
        if !found || schema < 1 {
            return Err(Error::Invalid("this isn't a QRZero log".into()));
        }
    }
    if schema > SCHEMA_VERSION {
        return Err(Error::Invalid(format!(
            "this log was made by a newer QRZero (schema {schema}, this one knows {SCHEMA_VERSION}); update QRZero first"
        )));
    }
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(bad)?;
    if check != "ok" {
        return Err(Error::Invalid(format!("this log file is damaged: {check}")));
    }
    let logs = conn.query_row("SELECT COUNT(*) FROM logs", [], |r| r.get(0)).map_err(bad)?;
    let qsos = conn.query_row("SELECT COUNT(*) FROM qsos", [], |r| r.get(0)).map_err(bad)?;
    Ok(DbSummary { schema, logs, qsos })
}

/// Stages a listed backup to be restored on the next start.
pub fn stage_backup(data_dir: &Path, name: &str) -> Result<Pending> {
    let src = file_path(data_dir, name)?;
    stage(data_dir, name, |tmp| {
        fs::copy(&src, tmp)?;
        Ok(())
    })
}

/// Stages an uploaded database file to be restored on the next start.
pub fn stage_file(data_dir: &Path, source: &str, bytes: &[u8]) -> Result<Pending> {
    stage(data_dir, source, |tmp| Ok(fs::write(tmp, bytes)?))
}

fn stage(data_dir: &Path, source: &str, fill: impl FnOnce(&Path) -> Result<()>) -> Result<Pending> {
    let tmp = data_dir.join(STAGING);
    remove_db_files(&tmp)?;
    let checked = fill(&tmp).and_then(|()| {
        // The copy may be in WAL mode (a log copied straight out of a data
        // folder). Switch it to a single self-contained file before the
        // read-only check, which would otherwise leave -wal/-shm files behind.
        let conn = Connection::open_with_flags(&tmp, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.query_row("PRAGMA journal_mode = DELETE", [], |r| r.get::<_, String>(0))
            .map_err(|e| Error::Invalid(format!("this isn't a QRZero log ({e})")))?;
        drop(conn);
        inspect(&tmp)
    });
    let summary = match checked {
        Ok(s) => s,
        Err(e) => {
            let _ = remove_db_files(&tmp);
            return Err(e);
        }
    };
    let pending = Pending { source: source.to_string(), staged_at: chrono::Utc::now().timestamp(), summary };
    fs::rename(&tmp, data_dir.join(PENDING_DB))?;
    fs::write(data_dir.join(PENDING_INFO), serde_json::to_vec(&pending)?)?;
    Ok(pending)
}

/// The restore waiting for the next start, if any.
pub fn pending(data_dir: &Path) -> Option<Pending> {
    if !data_dir.join(PENDING_DB).is_file() {
        return None;
    }
    let info = fs::read(data_dir.join(PENDING_INFO)).ok().and_then(|b| serde_json::from_slice(&b).ok());
    Some(info.unwrap_or(Pending {
        source: "a backup".into(),
        staged_at: 0,
        summary: DbSummary { schema: 0, logs: 0, qsos: 0 },
    }))
}

/// Drops a staged restore.
pub fn cancel(data_dir: &Path) -> Result<()> {
    remove_db_files(&data_dir.join(PENDING_DB))?;
    remove_file_if_exists(&data_dir.join(PENDING_INFO))
}

/// Swaps a staged restore in. Call on startup before the log is opened.
///
/// The current log is first saved as a "before restore" backup; if that
/// fails, nothing is changed and the restore stays pending. The old log's
/// -wal and -shm files are removed so they can't be replayed into the
/// restored file. Returns what was restored.
pub fn apply_pending(data_dir: &Path) -> Result<Option<Pending>> {
    let Some(pending) = pending(data_dir) else {
        return Ok(None);
    };
    let staged = data_dir.join(PENDING_DB);
    if let Err(e) = inspect(&staged) {
        cancel(data_dir)?;
        return Err(e);
    }
    let db = db_path(data_dir);
    if db.exists() {
        create(data_dir, Kind::BeforeRestore)?;
    }
    remove_db_files(&db)?;
    fs::rename(&staged, &db)?;
    remove_file_if_exists(&data_dir.join(PENDING_INFO))?;
    Ok(Some(pending))
}

/// Puts back the newest "before restore" backup, for when a restored log
/// won't open. Returns its name.
pub fn roll_back(data_dir: &Path) -> Result<String> {
    let before = list(data_dir)?
        .into_iter()
        .find(|b| b.kind == Kind::BeforeRestore)
        .ok_or_else(|| Error::NotFound("a backup from before the restore".into()))?;
    let db = db_path(data_dir);
    remove_db_files(&db)?;
    fs::copy(backup_dir(data_dir).join(&before.name), &db)?;
    Ok(before.name)
}

/// Removes a database file and its -wal, -shm and -journal companions.
fn remove_db_files(path: &Path) -> Result<()> {
    for ext in ["", "-wal", "-shm", "-journal"] {
        let mut p = path.as_os_str().to_owned();
        p.push(ext);
        remove_file_if_exists(Path::new(&p))?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(Kind::of("qrzero-2026-10-08-153012-auto.db"), Kind::Auto);
        assert_eq!(Kind::of("qrzero-2026-10-08-153012-2-before-restore.db"), Kind::BeforeRestore);
        assert_eq!(Kind::of("mylog.db"), Kind::Manual);
        assert!(name_time("qrzero-2026-10-08-153012-2-auto.db").is_some());
        assert!(name_time("mylog.db").is_none());
        assert!(!valid_name("../qrzero.db"));
        assert!(!valid_name("a\\b.db"));
        assert!(!valid_name(".x.db.tmp"));
        assert!(valid_name("qrzero-2026-10-08-153012-auto.db"));
    }

    #[test]
    fn prune_keeps_newest_auto_backups() {
        let dir = tempfile::tempdir().unwrap();
        crate::Store::open(&db_path(dir.path())).unwrap();
        let day = |d: u32| NaiveDate::from_ymd_opt(2026, 10, d).unwrap().and_hms_opt(8, 0, 0).unwrap();
        for d in 1..=5 {
            create_at(dir.path(), Kind::Auto, day(d)).unwrap();
        }
        create_at(dir.path(), Kind::Manual, day(1)).unwrap();
        assert!(!auto_due(dir.path(), day(5).date()).unwrap());
        assert!(auto_due(dir.path(), day(6).date()).unwrap());
        assert_eq!(prune(dir.path(), 2).unwrap(), 3);
        let left: Vec<String> = list(dir.path()).unwrap().into_iter().map(|b| b.name).collect();
        assert_eq!(
            left,
            ["qrzero-2026-10-05-080000-auto.db", "qrzero-2026-10-04-080000-auto.db", "qrzero-2026-10-01-080000-manual.db"]
        );
        // Same second twice gets a different name.
        let b = create_at(dir.path(), Kind::Manual, day(1)).unwrap();
        assert_eq!(b.name, "qrzero-2026-10-01-080000-2-manual.db");
    }
}
