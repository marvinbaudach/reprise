//! A relink reads the missing-track state before it rewrites the row, so a
//! rival commit landing between the two must not fail it with a snapshot
//! conflict (#1188). The shared fixture places the rival commit
//! deterministically; see `rival_commit_test_support`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::tests::{imported_missing_track, moved_group, target_for, targets_for};
use super::*;
use crate::library::rival_commit_test_support::arm_on_first_write;

const DB_FILE: &str = "reprise.db";

/// The fixtures build an in-memory database; the rival needs a file both
/// connections can open, so copy the fixture's state into one.
fn into_file_db(memory: &Db, directory: &Path) -> Db {
    let path = directory.join(DB_FILE);
    memory
        .conn()
        .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
        .unwrap();
    Db::open_migrated(Some(&path)).unwrap()
}

fn is_present(db: &Db, track_id: i64) -> bool {
    db.conn()
        .query_row(
            "SELECT missing_since IS NULL FROM tracks WHERE id = ?1",
            [track_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn relinking_a_track_survives_a_rival_commit_between_its_read_and_its_write() {
    let (_temp, memory, track_id, new_path) = imported_missing_track("Relinked title");
    let target = target_for(&memory, track_id);
    let directory = tempfile::tempdir().unwrap();
    let db = into_file_db(&memory, directory.path());
    let flag = arm_on_first_write(db.conn(), &directory.path().join(DB_FILE), "tracks");

    relink_track(&db, &target, &new_path).unwrap();

    assert!(
        flag.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    assert!(is_present(&db, track_id));
}

#[test]
fn relinking_from_a_folder_survives_a_rival_commit_between_its_read_and_its_write() {
    let (_temp, memory, ids, new_folder) = moved_group(1);
    let targets = targets_for(&memory, &ids);
    let directory = tempfile::tempdir().unwrap();
    let db = into_file_db(&memory, directory.path());
    let flag = arm_on_first_write(db.conn(), &directory.path().join(DB_FILE), "tracks");
    let cancel = AtomicBool::new(false);

    let report = relink_from_folder(&db, &new_folder, &targets, &cancel, |_, _| {}).unwrap();

    assert!(
        flag.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(report.relinked, 1);
    assert!(is_present(&db, ids[0]));
}
