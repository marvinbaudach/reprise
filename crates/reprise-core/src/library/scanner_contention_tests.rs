//! Every scanner lease reads before it writes, so each one must survive a
//! rival commit landing between its first read and its first write (#1188).
//! A deferred transaction pins a read snapshot first and then fails the
//! write-lock upgrade with `SQLITE_BUSY_SNAPSHOT` (extended code 517), which
//! `busy_timeout` never retries; the GUI writes settings, ratings and the
//! listen journal on its own connection while a scan runs. The shared fixture
//! places the rival commit deterministically; see
//! `library::rival_commit_test_support`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use super::source::{
    LibraryDirectoryEntry, LibraryReadHandle, LibraryWalkOrder, LibraryWalkVisitor,
};
use super::tests::{completed, fixture_copy};
use super::*;
use crate::library::rival_commit_test_support::arm_on_first_write;
use crate::models::MissingReason;

const DB_FILE: &str = "reprise.db";

struct Scene {
    directory: tempfile::TempDir,
    music: tempfile::TempDir,
    db: Db,
}

impl Scene {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let music = tempfile::tempdir().unwrap();
        let db = Db::open_migrated(Some(&directory.path().join(DB_FILE))).unwrap();
        Self {
            directory,
            music,
            db,
        }
    }

    /// Makes the first write the scan prepares against `table` provoke a
    /// rival commit.
    fn arm(&self, table: &'static str) -> std::sync::Arc<AtomicBool> {
        arm_on_first_write(self.db.conn(), &self.directory.path().join(DB_FILE), table)
    }

    fn scan(&self) -> Result<ScanReport, ScanError> {
        scan_folder(&self.db, self.music.path()).map(completed)
    }
}

fn assert_interleaved(flag: &AtomicBool) {
    assert!(
        flag.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
}

#[test]
fn importing_a_file_survives_a_rival_commit_between_its_read_and_its_write() {
    let scene = Scene::new();
    fixture_copy(scene.music.path(), "a.flac");
    // The import's first statement clears a stale import error: a write that
    // follows the batch's read of the known row.
    let flag = scene.arm("import_errors");

    let report = scene.scan().unwrap();

    assert_interleaved(&flag);
    assert_eq!(report.added, 1);
}

#[test]
fn restoring_a_reappeared_file_survives_a_rival_commit_between_its_read_and_its_write() {
    let scene = Scene::new();
    fixture_copy(scene.music.path(), "a.flac");
    scene.scan().unwrap();
    scene
        .db
        .conn()
        .execute("UPDATE tracks SET missing_since = 1", [])
        .unwrap();
    let flag = scene.arm("tracks");

    scene.scan().unwrap();

    assert_interleaved(&flag);
    let still_missing: i64 = scene
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM tracks WHERE missing_since IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(still_missing, 0);
}

#[test]
fn marking_a_vanished_file_survives_a_rival_commit_between_its_read_and_its_write() {
    let scene = Scene::new();
    let gone = fixture_copy(scene.music.path(), "a.flac");
    scene.scan().unwrap();
    // A folder emptied of audio: with nothing observed, the tail starts with
    // the vanish evidence read and only then marks the rows.
    std::fs::remove_file(gone).unwrap();
    let flag = scene.arm("tracks");

    let report = scene.scan().unwrap();

    assert_interleaved(&flag);
    assert_eq!(report.vanished, 1);
}

/// The real Unix source, except that every question it is asked about one
/// watched path first tries to take the write lock on a second connection. A
/// refusal means the scan held the lock while asking the filesystem.
struct LockObservingSource {
    watched: PathBuf,
    rival: Mutex<Connection>,
    questions_under_the_lock: AtomicU32,
    questions: AtomicU32,
}

impl LockObservingSource {
    fn new(database: &Path, watched: PathBuf) -> Self {
        let rival = Connection::open(database).unwrap();
        rival.pragma_update(None, "busy_timeout", 0).unwrap();
        Self {
            watched,
            rival: Mutex::new(rival),
            questions_under_the_lock: AtomicU32::new(0),
            questions: AtomicU32::new(0),
        }
    }

    fn observe(&self, at: &Path) {
        if at != self.watched {
            return;
        }
        self.questions.fetch_add(1, Ordering::SeqCst);
        let rival = self.rival.lock().unwrap();
        match rival.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => rival.execute_batch("ROLLBACK").unwrap(),
            Err(_) => {
                self.questions_under_the_lock.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

impl LibrarySource for LockObservingSource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        UnixLibrarySource.residence_token(at)
    }

    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.mount_point(at)
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.display_name(at)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.container_name(at)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.relative_path(root, at)
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        UnixLibrarySource.open_read(at)
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        self.observe(at);
        UnixLibrarySource.probe(at, links)
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        UnixLibrarySource.read_directory(directory)
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        UnixLibrarySource.walk(root, order, visitor);
    }

    fn reachability(&self, at: &Path, stored: Option<i64>) -> MissingReason {
        self.observe(at);
        UnixLibrarySource.reachability(at, stored)
    }
}

/// Deciding that a file is gone takes stats, and on a slow share thousands of
/// them: the scan must ask the filesystem before it takes the write lock, or
/// every other writer waits for the share.
#[test]
fn deciding_what_vanished_touches_the_filesystem_outside_the_write_lock() {
    let scene = Scene::new();
    fixture_copy(scene.music.path(), "a.flac");
    let gone = fixture_copy(scene.music.path(), "b.flac");
    scene.scan().unwrap();
    std::fs::remove_file(&gone).unwrap();
    let source = LockObservingSource::new(&scene.directory.path().join(DB_FILE), gone);

    let report =
        completed(scan_folder_with_source(&source, &scene.db, scene.music.path()).unwrap());

    assert_eq!(report.vanished, 1);
    assert!(
        source.questions.load(Ordering::SeqCst) > 0,
        "it must have asked"
    );
    assert_eq!(source.questions_under_the_lock.load(Ordering::SeqCst), 0);
}

/// The verdicts are asked for without the lock, so the rows may change before
/// they are applied. A row that is no longer the present candidate it was is
/// not marked on the old answer.
#[test]
fn a_row_that_changed_after_the_source_was_asked_is_not_marked_on_the_old_answer() {
    let scene = Scene::new();
    fixture_copy(scene.music.path(), "a.flac");
    let gone = fixture_copy(scene.music.path(), "b.flac");
    scene.scan().unwrap();
    std::fs::remove_file(&gone).unwrap();
    let trace = WalkTrace {
        audio_files_seen: 1,
        observed_paths: HashSet::from([scene.music.path().join("a.flac")]),
        dirs: HashSet::from([scene.music.path().to_path_buf()]),
        failed: HashSet::new(),
    };
    let tx = scene.db.conn().unchecked_transaction().unwrap();
    let facts = reconcile::read_facts(&tx, scene.music.path(), trace).unwrap();
    tx.commit().unwrap();
    let plan = reconcile::plan(&UnixLibrarySource, scene.music.path(), facts);
    scene
        .db
        .conn()
        .execute(
            "UPDATE tracks SET removed_at = 1 WHERE path LIKE '%/b.flac'",
            [],
        )
        .unwrap();

    let tx = crate::events::immediate_transaction(scene.db.conn()).unwrap();
    let outcome = reconcile::apply(&tx, scene.music.path(), &plan, ScanReport::default()).unwrap();
    tx.commit().unwrap();

    assert_eq!(completed(outcome).vanished, 0);
    let marked: i64 = scene
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM tracks WHERE missing_since IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marked, 0);
}
