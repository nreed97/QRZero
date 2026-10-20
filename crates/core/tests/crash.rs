//! A hard kill in the middle of logging must lose nothing that was acknowledged.
//!
//! The test starts itself again as a child process that logs QSOs forever and
//! prints each id once `insert_qso` has returned. The parent kills it (SIGKILL
//! on Unix, TerminateProcess on Windows) while it is mid-write, then opens the
//! log the way a restart would.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use qrzero_core::adif::Fields;
use qrzero_core::backup::{self, Kind};
use qrzero_core::Store;

const CHILD_DIR: &str = "QRZERO_CRASH_CHILD_DIR";

fn qso(i: usize) -> Fields {
    [
        ("CALL", format!("K{i}ABC")),
        ("QSO_DATE", "20240101".to_string()),
        ("TIME_ON", "1200".to_string()),
        ("BAND", "20m".to_string()),
        ("MODE", "CW".to_string()),
        ("COMMENT", "x".repeat(200)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// The child half: log until killed. Does nothing in a normal test run.
#[test]
fn child_logs_until_killed() {
    let Ok(dir) = std::env::var(CHILD_DIR) else { return };
    let st = Store::open(&backup::db_path(Path::new(&dir))).unwrap();
    let log = match st.list_logs().unwrap().first() {
        Some(l) => l.id,
        None => st.create_log("Crash").unwrap().id,
    };
    println!("ready");
    for i in 0.. {
        let id = st.insert_qso(log, None, &qso(i)).unwrap().id;
        println!("logged {i} {id}");
    }
}

fn count(st: &Store) -> i64 {
    st.list_logs().unwrap().iter().map(|l| l.qso_count).sum()
}

fn kill_mid_write(dir: &Path, acknowledged_before_kill: usize) -> usize {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["child_logs_until_killed", "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_DIR, dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut logged = 0;
    while logged < acknowledged_before_kill {
        let line = lines.next().expect("child ended early").unwrap();
        if line.starts_with("logged ") {
            logged += 1;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    // Lines the child printed before it died still count: those QSOs were acknowledged.
    logged + lines.map_while(Result::ok).filter(|l| l.starts_with("logged ")).count()
}

fn leftover_files(dir: &Path) -> Vec<PathBuf> {
    ["-wal", "-shm"]
        .iter()
        .map(|ext| {
            let mut p = backup::db_path(dir).into_os_string();
            p.push(ext);
            PathBuf::from(p)
        })
        .filter(|p| p.exists())
        .collect()
}

#[test]
fn killed_mid_write_loses_no_acknowledged_qso() {
    let dir = tempfile::tempdir().unwrap();
    let acknowledged = kill_mid_write(dir.path(), 300);
    // A kill leaves the -wal behind (the log was never closed); a restart must replay it.
    assert!(!leftover_files(dir.path()).is_empty(), "expected the kill to leave a -wal file");

    let st = Store::open(&backup::db_path(dir.path())).unwrap();
    let found = count(&st);
    assert!(found >= acknowledged as i64, "{acknowledged} QSOs were acknowledged but only {found} survived");
    assert!(found <= acknowledged as i64 + 1, "at most the one write in flight may also have landed");
    drop(st);

    // The file itself is sound, not just countable.
    let summary = backup::inspect(&backup::db_path(dir.path())).unwrap();
    assert_eq!(summary.qsos, found);
    let conn = rusqlite::Connection::open(backup::db_path(dir.path())).unwrap();
    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(check, "ok");
}

#[test]
fn backup_after_a_kill_includes_everything_and_restores() {
    let dir = tempfile::tempdir().unwrap();
    let acknowledged = kill_mid_write(dir.path(), 150) as i64;

    // The next start makes its daily backup from the log the kill left behind.
    let st = Store::open(&backup::db_path(dir.path())).unwrap();
    let made = backup::create(dir.path(), Kind::Auto).unwrap();
    let live = count(&st);
    assert!(live >= acknowledged);
    assert_eq!(backup::inspect(&backup::file_path(dir.path(), &made.name).unwrap()).unwrap().qsos, live);

    // More QSOs arrive, then the operator restores the backup: back to exactly what it held.
    let log = st.list_logs().unwrap()[0].id;
    for i in 0..5 {
        st.insert_qso(log, None, &qso(10_000 + i)).unwrap();
    }
    drop(st);
    backup::stage_backup(dir.path(), &made.name).unwrap();
    assert!(backup::apply_pending(dir.path()).unwrap().is_some());
    let restored = Store::open(&backup::db_path(dir.path())).unwrap();
    assert_eq!(count(&restored), live);
    // The log as it stood before the restore was kept.
    let before = backup::list(dir.path()).unwrap().into_iter().find(|b| b.kind == Kind::BeforeRestore).unwrap();
    assert_eq!(backup::inspect(&backup::file_path(dir.path(), &before.name).unwrap()).unwrap().qsos, live + 5);
}

#[test]
fn a_killed_log_is_not_replayed_into_a_restored_one() {
    let dir = tempfile::tempdir().unwrap();
    kill_mid_write(dir.path(), 100);
    let made = backup::create(dir.path(), Kind::Manual).unwrap();
    let kept = backup::inspect(&backup::file_path(dir.path(), &made.name).unwrap()).unwrap().qsos;
    // The app runs on, logs more, and is killed again: the newest QSOs sit only in the -wal file.
    kill_mid_write(dir.path(), 100);
    assert!(!leftover_files(dir.path()).is_empty());

    backup::stage_backup(dir.path(), &made.name).unwrap();
    backup::apply_pending(dir.path()).unwrap().unwrap();
    assert!(leftover_files(dir.path()).is_empty(), "the old log's -wal must not be left to replay");
    assert_eq!(count(&Store::open(&backup::db_path(dir.path())).unwrap()), kept);
}
