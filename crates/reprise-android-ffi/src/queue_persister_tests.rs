use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reprise_core::db::Db;
use reprise_core::library::session;
use reprise_core::queue::Queue;

use super::test_support::{library_in, RecordingListener, RecordingPort};
use crate::playback::AndroidPlaybackState;
use crate::playback_session::queue_persistence;
use crate::queue_persister::QueuePersister;
use crate::queue_snapshot_file::FILE_NAME;
use crate::{AndroidPlaybackSession, MusicLibrary};

fn queue(ids: Vec<i64>, position: usize) -> Queue {
    let mut queue = Queue::new();
    queue.set_tracks(ids, position);
    queue
}

fn database_in(directory: &Path) -> (std::path::PathBuf, Arc<Mutex<Db>>) {
    let path = directory.join(crate::DATABASE_FILE_NAME);
    let database = Db::open_migrated(Some(&path)).unwrap();
    (path, Arc::new(Mutex::new(database)))
}

fn saved_queue(path: &Path) -> reprise_core::queue::QueueSnapshot {
    let database = Db::open_ready(path).unwrap();
    session::load(&database).queue
}

fn session_for(library: Arc<crate::MusicLibrary>) -> AndroidPlaybackSession {
    AndroidPlaybackSession::new(
        library,
        Box::new(RecordingPort {
            calls: Arc::new(Mutex::new(Vec::new())),
            bridge: Arc::new(Mutex::new(None)),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap()
}

fn seed_tracks(library: &MusicLibrary, directory: &Path, count: usize) -> Vec<i64> {
    let music = directory.join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../android/app/src/main/assets/sine.flac");
    for index in 0..count {
        std::fs::copy(&fixture, music.join(format!("{index}.flac"))).unwrap();
    }
    let database = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
    reprise_core::queries::query_library_text_search(
        &database,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: count as i64,
        },
    )
    .unwrap()
    .rows
    .into_iter()
    .map(|track| track.id)
    .collect()
}

#[test]
fn a_snapshot_waiting_for_the_writer_commits_after_release() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, writer) = database_in(directory.path());
    let persister = QueuePersister::spawn(&database_path, Arc::clone(&writer), None).unwrap();
    let held = writer.lock().unwrap();

    persister.persist(&queue(vec![1, 2], 1)).unwrap();
    drop(held);
    persister.flush();

    assert_eq!(saved_queue(&database_path), queue(vec![1, 2], 1).snapshot());
}

#[test]
fn twenty_snapshots_coalesce_to_the_last_queue() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, writer) = database_in(directory.path());
    let persister = QueuePersister::spawn(&database_path, Arc::clone(&writer), None).unwrap();
    let held = writer.lock().unwrap();

    for id in 1..=20 {
        persister.persist(&queue(vec![id], 0)).unwrap();
    }
    drop(held);
    persister.flush();

    assert_eq!(saved_queue(&database_path), queue(vec![20], 0).snapshot());
    assert_eq!(persister.successful_commit_count(), 1);
    assert!(!directory.path().join(FILE_NAME).exists());
}

#[test]
fn a_newer_snapshot_survives_an_older_drain_removal() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, writer) = database_in(directory.path());
    let persister = QueuePersister::spawn(&database_path, Arc::clone(&writer), None).unwrap();
    let held = writer.lock().unwrap();

    persister.persist(&queue(vec![1], 0)).unwrap();
    persister.wait_until_worker_attempts(1);
    persister.persist(&queue(vec![2], 0)).unwrap();
    drop(held);
    persister.flush();

    assert_eq!(saved_queue(&database_path), queue(vec![2], 0).snapshot());
    assert!(!directory.path().join(FILE_NAME).exists());
}

#[test]
fn a_poisoned_writer_stops_the_worker_and_keeps_the_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, writer) = database_in(directory.path());
    let poisoned = Arc::clone(&writer);
    assert!(std::thread::spawn(move || {
        let _held = poisoned.lock().unwrap();
        panic!("poison the shared writer for the test");
    })
    .join()
    .is_err());
    let persister = QueuePersister::spawn(&database_path, writer, None).unwrap();

    persister.persist(&queue(vec![9], 0)).unwrap();
    persister.flush();

    assert_eq!(persister.successful_commit_count(), 0);
    assert!(directory.path().join(FILE_NAME).exists());
}

#[test]
fn a_snapshot_write_failure_does_not_prevent_session_startup() {
    let directory = tempfile::tempdir().unwrap();
    let library = library_in(directory.path());
    std::fs::create_dir(directory.path().join(".android-queue-snapshot.v1.tmp")).unwrap();

    let session = AndroidPlaybackSession::new(
        library,
        Box::new(RecordingPort {
            calls: Arc::new(Mutex::new(Vec::new())),
            bridge: Arc::new(Mutex::new(None)),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    );

    assert!(session.is_ok());
}

#[test]
fn drop_does_not_wait_for_a_held_writer_and_leaves_the_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, writer) = database_in(directory.path());
    let persister = QueuePersister::spawn(&database_path, Arc::clone(&writer), None).unwrap();
    let held = writer.lock().unwrap();
    persister.persist(&queue(vec![7], 0)).unwrap();

    let started = Instant::now();
    drop(persister);
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(500),
        "drop took {elapsed:?}"
    );
    assert!(directory.path().join(FILE_NAME).exists());
    drop(held);
}

#[test]
fn a_fresh_session_prefers_the_durable_snapshot_over_the_database() {
    let directory = tempfile::tempdir().unwrap();
    let library = library_in(directory.path());
    let ids = seed_tracks(&library, directory.path(), 2);
    {
        let database = library.writer().unwrap();
        queue_persistence::save(&database, &queue(vec![ids[0]], 0)).unwrap();
    }
    let persister =
        QueuePersister::spawn(&library.database_path, library.writer_handle(), None).unwrap();
    let held = library.writer().unwrap();
    persister.persist(&queue(vec![ids[1]], 0)).unwrap();
    drop(persister);
    drop(held);

    let session = session_for(Arc::clone(&library));

    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Paused);
    assert_eq!(snapshot.current_track_id, Some(ids[1]));
}

#[test]
fn a_damaged_snapshot_falls_back_to_the_database_queue() {
    let directory = tempfile::tempdir().unwrap();
    let library = library_in(directory.path());
    let ids = seed_tracks(&library, directory.path(), 1);
    {
        let database = library.writer().unwrap();
        queue_persistence::save(&database, &queue(vec![ids[0]], 0)).unwrap();
    }
    std::fs::write(directory.path().join(FILE_NAME), b"damaged").unwrap();

    let session = session_for(library);

    assert_eq!(session.snapshot().unwrap().current_track_id, Some(ids[0]));
}
