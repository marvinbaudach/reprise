use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use reprise_core::db::Db;

use super::{PlayRecorder, RecordedPlay};
use crate::log_capture::CapturedLogs;
use crate::play_journal::FILE_NAME as JOURNAL_FILE_NAME;
use crate::play_recorder_retry::{retry_after, BUSY_ATTEMPTS};

const RETRY_TEST_MARGIN: Duration = Duration::from_secs(1);

fn seeded_database(directory: &Path) -> (PathBuf, i64) {
    let music = directory.join("music");
    std::fs::create_dir(&music).unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(source, music.join("sine.flac")).unwrap();
    let database_path = directory.join("reprise.db");
    let database = Db::open_migrated(Some(&database_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
    let track_id = reprise_core::queries::query_library_text_search(
        &database,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows[0]
        .id;
    (database_path, track_id)
}

fn shared_writer(database_path: &Path) -> Arc<Mutex<Db>> {
    Arc::new(Mutex::new(Db::open_ready(database_path).unwrap()))
}

fn captured_recorder(
    database_path: PathBuf,
    writer: Arc<Mutex<Db>>,
    logs: CapturedLogs,
) -> PlayRecorder {
    let (plays, queued) = mpsc::channel();
    let shutting_down = Arc::new(AtomicBool::new(false));
    let worker_flag = Arc::clone(&shutting_down);
    let worker = std::thread::spawn(move || {
        logs.capture(|| {
            super::write_queued_plays(&database_path, writer.as_ref(), 0, queued, &worker_flag);
        });
    });
    PlayRecorder {
        plays: Some(plays),
        shutting_down,
        worker: Some(worker),
    }
}

fn play_count(database_path: &Path, track_id: i64) -> i64 {
    let database = Db::open_ready(database_path).unwrap();
    reprise_core::queries::query_present_track_by_id(&database, track_id)
        .unwrap()
        .unwrap()
        .play_count
}

fn wait_for(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while !condition() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

fn wait_until(timeout: Duration, condition: impl FnMut() -> bool) {
    assert!(
        wait_for(timeout, condition),
        "condition was not met before {timeout:?}",
    );
}

fn busy_retry_schedule_with_margin() -> Duration {
    (1..=BUSY_ATTEMPTS)
        .filter_map(|attempt| retry_after(true, attempt))
        .sum::<Duration>()
        + RETRY_TEST_MARGIN
}

#[test]
fn drop_while_the_writer_is_held_returns_and_keeps_the_journaled_play() {
    let held_directory = tempfile::tempdir().unwrap();
    let (held_database_path, held_track_id) = seeded_database(held_directory.path());
    let held_writer = shared_writer(&held_database_path);
    let writer_guard = held_writer.lock().unwrap();
    let logs = CapturedLogs::default();
    let held_recorder = captured_recorder(
        held_database_path.clone(),
        Arc::clone(&held_writer),
        logs.clone(),
    );
    held_recorder.record(RecordedPlay {
        track_id: held_track_id,
        at_unix: 1_700_000_000,
    });
    let held_journal = held_directory.path().join(JOURNAL_FILE_NAME);
    wait_until(Duration::from_secs(3), || {
        std::fs::read(&held_journal).is_ok_and(|contents| !contents.is_empty())
    });
    wait_until(Duration::from_secs(3), || {
        let logged = logs.joined();
        logged.contains("offering an Android play count again") && logged.contains("attempt=3")
    });

    let started = Instant::now();
    let (dropped, wait_for_drop) = mpsc::channel();
    let drop_worker = std::thread::spawn(move || {
        drop(held_recorder);
        dropped.send(()).unwrap();
    });
    let drop_result = wait_for_drop.recv_timeout(Duration::from_millis(500));

    assert!(
        drop_result.is_ok(),
        "dropping the recorder waited {:?} for the held writer",
        started.elapsed(),
    );
    assert!(
        !std::fs::read(&held_journal).unwrap().is_empty(),
        "the play must remain durable when shutdown cannot reach the writer",
    );
    drop(writer_guard);
    drop_worker.join().unwrap();

    let free_directory = tempfile::tempdir().unwrap();
    let (free_database_path, free_track_id) = seeded_database(free_directory.path());
    let free_recorder = PlayRecorder::spawn(
        free_database_path.clone(),
        shared_writer(&free_database_path),
        0,
    );
    free_recorder.record(RecordedPlay {
        track_id: free_track_id,
        at_unix: 1_700_000_000,
    });
    drop(free_recorder);

    assert_eq!(
        std::fs::read(free_directory.path().join(JOURNAL_FILE_NAME)).unwrap(),
        b"",
        "the control arm must commit and remove its journal entry",
    );
    assert_eq!(play_count(&free_database_path, free_track_id), 1);
}

#[test]
fn journaled_play_retries_after_the_writer_is_released_without_another_play() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, track_id) = seeded_database(directory.path());
    let writer = shared_writer(&database_path);
    let writer_guard = writer.lock().unwrap();
    let logs = CapturedLogs::default();
    let recorder = captured_recorder(database_path.clone(), Arc::clone(&writer), logs.clone());
    recorder.record(RecordedPlay {
        track_id,
        at_unix: 1_700_000_000,
    });
    let journal = directory.path().join(JOURNAL_FILE_NAME);
    wait_until(Duration::from_secs(3), || {
        std::fs::read(&journal).is_ok_and(|contents| !contents.is_empty())
    });
    let gave_up = wait_for(Duration::from_secs(3), || {
        logs.joined().contains("the shared writer was busy")
    });

    drop(writer_guard);

    assert!(gave_up, "the first bounded retry round did not give up");
    wait_until(Duration::from_secs(3), || {
        play_count(&database_path, track_id) == 1
    });
    drop(recorder);
    assert_eq!(play_count(&database_path, track_id), 1);
}

#[test]
fn non_retryable_journal_failure_does_not_keep_waking_the_worker() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, track_id) = seeded_database(directory.path());
    let writer = shared_writer(&database_path);
    let poison_writer = Arc::clone(&writer);
    let poisoner = std::thread::spawn(move || {
        let _guard = poison_writer.lock().unwrap();
        panic!("poison the shared writer for the test");
    });
    assert!(poisoner.join().is_err());

    let logs = CapturedLogs::default();
    let recorder = captured_recorder(database_path, writer, logs.clone());
    recorder.record(RecordedPlay {
        track_id,
        at_unix: 1_700_000_000,
    });
    let warning = "kept an Android play count in its journal: the shared writer was poisoned";
    wait_until(Duration::from_millis(500), || {
        logs.joined().contains(warning)
    });

    std::thread::sleep(Duration::from_millis(2_500));

    assert_eq!(
        logs.joined().matches(warning).count(),
        1,
        "a failure that retrying cannot fix must wait for another play",
    );
    drop(recorder);
}

#[test]
fn unjournaled_play_gives_up_and_shutdown_does_not_wait_for_the_writer() {
    let directory = tempfile::tempdir().unwrap();
    let (database_path, track_id) = seeded_database(directory.path());
    std::fs::write(
        directory.path().join(JOURNAL_FILE_NAME),
        format!("v2\t1\t{track_id}\t1700000000\n"),
    )
    .unwrap();
    let writer = shared_writer(&database_path);
    let writer_guard = writer.lock().unwrap();
    let logs = CapturedLogs::default();
    let recorder = captured_recorder(database_path.clone(), Arc::clone(&writer), logs.clone());
    recorder.record(RecordedPlay {
        track_id,
        at_unix: 1_700_000_000,
    });
    let busy_warning =
        "dropped an Android play count: no journal was open and the library writer stayed busy";
    let warned_before_shutdown = wait_for(busy_retry_schedule_with_margin(), || {
        logs.joined().contains(busy_warning)
    });
    let (dropped, wait_for_drop) = mpsc::channel();
    let drop_worker = std::thread::spawn(move || {
        drop(recorder);
        dropped.send(()).unwrap();
    });

    let drop_result = wait_for_drop.recv_timeout(Duration::from_millis(500));
    drop(writer_guard);
    drop_worker.join().unwrap();

    assert!(
        drop_result.is_ok(),
        "shutdown waited for the writer after the unjournaled retry budget",
    );
    assert!(
        warned_before_shutdown,
        "the unjournaled play did not give up after its bounded retry schedule",
    );
    assert_eq!(
        play_count(&database_path, track_id),
        0,
        "degraded mode must give up rather than wait indefinitely",
    );
    let logged = logs.joined();
    assert!(
        logged.contains(busy_warning),
        "the degraded loss must be explicit, got {logged}",
    );
    assert!(
        logged.contains(&format!("track_id={track_id}")),
        "the warning must name the affected track, got {logged}",
    );
}
