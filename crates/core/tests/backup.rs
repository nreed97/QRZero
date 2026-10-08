use std::path::Path;

use qrzero_core::adif::Fields;
use qrzero_core::backup::{self, Kind};
use qrzero_core::Store;

fn qso(call: &str) -> Fields {
    [("CALL", call), ("QSO_DATE", "20240101"), ("TIME_ON", "1200"), ("BAND", "20m"), ("MODE", "CW")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn open_with(dir: &Path, calls: &[&str]) -> Store {
    let st = Store::open(&backup::db_path(dir)).unwrap();
    let log = match st.list_logs().unwrap().first() {
        Some(l) => l.id,
        None => st.create_log("Test").unwrap().id,
    };
    for c in calls {
        st.insert_qso(log, None, &qso(c)).unwrap();
    }
    st
}

fn qso_count(st: &Store) -> i64 {
    st.list_logs().unwrap().iter().map(|l| l.qso_count).sum()
}

#[test]
fn backup_while_open_includes_unsaved_wal() {
    let dir = tempfile::tempdir().unwrap();
    let st = open_with(dir.path(), &["W1AW", "K1ABC"]);
    // The store is still open, so the QSOs may only be in the -wal file.
    let b = backup::create(dir.path(), Kind::Manual).unwrap();
    assert_eq!(b.kind, Kind::Manual);
    assert!(b.size > 0);
    let path = backup::file_path(dir.path(), &b.name).unwrap();
    let s = backup::inspect(&path).unwrap();
    assert_eq!((s.logs, s.qsos), (1, 2));
    // Backups are single files: nothing left beside them.
    let names: Vec<_> = std::fs::read_dir(backup::backup_dir(dir.path())).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 1, "{names:?}");
    // The live log keeps working.
    st.insert_qso(1, None, &qso("N0CALL")).unwrap();
    assert_eq!(backup::list(dir.path()).unwrap().len(), 1);
    backup::delete(dir.path(), &b.name).unwrap();
    assert!(backup::list(dir.path()).unwrap().is_empty());
    assert!(backup::delete(dir.path(), "../qrzero.db").is_err());
}

#[test]
fn rejects_files_that_are_not_qrzero_logs() {
    let dir = tempfile::tempdir().unwrap();
    let err = backup::stage_file(dir.path(), "notes.txt", b"hello, this is not a database at all, just some text").unwrap_err();
    assert!(err.to_string().contains("isn't a QRZero log"), "{err}");
    let err = backup::stage_file(dir.path(), "empty.db", b"").unwrap_err();
    assert!(err.to_string().contains("isn't a QRZero log"), "{err}");

    // Another program's SQLite database.
    let other = dir.path().join("other.db");
    let c = rusqlite::Connection::open(&other).unwrap();
    c.execute_batch("CREATE TABLE things (x); PRAGMA user_version = 3;").unwrap();
    drop(c);
    let err = backup::stage_file(dir.path(), "other.db", &std::fs::read(&other).unwrap()).unwrap_err();
    assert!(err.to_string().contains("isn't a QRZero log"), "{err}");

    // A log from a newer QRZero.
    let newer = dir.path().join("newer.db");
    drop(Store::open(&newer).unwrap());
    let c = rusqlite::Connection::open(&newer).unwrap();
    c.execute_batch("PRAGMA user_version = 999;").unwrap();
    drop(c);
    let err = backup::inspect(&newer).unwrap_err();
    assert!(err.to_string().contains("newer QRZero"), "{err}");
    let err = backup::stage_file(dir.path(), "newer.db", &std::fs::read(&newer).unwrap()).unwrap_err();
    assert!(err.to_string().contains("newer QRZero"), "{err}");

    assert!(backup::pending(dir.path()).is_none());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("restore"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn staged_restore_swaps_in_on_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let st = open_with(dir.path(), &["W1AW"]);
    let b = backup::create(dir.path(), Kind::Manual).unwrap();
    st.insert_qso(1, None, &qso("K1ABC")).unwrap();

    let p = backup::stage_backup(dir.path(), &b.name).unwrap();
    assert_eq!(p.source, b.name);
    assert_eq!(p.summary.qsos, 1);
    assert_eq!(backup::pending(dir.path()).unwrap().summary.qsos, 1);
    // Staging doesn't touch the open log.
    assert_eq!(qso_count(&st), 2);
    drop(st); // QRZero closes

    let applied = backup::apply_pending(dir.path()).unwrap().unwrap();
    assert_eq!(applied.source, b.name);
    assert!(backup::pending(dir.path()).is_none());
    assert!(!dir.path().join("qrzero.db-wal").exists());
    let st = Store::open(&backup::db_path(dir.path())).unwrap();
    assert_eq!(qso_count(&st), 1);

    // The log as it was before the restore was kept.
    let before: Vec<_> = backup::list(dir.path()).unwrap().into_iter().filter(|b| b.kind == Kind::BeforeRestore).collect();
    assert_eq!(before.len(), 1);
    let s = backup::inspect(&backup::file_path(dir.path(), &before[0].name).unwrap()).unwrap();
    assert_eq!(s.qsos, 2);
    // Nothing pending: nothing happens.
    drop(st);
    assert!(backup::apply_pending(dir.path()).unwrap().is_none());
}

#[test]
fn restore_of_an_uploaded_wal_mode_copy_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    // A log copied straight out of another data folder (WAL mode header).
    drop(open_with(src.path(), &["W1AW", "K1ABC", "N0CALL"]));
    let bytes = std::fs::read(backup::db_path(src.path())).unwrap();
    drop(open_with(dir.path(), &["G4ABC"]));

    backup::stage_file(dir.path(), "old.db", &bytes).unwrap();
    backup::cancel(dir.path()).unwrap();
    assert!(backup::pending(dir.path()).is_none());
    assert!(backup::apply_pending(dir.path()).unwrap().is_none());

    let p = backup::stage_file(dir.path(), "old.db", &bytes).unwrap();
    assert_eq!(p.summary.qsos, 3);
    let staged: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("restore"))
        .collect();
    assert_eq!(staged.len(), 2, "{staged:?}"); // the file and its note, no -wal/-shm
    backup::apply_pending(dir.path()).unwrap();
    assert_eq!(qso_count(&Store::open(&backup::db_path(dir.path())).unwrap()), 3);
}

#[test]
fn restores_an_older_schema_and_migrates_it() {
    let dir = tempfile::tempdir().unwrap();
    drop(open_with(dir.path(), &["W1AW"]));
    let old = dir.path().join("v1.db");
    let c = rusqlite::Connection::open(&old).unwrap();
    // Just enough of schema 1 to pass the checks.
    c.execute_batch(
        "CREATE TABLE logs (id INTEGER PRIMARY KEY, name TEXT NOT NULL, created_at INTEGER NOT NULL);
         CREATE TABLE station_callsigns (id INTEGER PRIMARY KEY, log_id INTEGER NOT NULL, callsign TEXT NOT NULL, is_default INTEGER NOT NULL DEFAULT 0, UNIQUE (log_id, callsign));
         CREATE TABLE locations (id INTEGER PRIMARY KEY, log_id INTEGER NOT NULL, name TEXT NOT NULL, is_default INTEGER NOT NULL DEFAULT 0, fields TEXT NOT NULL DEFAULT '{}');
         CREATE TABLE qsos (id INTEGER PRIMARY KEY, log_id INTEGER NOT NULL, location_id INTEGER, call TEXT NOT NULL, time_on INTEGER NOT NULL, band TEXT, mode TEXT, submode TEXT, freq REAL, station_callsign TEXT, dxcc INTEGER, fields TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
         CREATE INDEX qsos_log_time ON qsos (log_id, time_on);
         CREATE INDEX qsos_log_call ON qsos (log_id, call, time_on);
         CREATE INDEX qsos_log_band_mode ON qsos (log_id, band, mode, submode, time_on);
         CREATE INDEX qsos_log_dxcc ON qsos (log_id, dxcc, band, mode);
         CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE lookup_cache (call TEXT PRIMARY KEY, source TEXT NOT NULL, fetched_at INTEGER NOT NULL, data TEXT NOT NULL);
         INSERT INTO logs VALUES (1, 'Old', 0);
         PRAGMA user_version = 1;",
    )
    .unwrap();
    drop(c);
    let p = backup::stage_file(dir.path(), "v1.db", &std::fs::read(&old).unwrap()).unwrap();
    assert_eq!(p.summary.schema, 1);
    backup::apply_pending(dir.path()).unwrap();
    let st = Store::open(&backup::db_path(dir.path())).unwrap();
    assert_eq!(st.list_logs().unwrap()[0].name, "Old");
    // Migrated far enough to use notes (schema 5).
    st.set_note(1, "W1AW", "hi").unwrap();
}
