//! A relink reads the missing-track state before it rewrites the row, so a
//! rival commit landing between the two must not fail it with a snapshot
//! conflict (#1188). The shared fixture places the rival commit
//! deterministically; see `rival_commit_test_support`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use super::tests::{imported_missing_track, moved_group, target_for, targets_for};
use super::*;
use crate::library::rival_commit_test_support::arm_on_first_write;
use crate::library::source::{LibraryDirectoryEntry, LibraryReadHandle, LibraryWalkVisitor};

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

/// A Unix-backed source that notes every call made while the database's write
/// lock is held: a rival connection with no busy timeout tries to take the lock
/// at each call, and a refusal means the relink holds it across filesystem I/O.
struct LockObservingSource {
    rival: Mutex<rusqlite::Connection>,
    calls_under_the_lock: AtomicU32,
}

impl LockObservingSource {
    fn new(directory: &Path) -> Self {
        let rival = rusqlite::Connection::open(directory.join(DB_FILE)).unwrap();
        rival.pragma_update(None, "busy_timeout", 0).unwrap();
        Self {
            rival: Mutex::new(rival),
            calls_under_the_lock: AtomicU32::new(0),
        }
    }

    fn observe(&self) {
        let rival = self.rival.lock().unwrap();
        match rival.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => rival.execute_batch("ROLLBACK").unwrap(),
            Err(_) => {
                self.calls_under_the_lock.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn calls_under_the_lock(&self) -> u32 {
        self.calls_under_the_lock.load(Ordering::SeqCst)
    }
}

impl LibrarySource for LockObservingSource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        self.observe();
        UnixLibrarySource.residence_token(at)
    }

    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        self.observe();
        UnixLibrarySource.mount_point(at)
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        self.observe();
        UnixLibrarySource.display_name(at)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        self.observe();
        UnixLibrarySource.container_name(at)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.relative_path(root, at)
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        self.observe();
        UnixLibrarySource.open_read(at)
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        self.observe();
        UnixLibrarySource.probe(at, links)
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        self.observe();
        UnixLibrarySource.read_directory(directory)
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        UnixLibrarySource.walk(root, order, visitor);
    }
}

#[test]
fn relinking_a_track_touches_the_filesystem_only_outside_the_write_lock() {
    let (_temp, memory, track_id, new_path) = imported_missing_track("Relinked title");
    let target = target_for(&memory, track_id);
    let directory = tempfile::tempdir().unwrap();
    let db = into_file_db(&memory, directory.path());
    let source = LockObservingSource::new(directory.path());

    relink_track_with_source(&source, &db, &target, &new_path).unwrap();

    assert!(is_present(&db, track_id));
    assert_eq!(source.calls_under_the_lock(), 0);
}

#[test]
fn relinking_from_a_folder_touches_the_filesystem_only_outside_the_write_lock() {
    let (_temp, memory, ids, new_folder) = moved_group(1);
    let targets = targets_for(&memory, &ids);
    let directory = tempfile::tempdir().unwrap();
    let db = into_file_db(&memory, directory.path());
    let source = LockObservingSource::new(directory.path());
    let cancel = AtomicBool::new(false);

    let report = relink_from_folder_with_source(
        &source,
        &db,
        &new_folder,
        &targets,
        &cancel,
        &mut |_, _| {},
    )
    .unwrap();

    assert_eq!(report.relinked, 1);
    assert!(is_present(&db, ids[0]));
    assert_eq!(source.calls_under_the_lock(), 0);
}

/// A file that matches no missing track writes nothing, so it must not wait for
/// the write lock another connection holds. The unmatched file is created while
/// the matching one still exists: a filesystem may hand a freed inode number to
/// the next file it creates, and a file that reuses the missing track's
/// `(device, inode)` is that track, moved, and does write.
#[test]
fn a_folder_file_that_matches_nothing_never_waits_for_the_write_lock() {
    let (_temp, memory, ids, new_folder) = moved_group(1);
    let targets = targets_for(&memory, &ids);
    let foreign = new_folder.join("foreign.flac");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
        &foreign,
    )
    .unwrap();
    std::fs::remove_file(new_folder.join("00.flac")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let db = into_file_db(&memory, directory.path());
    db.conn().pragma_update(None, "busy_timeout", 0).unwrap();
    let holder = rusqlite::Connection::open(directory.path().join(DB_FILE)).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let cancel = AtomicBool::new(false);

    let report = relink_from_folder(&db, &new_folder, &targets, &cancel, |_, _| {}).unwrap();

    assert_eq!(report.relinked, 0);
}
