//! T0.4 — headless, single-process notifier tests. A commit through a second
//! connection in the *same* process wakes the notifier by the identical
//! `PRAGMA data_version` mechanism a foreign process would trigger, so these
//! never need a second process or a display.
//!
//! The wake windows are deliberately generous: on a host whose inotify watch
//! quota is exhausted `notify` cannot arm (`MaxFilesWatch`) and the notifier
//! degrades to the 2-second polling fallback — the very degradation
//! [`polling_fallback_also_wakes_on_a_foreign_commit`] exercises on purpose.
//! Either way the `data_version` wake is what is under test, so the window
//! must comfortably clear the 2-second poll even under concurrent test load.

use std::sync::mpsc;
use std::time::Duration;

use super::*;

/// Comfortably longer than the 2-second polling fallback so the test is robust
/// whether `notify` armed (wake in ~250 ms) or degraded to polling.
const WAKE_TIMEOUT: Duration = Duration::from_secs(8);

/// Creates and migrates a scratch database, returning its path (kept alive by
/// the returned `TempDir`).
fn scratch_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("reprise.db");
    crate::db::open_migrated(Some(&db_path)).unwrap();
    (temp, db_path)
}

fn commit_foreign_change(db_path: &Path) {
    let writer = crate::db::open_migrated(Some(db_path)).unwrap();
    writer
        .execute(
            "INSERT INTO settings (key, value) VALUES ('notifier-probe', '1') \
             ON CONFLICT(key) DO UPDATE SET value = value || '1'",
            [],
        )
        .unwrap();
}

#[test]
fn commit_from_another_connection_wakes_the_notifier() {
    let (_temp, db_path) = scratch_db();
    let (tx, rx) = mpsc::channel();

    let handle = Notifier::start(&db_path, move || {
        let _ = tx.send(());
    })
    .expect("notifier must arm on a normal temp directory");

    commit_foreign_change(&db_path);

    rx.recv_timeout(WAKE_TIMEOUT)
        .expect("a foreign commit must wake the notifier");
    drop(handle);
}

#[test]
fn notifier_stays_quiet_without_a_foreign_commit() {
    let (_temp, db_path) = scratch_db();
    let (tx, rx) = mpsc::channel();

    let handle = Notifier::start(&db_path, move || {
        let _ = tx.send(());
    })
    .expect("notifier must arm on a normal temp directory");

    // Longer than the debounce, shorter than the poll fallback: with no
    // foreign commit the `data_version` gate must suppress any wake, even if
    // opening the watch produced incidental filesystem events.
    assert!(
        rx.recv_timeout(Duration::from_millis(800)).is_err(),
        "the notifier must not fire without an actual data change"
    );
    drop(handle);
}

#[test]
fn polling_fallback_also_wakes_on_a_foreign_commit() {
    let (_temp, db_path) = scratch_db();
    let (tx, rx) = mpsc::channel();

    // Degraded mode: as if `notify` could not be armed, the notifier polls
    // `data_version` on the 2-second fallback cadence and still wakes.
    let handle = Notifier::start_polling_for_test(&db_path, move || {
        let _ = tx.send(());
    })
    .expect("polling notifier must start");

    commit_foreign_change(&db_path);

    rx.recv_timeout(WAKE_TIMEOUT)
        .expect("the polling fallback must detect a foreign commit within its window");
    drop(handle);
}

#[test]
fn start_returns_none_when_the_database_cannot_be_opened() {
    let temp = tempfile::tempdir().unwrap();
    // A directory path is not an openable SQLite database file, so the
    // connection open fails and the notifier degrades to `None` (the app stays
    // usable, just without live updates) rather than panicking.
    let handle = Notifier::start(temp.path(), || {});
    assert!(handle.is_none());
}
