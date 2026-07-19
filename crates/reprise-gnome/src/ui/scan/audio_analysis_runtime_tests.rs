use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reprise_core::audio_analysis::{
    AnalysisOutput, AudioAnalysisBackend, AudioAnalysisError, AudioEvidenceAccumulator,
};
use reprise_core::sound_profile::{self, TrackAnalysis};
use reprise_core::waveform::{WaveformBackend, WaveformError};

use super::*;

struct FakeAnalysisBackend {
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    block_next: AtomicBool,
    release: AtomicBool,
    fail: AtomicBool,
    ignore_cancel: AtomicBool,
}

impl FakeAnalysisBackend {
    fn ready() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            block_next: AtomicBool::new(false),
            release: AtomicBool::new(false),
            fail: AtomicBool::new(false),
            ignore_cancel: AtomicBool::new(false),
        }
    }

    fn blocking() -> Self {
        let backend = Self::ready();
        backend.block_next.store(true, Ordering::Release);
        backend
    }
}

struct ActiveCall<'a>(&'a AtomicUsize);

impl Drop for ActiveCall<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl AudioAnalysisBackend for FakeAnalysisBackend {
    fn analyze(
        &self,
        _path: &Path,
        cancelled: &AtomicBool,
    ) -> Result<AnalysisOutput, AudioAnalysisError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_active.fetch_max(active, Ordering::AcqRel);
        let _active = ActiveCall(&self.active);
        if self.block_next.swap(false, Ordering::AcqRel) {
            while !self.release.load(Ordering::Acquire) {
                if cancelled.load(Ordering::Acquire) && !self.ignore_cancel.load(Ordering::Acquire)
                {
                    return Err(AudioAnalysisError::Cancelled);
                }
                std::thread::yield_now();
            }
        }
        if self.fail.load(Ordering::Acquire) {
            return Err(AudioAnalysisError::DecodeFailed("fixture failure".into()));
        }
        Ok(output())
    }
}

#[derive(Default)]
struct FakeWaveformBackend {
    calls: AtomicUsize,
}

impl WaveformBackend for FakeWaveformBackend {
    fn extract_peaks(&self, _path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Ok(vec![42; buckets])
    }
}

fn output() -> AnalysisOutput {
    let samples = (0..800)
        .map(|index| (std::f32::consts::TAU * 440.0 * index as f32 / 8_000.0).sin() * 0.5)
        .collect::<Vec<_>>();
    let mut accumulator = AudioEvidenceAccumulator::new(8_000, 800, 1_000).unwrap();
    accumulator.push(&samples).unwrap();
    accumulator.finish().unwrap()
}

fn database(track_count: i64) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("analysis.db");
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    for id in 1..=track_count {
        conn.execute(
            "INSERT INTO tracks
               (id, path, title, artist, added_at, file_mtime, file_size)
             VALUES (?1, ?2, 'Fixture', 'Artist', 1, 20, 30)",
            rusqlite::params![id, format!("/fixture-{id}.flac")],
        )
        .unwrap();
    }
    drop(conn);
    (directory, path)
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for worker");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn analysis_count(path: &Path) -> i64 {
    let conn = reprise_core::db::open_migrated(Some(path)).unwrap();
    conn.query_row("SELECT COUNT(*) FROM track_audio_analysis", [], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn disabled_runtime_backfills_waveforms_without_character_work() {
    let (_directory, db_path) = database(1);
    let analysis = Arc::new(FakeAnalysisBackend::ready());
    let waveform = Arc::new(FakeWaveformBackend::default());
    let runtime =
        AudioAnalysisRuntime::new(db_path.clone(), analysis.clone(), waveform.clone(), false)
            .unwrap();

    wait_until(|| waveform.calls.load(Ordering::Acquire) == 1);

    assert_eq!(analysis.calls.load(Ordering::Acquire), 0);
    assert_eq!(analysis_count(&db_path), 0);
    let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
    assert!(reprise_core::db::get_waveform_peaks(&conn, 1)
        .unwrap()
        .is_some());
    drop(conn);
    runtime.set_enabled(true);
    wait_until(|| analysis_count(&db_path) == 1);
    assert_eq!(analysis.calls.load(Ordering::Acquire), 1);
    runtime.shutdown();
}

#[test]
fn enabled_runtime_uses_one_coordinated_decode_and_one_worker() {
    let (_directory, db_path) = database(2);
    let analysis = Arc::new(FakeAnalysisBackend::ready());
    let waveform = Arc::new(FakeWaveformBackend::default());
    let runtime =
        AudioAnalysisRuntime::new(db_path.clone(), analysis.clone(), waveform.clone(), true)
            .unwrap();

    wait_until(|| analysis_count(&db_path) == 2);

    assert_eq!(analysis.calls.load(Ordering::Acquire), 2);
    assert_eq!(analysis.max_active.load(Ordering::Acquire), 1);
    assert_eq!(waveform.calls.load(Ordering::Acquire), 0);
    assert!(!runtime.start().unwrap(), "a second worker was started");
    runtime.shutdown();
}

#[test]
fn pause_cancels_the_current_decode_and_resume_continues() {
    let (_directory, db_path) = database(2);
    let analysis = Arc::new(FakeAnalysisBackend::blocking());
    let runtime = AudioAnalysisRuntime::new(
        db_path.clone(),
        analysis.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();
    wait_until(|| analysis.calls.load(Ordering::Acquire) == 1);

    runtime.pause();
    wait_until(|| analysis.active.load(Ordering::Acquire) == 0);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(analysis.calls.load(Ordering::Acquire), 1);
    assert_eq!(analysis_count(&db_path), 0);

    analysis.release.store(true, Ordering::Release);
    runtime.resume();
    wait_until(|| analysis_count(&db_path) == 2);
    assert_eq!(analysis.calls.load(Ordering::Acquire), 3);
    runtime.shutdown();
}

#[test]
fn deactivation_cancels_new_work_and_retains_finished_profiles() {
    let (_directory, db_path) = database(1);
    let analysis = Arc::new(FakeAnalysisBackend::ready());
    let runtime = AudioAnalysisRuntime::new(
        db_path.clone(),
        analysis.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();
    wait_until(|| analysis_count(&db_path) == 1);

    let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (2, '/new.flac', 'New', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    drop(conn);
    analysis.block_next.store(true, Ordering::Release);
    runtime.wake();
    wait_until(|| analysis.calls.load(Ordering::Acquire) == 2);

    runtime.set_enabled(false);
    wait_until(|| analysis.active.load(Ordering::Acquire) == 0);
    std::thread::sleep(Duration::from_millis(30));

    assert_eq!(analysis_count(&db_path), 1);
    let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
    assert!(matches!(
        sound_profile::load_analysis(&conn, 1).unwrap(),
        Some(TrackAnalysis::Ready(_))
    ));
    assert!(sound_profile::load_analysis(&conn, 2).unwrap().is_none());
    runtime.shutdown();
}

#[test]
fn confirmed_reanalysis_waits_for_in_flight_decode_then_rebuilds_every_profile() {
    let (_directory, db_path) = database(2);
    let analysis = Arc::new(FakeAnalysisBackend::blocking());
    let runtime = AudioAnalysisRuntime::new(
        db_path.clone(),
        analysis.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();
    wait_until(|| analysis.calls.load(Ordering::Acquire) == 1);

    assert_eq!(runtime.reanalyze().unwrap(), 0);
    wait_until(|| analysis_count(&db_path) == 2);

    assert_eq!(analysis.calls.load(Ordering::Acquire), 3);
    runtime.shutdown();
}

#[test]
fn cancellation_and_fingerprint_changes_never_publish_partial_ready_results() {
    let (_directory, cancel_db) = database(1);
    let cancelled_backend = Arc::new(FakeAnalysisBackend::blocking());
    cancelled_backend
        .ignore_cancel
        .store(true, Ordering::Release);
    let cancelled_runtime = AudioAnalysisRuntime::new(
        cancel_db.clone(),
        cancelled_backend.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();
    wait_until(|| cancelled_backend.calls.load(Ordering::Acquire) == 1);
    cancelled_runtime.cancel();
    cancelled_backend.release.store(true, Ordering::Release);
    wait_until(|| cancelled_backend.active.load(Ordering::Acquire) == 0);
    wait_until(|| cancelled_runtime.progress().activity == AnalysisActivity::Cancelled);
    assert_eq!(analysis_count(&cancel_db), 0);
    cancelled_runtime.shutdown();

    let (_directory, changed_db) = database(1);
    let changed_backend = Arc::new(FakeAnalysisBackend::blocking());
    let changed_runtime = AudioAnalysisRuntime::new(
        changed_db.clone(),
        changed_backend.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();
    wait_until(|| changed_backend.calls.load(Ordering::Acquire) == 1);
    let conn = reprise_core::db::open_migrated(Some(&changed_db)).unwrap();
    conn.execute("UPDATE tracks SET file_mtime = 21 WHERE id = 1", [])
        .unwrap();
    drop(conn);
    changed_runtime.pause();
    changed_backend.release.store(true, Ordering::Release);
    wait_until(|| changed_backend.active.load(Ordering::Acquire) == 0);
    assert_eq!(analysis_count(&changed_db), 0);
    let conn = reprise_core::db::open_migrated(Some(&changed_db)).unwrap();
    assert!(reprise_core::db::get_waveform_peaks(&conn, 1)
        .unwrap()
        .is_none());
    changed_runtime.shutdown();
}

#[test]
fn failures_wait_for_explicit_retry_and_scan_wake_adds_new_work() {
    let (_directory, db_path) = database(1);
    let analysis = Arc::new(FakeAnalysisBackend::ready());
    analysis.fail.store(true, Ordering::Release);
    let runtime = AudioAnalysisRuntime::new(
        db_path.clone(),
        analysis.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();
    wait_until(|| analysis_count(&db_path) == 1);
    wait_until(|| runtime.progress().activity == AnalysisActivity::Failed);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(analysis.calls.load(Ordering::Acquire), 1);
    let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
    assert!(matches!(
        sound_profile::load_analysis(&conn, 1).unwrap(),
        Some(TrackAnalysis::Failed(_))
    ));
    drop(conn);

    analysis.fail.store(false, Ordering::Release);
    assert_eq!(runtime.retry_failed().unwrap(), 1);
    wait_until(|| {
        let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
        matches!(
            sound_profile::load_analysis(&conn, 1).unwrap(),
            Some(TrackAnalysis::Ready(_))
        )
    });

    let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (2, '/scan-added.flac', 'Added', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    drop(conn);
    runtime.wake();
    wait_until(|| analysis_count(&db_path) == 2);
    runtime.shutdown();
}

#[test]
fn progress_channel_coalesces_to_the_latest_state() {
    let (_directory, db_path) = database(0);
    let runtime = AudioAnalysisRuntime::new(
        db_path,
        Arc::new(FakeAnalysisBackend::ready()),
        Arc::new(FakeWaveformBackend::default()),
        false,
    )
    .unwrap();
    let receiver = runtime.progress_receiver();

    runtime.pause();
    runtime.resume();
    runtime.cancel();

    assert_eq!(
        receiver.try_recv().unwrap().activity,
        AnalysisActivity::Cancelled
    );
    assert!(receiver.is_empty());
    runtime.shutdown();
}

#[test]
fn progress_is_broadcast_to_each_ui_surface() {
    let (_directory, db_path) = database(0);
    let runtime = AudioAnalysisRuntime::new(
        db_path,
        Arc::new(FakeAnalysisBackend::ready()),
        Arc::new(FakeWaveformBackend::default()),
        false,
    )
    .unwrap();
    let first = runtime.progress_receiver();
    let second = runtime.progress_receiver();
    let _ = first.try_recv();
    let _ = second.try_recv();

    runtime.cancel();

    assert_eq!(
        first.try_recv().unwrap().activity,
        AnalysisActivity::Cancelled
    );
    assert_eq!(
        second.try_recv().unwrap().activity,
        AnalysisActivity::Cancelled
    );
    runtime.shutdown();
}

#[test]
fn profile_version_changes_reproject_stored_evidence_without_pcm() {
    let (_directory, db_path) = database(1);
    let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
    conn.execute("UPDATE tracks SET waveform_peaks = X'01' WHERE id = 1", [])
        .unwrap();
    let analyzed = output();
    let future_versions = sound_profile::AnalysisVersions::new(
        reprise_core::audio_analysis::CURRENT_EXTRACTOR_VERSION,
        sound_profile::CURRENT_PROFILE_VERSION + 1,
    )
    .unwrap();
    let stored = sound_profile::ReadyAnalysis::new(
        sound_profile::SourceFingerprint::new(20, 30).unwrap(),
        future_versions,
        10,
        analyzed.evidence,
        analyzed.profile,
    )
    .unwrap();
    sound_profile::save_ready_analysis(&conn, 1, &stored).unwrap();
    drop(conn);
    let backend = Arc::new(FakeAnalysisBackend::ready());
    let runtime = AudioAnalysisRuntime::new(
        db_path.clone(),
        backend.clone(),
        Arc::new(FakeWaveformBackend::default()),
        true,
    )
    .unwrap();

    wait_until(|| {
        let conn = reprise_core::db::open_migrated(Some(&db_path)).unwrap();
        let Some(TrackAnalysis::Ready(ready)) = sound_profile::load_analysis(&conn, 1).unwrap()
        else {
            return false;
        };
        ready.versions.profile() == sound_profile::CURRENT_PROFILE_VERSION
    });

    assert_eq!(backend.calls.load(Ordering::Acquire), 0);
    runtime.shutdown();
}
