use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use rusqlite::Connection;

use super::source::{
    LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibraryReadHandle, LibrarySource,
    LibraryWalkControl, LibraryWalkOrder, LibraryWalkVisitor,
};
use super::source_tests::{scripted_virtual_file, ScriptedSource};
use super::*;

struct CountingWriter {
    database: Db,
    leases: AtomicUsize,
}

impl ScanWriter for CountingWriter {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        self.leases.fetch_add(1, Ordering::SeqCst);
        work(self.database.conn())
    }
}

struct HeldWriter {
    database: Db,
    held: Arc<AtomicBool>,
}

struct SkippingWriter;

impl ScanWriter for SkippingWriter {
    fn lease(
        &self,
        _work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        Ok(())
    }
}

impl ScanWriter for HeldWriter {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        assert!(!self.held.swap(true, Ordering::SeqCst));
        let result = work(self.database.conn());
        self.held.store(false, Ordering::SeqCst);
        result
    }
}

struct FailingWriter {
    database: Db,
    leases: AtomicUsize,
    fail_on: usize,
}

struct SqlFailingWriter {
    database: Db,
    leases: AtomicUsize,
    fail_on: usize,
}

impl ScanWriter for SqlFailingWriter {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        let lease = self.leases.fetch_add(1, Ordering::SeqCst) + 1;
        if lease != self.fail_on {
            return work(self.database.conn());
        }
        self.database.conn().execute_batch(
            "CREATE TEMP TRIGGER fail_track_insert
                 BEFORE INSERT ON tracks
                 BEGIN
                   SELECT RAISE(FAIL, 'injected batch write failure');
                 END;",
        )?;
        let result = work(self.database.conn());
        self.database
            .conn()
            .execute_batch("DROP TRIGGER fail_track_insert")?;
        result
    }
}

impl ScanWriter for FailingWriter {
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        let lease = self.leases.fetch_add(1, Ordering::SeqCst) + 1;
        if lease == self.fail_on {
            return Err(io::Error::other("injected writer lease failure").into());
        }
        work(self.database.conn())
    }
}

struct ObservedSource {
    inner: ScriptedSource,
    held: Arc<AtomicBool>,
    block_at: Option<PathBuf>,
    entered: Option<mpsc::SyncSender<()>>,
    release: Option<Mutex<mpsc::Receiver<()>>>,
}

impl ObservedSource {
    fn plain(inner: ScriptedSource, held: Arc<AtomicBool>) -> Self {
        Self {
            inner,
            held,
            block_at: None,
            entered: None,
            release: None,
        }
    }

    fn blocking(
        inner: ScriptedSource,
        block_at: PathBuf,
        entered: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            inner,
            held: Arc::new(AtomicBool::new(false)),
            block_at: Some(block_at),
            entered: Some(entered),
            release: Some(Mutex::new(release)),
        }
    }

    fn assert_writer_free(&self, operation: &str) {
        assert!(
            !self.held.load(Ordering::SeqCst),
            "the writer was leased during source {operation}"
        );
    }
}

impl LibrarySource for ObservedSource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        self.inner.residence_token(at)
    }

    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        self.inner.mount_point(at)
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        self.inner.display_name(at)
    }

    fn container_name(&self, at: &Path) -> Option<String> {
        self.inner.container_name(at)
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        self.inner.relative_path(root, at)
    }

    fn open_read(&self, at: &Path) -> io::Result<LibraryReadHandle> {
        self.assert_writer_free("open_read");
        if self.block_at.as_deref() == Some(at) {
            self.entered.as_ref().unwrap().send(()).unwrap();
            self.release
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .recv()
                .unwrap();
        }
        self.inner.open_read(at)
    }

    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        self.inner.probe(at, links)
    }

    fn read_directory(&self, directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        self.assert_writer_free("read_directory");
        self.inner.read_directory(directory)
    }

    fn walk(&self, root: &Path, order: LibraryWalkOrder, visitor: &mut dyn LibraryWalkVisitor) {
        self.assert_writer_free("walk");
        let _ = self.read_directory(root);
        struct ObservingVisitor<'a> {
            held: &'a AtomicBool,
            visitor: &'a mut dyn LibraryWalkVisitor,
        }
        impl LibraryWalkVisitor for ObservingVisitor<'_> {
            fn visit(&mut self, item: super::source::LibraryWalkItem) -> LibraryWalkControl {
                assert!(
                    !self.held.load(Ordering::SeqCst),
                    "the writer was leased while the source delivered a walk item"
                );
                self.visitor.visit(item)
            }
        }
        self.inner.walk(
            root,
            order,
            &mut ObservingVisitor {
                held: &self.held,
                visitor,
            },
        );
    }
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac")).unwrap()
}

fn item_source(root: &Path, count: usize) -> ScriptedSource {
    let fixture = fixture_bytes();
    let mut source = ScriptedSource::new(
        (0..count)
            .map(|index| {
                scripted_virtual_file(
                    &root.join(format!("track-{index:02}.flac")),
                    fixture.len() as u64,
                )
            })
            .collect(),
    );
    for index in 0..count {
        source = source.with_content(root.join(format!("track-{index:02}.flac")), fixture.clone());
    }
    source
}

fn file_database(directory: &tempfile::TempDir) -> (PathBuf, Db) {
    let path = directory.path().join("reprise.db");
    let database = Db::open_migrated(Some(&path)).unwrap();
    (path, database)
}

fn row_count(database: &Db) -> i64 {
    database
        .conn()
        .query_row("SELECT count(*) FROM tracks", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn source_io_runs_without_a_writer_lease() {
    let directory = tempfile::tempdir().unwrap();
    let (_, database) = file_database(&directory);
    let held = Arc::new(AtomicBool::new(false));
    let writer = HeldWriter {
        database,
        held: Arc::clone(&held),
    };
    let source = ObservedSource::plain(item_source(directory.path(), 17), held);

    let callback_held = Arc::clone(&writer.held);
    let outcome =
        scan_folder_with_writer_and_progress(&source, &writer, directory.path(), move |_| {
            assert!(
                !callback_held.load(Ordering::SeqCst),
                "progress callbacks must run after the writer lease is released"
            );
        })
        .unwrap();

    assert_eq!(super::tests::completed(outcome).added, 17);
}

#[test]
fn a_writer_that_skips_lease_work_returns_an_invariant_error() {
    let directory = tempfile::tempdir().unwrap();
    let source = item_source(directory.path(), 1);

    let result =
        scan_folder_with_writer_and_progress(&source, &SkippingWriter, directory.path(), |_| {});

    assert!(matches!(result, Err(ScanError::InternalInvariant(_))));
}

#[test]
fn mobile_sync_metadata_is_read_without_a_writer_lease() {
    use crate::device_sync::track_metadata_list::{TrackMetadataList, FILE_NAME};

    let directory = tempfile::tempdir().unwrap();
    let (_, database) = file_database(&directory);
    let held = Arc::new(AtomicBool::new(false));
    let writer = HeldWriter {
        database,
        held: Arc::clone(&held),
    };
    let track = directory.path().join("track.flac");
    let list = directory.path().join(FILE_NAME);
    let fixture = fixture_bytes();
    let encoded = TrackMetadataList::new(Vec::new()).encode().unwrap();
    let source = ScriptedSource::new(vec![
        scripted_virtual_file(&track, fixture.len() as u64),
        scripted_virtual_file(&list, encoded.len() as u64),
    ])
    .with_content(track, fixture)
    .with_content(list, encoded);
    let source = ObservedSource::plain(source, held);

    let outcome =
        scan_folder_with_writer_and_progress(&source, &writer, directory.path(), |_| {}).unwrap();

    assert_eq!(super::tests::completed(outcome).added, 1);
}

#[test]
fn completed_batches_are_visible_before_the_walk_ends() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, database) = file_database(&directory);
    let writer = Arc::new(Mutex::new(database));
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let source = Arc::new(ObservedSource::blocking(
        item_source(directory.path(), 40),
        directory.path().join("track-32.flac"),
        entered_tx,
        release_rx,
    ));
    let scan_writer = Arc::clone(&writer);
    let scan_source = Arc::clone(&source);
    let root = directory.path().to_path_buf();
    let scan = std::thread::spawn(move || {
        scan_folder_with_writer_and_progress(&*scan_source, &*scan_writer, &root, |_| {})
    });

    entered_rx.recv().unwrap();
    let reader = Db::open_ready(&database_path).unwrap();
    assert_eq!(row_count(&reader), 32);
    release_tx.send(()).unwrap();
    assert_eq!(
        super::tests::completed(scan.join().unwrap().unwrap()).added,
        40
    );
}

#[test]
fn forty_items_use_two_leases_per_batch_and_one_tail_lease() {
    let directory = tempfile::tempdir().unwrap();
    let (_, database) = file_database(&directory);
    let writer = CountingWriter {
        database,
        leases: AtomicUsize::new(0),
    };
    let source = item_source(directory.path(), 40);

    let outcome =
        scan_folder_with_writer_and_progress(&source, &writer, directory.path(), |_| {}).unwrap();

    assert_eq!(super::tests::completed(outcome).added, 40);
    assert_eq!(writer.leases.load(Ordering::SeqCst), 8);
}

#[test]
fn a_failed_later_batch_keeps_earlier_commits_without_marking_missing() {
    let directory = tempfile::tempdir().unwrap();
    let (_, database) = file_database(&directory);
    let writer = FailingWriter {
        database,
        leases: AtomicUsize::new(0),
        fail_on: 5,
    };
    let source = item_source(directory.path(), 40);

    let result = scan_folder_with_writer_and_progress(&source, &writer, directory.path(), |_| {});

    assert!(matches!(result, Err(ScanError::Io(_))));
    assert_eq!(row_count(&writer.database), 16);
    let marked_missing: i64 = writer
        .database
        .conn()
        .query_row(
            "SELECT count(*) FROM tracks WHERE missing_since IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marked_missing, 0);
}

#[test]
fn a_sql_failure_inside_a_later_batch_rolls_that_batch_back_only() {
    let directory = tempfile::tempdir().unwrap();
    let (_, database) = file_database(&directory);
    let writer = SqlFailingWriter {
        database,
        leases: AtomicUsize::new(0),
        fail_on: 5,
    };
    let source = item_source(directory.path(), 40);

    let result = scan_folder_with_writer_and_progress(&source, &writer, directory.path(), |_| {});

    assert!(matches!(result, Err(ScanError::Sqlite(_))));
    assert_eq!(row_count(&writer.database), 16);
    let marked_missing: i64 = writer
        .database
        .conn()
        .query_row(
            "SELECT count(*) FROM tracks WHERE missing_since IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marked_missing, 0);
}

#[test]
fn a_row_deleted_after_classification_is_not_recreated() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, database) = file_database(&directory);
    let path = directory.path().join("track-00.flac");
    let initial = item_source(directory.path(), 1);
    super::tests::completed(
        scan_folder_with_source(&initial, &database, directory.path()).unwrap(),
    );
    database
        .conn()
        .execute(
            "UPDATE tracks SET file_mtime = -1 WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
        )
        .unwrap();

    let writer = Arc::new(Mutex::new(database));
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let source = Arc::new(ObservedSource::blocking(
        item_source(directory.path(), 1),
        path.clone(),
        entered_tx,
        release_rx,
    ));
    let scan_writer = Arc::clone(&writer);
    let scan_source = Arc::clone(&source);
    let root = directory.path().to_path_buf();
    let scan = std::thread::spawn(move || {
        scan_folder_with_writer_and_progress(&*scan_source, &*scan_writer, &root, |_| {})
    });

    entered_rx.recv().unwrap();
    let other = Db::open_ready(&database_path).unwrap();
    other
        .conn()
        .execute(
            "DELETE FROM tracks WHERE path = ?1",
            [path.to_string_lossy().as_ref()],
        )
        .unwrap();
    release_tx.send(()).unwrap();
    let report = super::tests::completed(scan.join().unwrap().unwrap());

    assert_eq!(report.added, 0);
    assert_eq!(report.updated, 0);
    assert_eq!(report.skipped_unchanged, 1);
    assert_eq!(row_count(&other), 0);
}
