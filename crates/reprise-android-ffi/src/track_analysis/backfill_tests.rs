use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use reprise_core::db::Db;
use reprise_core::library::scanner::scan_folder;

use crate::track_analysis::{
    AnalysisDecodeError, AnalysisPcmSink, AndroidAnalysisOutcome, TrackAnalysisProgress,
    TrackAnalysisProgressListener, TrackPcmDecoder,
};
use crate::MusicLibrary;

struct InertSource;

impl crate::source::SafSource for InertSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, crate::source::SafSourceError> {
        Ok(None)
    }

    fn probe(
        &self,
        _uri: String,
        _follow_links: bool,
    ) -> Result<Option<crate::source::SourceFacts>, crate::source::SafSourceError> {
        Ok(None)
    }

    fn list_children(
        &self,
        _uri: String,
    ) -> Result<Vec<crate::source::SourceChild>, crate::source::SafSourceError> {
        Ok(Vec::new())
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, crate::source::SafSourceError> {
        Err(crate::source::SafSourceError::NotFound {
            detail: format!("inert source has no documents ({uri})"),
        })
    }
}

/// A library with `count` tracks, none analysed, none SAF-synced.
fn library_with_n_tracks(count: usize) -> (tempfile::TempDir, MusicLibrary, Vec<(i64, String)>) {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    for index in 0..count {
        std::fs::copy(&fixture, music.join(format!("track-{index}.flac"))).unwrap();
    }
    let database_path = directory.path().join("reprise.db");
    let db = Db::open_migrated(Some(&database_path)).unwrap();
    scan_folder(&db, &music).unwrap();
    let pending = reprise_core::db::pending_render_data_tracks(&db).unwrap();
    let ids_and_paths: Vec<(i64, String)> = pending
        .into_iter()
        .map(|track| (track.track_id, track.path))
        .collect();
    drop(db);
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    library
        .set_tree_uri("content://inert".into(), Box::new(InertSource))
        .unwrap();
    (directory, library, ids_and_paths)
}

fn valid_pcm_bytes() -> Vec<u8> {
    let sample_rate_hz = 32_000_u32;
    let mut bytes = Vec::new();
    for index in 0..sample_rate_hz {
        let phase = std::f64::consts::TAU * 440.0 * f64::from(index) / f64::from(sample_rate_hz);
        let sample = (phase.sin() * 0.5 * f64::from(i16::MAX)) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

struct ClosureDecoder<F> {
    calls: Arc<AtomicUsize>,
    run: F,
}

impl<F> ClosureDecoder<F>
where
    F: Fn(&str, &Arc<AnalysisPcmSink>) -> Result<(), AnalysisDecodeError> + Send + Sync,
{
    fn new(calls: Arc<AtomicUsize>, run: F) -> Self {
        Self { calls, run }
    }
}

impl<F> TrackPcmDecoder for ClosureDecoder<F>
where
    F: Fn(&str, &Arc<AnalysisPcmSink>) -> Result<(), AnalysisDecodeError> + Send + Sync,
{
    fn decode(
        &self,
        track_uri: String,
        sink: Arc<AnalysisPcmSink>,
        _background: bool,
    ) -> Result<(), AnalysisDecodeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.run)(&track_uri, &sink)
    }
}

struct NoopListener;

impl TrackAnalysisProgressListener for NoopListener {
    fn on_progress(&self, _progress: TrackAnalysisProgress) {}
}

type WaitState = Arc<(Mutex<Option<TrackAnalysisProgress>>, Condvar)>;

struct RecordingListener {
    state: WaitState,
}

impl TrackAnalysisProgressListener for RecordingListener {
    fn on_progress(&self, progress: TrackAnalysisProgress) {
        let (lock, condvar) = &*self.state;
        *lock.lock().unwrap() = Some(progress);
        condvar.notify_all();
    }
}

/// Waits until the backfill has published a terminal progress snapshot
/// (`done + failed == total`). `None` (nothing published yet) always keeps
/// waiting, so a listener call racing with this wait can never be missed.
fn wait_until_done(state: &WaitState) -> TrackAnalysisProgress {
    let (lock, condvar) = &**state;
    let guard = lock.lock().unwrap();
    let (guard, timed_out) = condvar
        .wait_timeout_while(guard, Duration::from_secs(10), |progress| {
            !matches!(progress, Some(progress) if progress.done + progress.failed >= progress.total)
        })
        .unwrap();
    assert!(!timed_out.timed_out(), "the backfill never finished");
    guard.expect("the wait only exits once a progress snapshot is published")
}

/// Waits for a test-controlled gate to open. Bounded: an un-timed-out wait on
/// a background worker is a hang waiting to happen the day the worker never
/// reaches the gate — a bug here must fail this test, not the whole suite.
fn wait_flag(state: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, condvar) = &**state;
    let guard = lock.lock().unwrap();
    let (_guard, timed_out) = condvar
        .wait_timeout_while(guard, Duration::from_secs(10), |set| !*set)
        .unwrap();
    assert!(!timed_out.timed_out(), "the gate was never opened");
}

fn set_flag(state: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, condvar) = &**state;
    *lock.lock().unwrap() = true;
    condvar.notify_all();
}

#[test]
fn the_backfill_drains_pending_tracks_in_id_order() {
    let (_directory, library, expected) = library_with_n_tracks(3);
    let order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let order_in_decode = Arc::clone(&order);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |uri, sink| {
            order_in_decode.lock().unwrap().push(uri.to_owned());
            assert!(sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1));
            Ok(())
        },
    )));

    let state: WaitState = Arc::new((Mutex::new(None), Condvar::new()));
    library.start_track_analysis_backfill(Box::new(RecordingListener {
        state: Arc::clone(&state),
    }));
    let progress = wait_until_done(&state);

    assert_eq!(progress.done, 3);
    assert_eq!(progress.failed, 0);
    let expected_order: Vec<String> = expected.into_iter().map(|(_, path)| path).collect();
    assert_eq!(*order.lock().unwrap(), expected_order);
}

#[test]
fn cancel_stops_at_the_next_chunk_and_leaves_the_track_pending() {
    let (_directory, library, expected) = library_with_n_tracks(2);
    let first_id = expected[0].0;
    let started: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let release: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let started_in_decode = Arc::clone(&started);
    let release_in_decode = Arc::clone(&release);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            set_flag(&started_in_decode);
            wait_flag(&release_in_decode);
            loop {
                if !sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1) {
                    break;
                }
            }
            Ok(())
        },
    )));

    library.start_track_analysis_backfill(Box::new(NoopListener));
    wait_flag(&started);

    let release_clone = Arc::clone(&release);
    let releaser = std::thread::spawn(move || {
        set_flag(&release_clone);
    });
    library.cancel_track_analysis_backfill();
    releaser.join().unwrap();

    let reader = library.reader().unwrap();
    assert!(
        reprise_core::db::get_track_spectrogram(&reader, first_id)
            .unwrap()
            .is_none(),
        "the track mid-decode when cancelled must stay pending"
    );
}

#[test]
fn a_failed_track_is_skipped_for_the_rest_of_the_process() {
    let (_directory, library, expected) = library_with_n_tracks(3);
    let bad_uri = expected[0].1.clone();
    let attempts_on_bad = Arc::new(AtomicUsize::new(0));
    let attempts_on_bad_in_decode = Arc::clone(&attempts_on_bad);
    let bad_uri_in_decode = bad_uri.clone();
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |uri, sink| {
            if uri == bad_uri_in_decode {
                attempts_on_bad_in_decode.fetch_add(1, Ordering::SeqCst);
                return Err(AnalysisDecodeError::DecodeFailed {
                    detail: "boom".into(),
                });
            }
            assert!(sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1));
            Ok(())
        },
    )));

    let state: WaitState = Arc::new((Mutex::new(None), Condvar::new()));
    library.start_track_analysis_backfill(Box::new(RecordingListener {
        state: Arc::clone(&state),
    }));
    let progress = wait_until_done(&state);

    assert_eq!(progress.done, 2);
    assert_eq!(progress.failed, 1);
    assert_eq!(
        attempts_on_bad.load(Ordering::SeqCst),
        1,
        "a failed track must not be retried within the same process"
    );
}

#[test]
fn a_foreground_request_preempts_the_worker() {
    let (_directory, library, expected) = library_with_n_tracks(2);
    let library = Arc::new(library);
    let a_uri = expected[0].1.clone();
    let a_id = expected[0].0;
    let b_id = expected[1].0;
    let started: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let release: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let a_blocked_once = Arc::new(AtomicBool::new(false));
    let calls_on_a = Arc::new(AtomicUsize::new(0));

    let a_uri_in_decode = a_uri.clone();
    let started_in_decode = Arc::clone(&started);
    let release_in_decode = Arc::clone(&release);
    let a_blocked_once_in_decode = Arc::clone(&a_blocked_once);
    let calls_on_a_in_decode = Arc::clone(&calls_on_a);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |uri, sink| {
            if uri == a_uri_in_decode {
                calls_on_a_in_decode.fetch_add(1, Ordering::SeqCst);
                if !a_blocked_once_in_decode.swap(true, Ordering::SeqCst) {
                    set_flag(&started_in_decode);
                    wait_flag(&release_in_decode);
                    loop {
                        if !sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1) {
                            break;
                        }
                    }
                    return Ok(());
                }
            }
            assert!(sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1));
            Ok(())
        },
    )));

    let state: WaitState = Arc::new((Mutex::new(None), Condvar::new()));
    library.start_track_analysis_backfill(Box::new(RecordingListener {
        state: Arc::clone(&state),
    }));
    wait_flag(&started);

    // The foreground request finishes first, while the backfill is still
    // blocked mid-decode on the other track.
    let foreground_outcome = library.import_track_analysis(b_id).unwrap();
    assert_eq!(foreground_outcome, AndroidAnalysisOutcome::Computed);

    set_flag(&release);

    let progress = wait_until_done(&state);

    // The backfill's own counters credit only the work it did itself: `b_id`
    // was resolved by the foreground request and simply vanished from its
    // worklist, not something the worker ever finished.
    assert_eq!(progress.done, 1);
    assert_eq!(progress.failed, 0);
    assert!(
        calls_on_a.load(Ordering::SeqCst) >= 2,
        "the preempted track must be decoded again afterwards"
    );
    let reader = library.reader().unwrap();
    assert!(
        reprise_core::db::get_track_spectrogram(&reader, b_id)
            .unwrap()
            .is_some(),
        "the foreground request's own track must still be stored"
    );
    assert!(reprise_core::db::get_track_spectrogram(&reader, a_id)
        .unwrap()
        .is_some());
}

/// Polls `library`'s backfill worker handle until the thread has actually
/// finished (including a panicking unwind), the same way the sibling
/// `PortraitBackfill` test suite does for the same class of race.
fn wait_for_worker_to_finish(library: &MusicLibrary) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let finished = library
            .analysis_backfill
            .worker
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished);
        if finished {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the backfill worker never finished"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits for the first published progress snapshot, with no terminal-state
/// predicate: unlike [`wait_until_done`], this only proves the worker loop
/// ran and reached its first `publish` call at least once, which is exactly
/// what a permanently-disabled backfill (the bug this pins) would never do.
fn wait_for_any_progress(state: &WaitState) -> TrackAnalysisProgress {
    let (lock, condvar) = &**state;
    let guard = lock.lock().unwrap();
    let (guard, timed_out) = condvar
        .wait_timeout_while(guard, Duration::from_secs(10), |progress| {
            progress.is_none()
        })
        .unwrap();
    assert!(
        !timed_out.timed_out(),
        "the backfill worker never restarted after the panic"
    );
    guard.expect("the wait only exits once a progress snapshot is published")
}

#[test]
fn a_panicking_worker_does_not_permanently_disable_the_backfill() {
    let (_directory, library, _expected) = library_with_n_tracks(1);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        |_uri, _sink| -> Result<(), AnalysisDecodeError> { panic!("intentional decode panic") },
    )));

    library.start_track_analysis_backfill(Box::new(NoopListener));
    wait_for_worker_to_finish(&library);

    // Before the fix, a panic inside `compute()` left `shared.active`
    // permanently `true`: this `start()` would join the already-finished
    // thread, see `active` still set, and silently no-op forever — no new
    // worker thread would ever spawn, and no further progress would ever be
    // published.
    let state: WaitState = Arc::new((Mutex::new(None), Condvar::new()));
    library.start_track_analysis_backfill(Box::new(RecordingListener {
        state: Arc::clone(&state),
    }));
    wait_for_any_progress(&state);
}

/// `PhoneSourceChanged` stores nothing (`set_track_render_data` found the
/// file's fingerprint had changed mid-decode) and the track stays pending,
/// so it must not be credited as `done` the way a real `Computed`/
/// `AlreadyImported` outcome is — otherwise `done` climbs past the number of
/// tracks the backfill ever actually finished.
#[test]
fn a_phone_source_change_is_not_counted_as_done() {
    let (directory, library, expected) = library_with_n_tracks(1);
    let music = directory.path().join("music");
    let track_id = expected[0].0;
    let changed_once = Arc::new(AtomicBool::new(false));
    let changed_once_in_decode = Arc::clone(&changed_once);
    let writer = library.writer_handle();
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            if !changed_once_in_decode.swap(true, Ordering::SeqCst) {
                let song_path = music.join("track-0.flac");
                let file = File::open(&song_path).unwrap();
                file.set_modified(
                    std::time::SystemTime::now() + std::time::Duration::from_secs(120),
                )
                .unwrap();
                drop(file);
                let db = writer.lock().unwrap();
                scan_folder(&db, &music).unwrap();
                drop(db);
            }
            assert!(sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1));
            Ok(())
        },
    )));

    let state: WaitState = Arc::new((Mutex::new(None), Condvar::new()));
    library.start_track_analysis_backfill(Box::new(RecordingListener {
        state: Arc::clone(&state),
    }));
    let progress = wait_until_done(&state);

    assert_eq!(
        progress.done, 1,
        "only the eventual real store may count as done, not the earlier source change"
    );
    assert_eq!(progress.failed, 0);
    let reader = library.reader().unwrap();
    assert!(reprise_core::db::get_track_spectrogram(&reader, track_id)
        .unwrap()
        .is_some());
}

#[test]
fn start_while_running_is_a_no_op() {
    let (_directory, library, _expected) = library_with_n_tracks(2);
    let started: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let release: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let started_in_decode = Arc::clone(&started);
    let release_in_decode = Arc::clone(&release);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::clone(&calls),
        move |_uri, sink| {
            set_flag(&started_in_decode);
            wait_flag(&release_in_decode);
            assert!(sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1));
            Ok(())
        },
    )));

    library.start_track_analysis_backfill(Box::new(NoopListener));
    wait_flag(&started);

    library.start_track_analysis_backfill(Box::new(NoopListener));

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a second start must not begin a second decode"
    );

    set_flag(&release);
    library.cancel_track_analysis_backfill();
}
