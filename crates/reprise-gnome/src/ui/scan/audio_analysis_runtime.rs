//! Persistent single-worker scheduling for waveform and audio-character work.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use reprise_core::audio_analysis::{
    pending_waveform_work, project_profile, reset_failed_analyses, save_waveform_if_current,
    AnalysisOutput, AudioAnalysisBackend, AudioAnalysisError,
};
use reprise_core::sound_profile::{
    self, AnalysisVersions, FailedAnalysis, FailureKind, PendingTrack, PendingWork, ReadyAnalysis,
    TrackAnalysis, CURRENT_PROFILE_VERSION,
};
use reprise_core::waveform::{WaveformBackend, STORED_PEAK_COUNT};

const EXTRACTOR_VERSION: u32 = reprise_core::audio_analysis::CURRENT_EXTRACTOR_VERSION;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkCapability {
    WaveformOnly,
    CharacterAndWaveform,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum AnalysisActivity {
    Disabled,
    Idle,
    Running { track_id: i64 },
    Paused,
    Cancelled,
    Failed,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct AnalysisProgress {
    pub activity: AnalysisActivity,
    pub analyzed: u64,
    pub total: u64,
    pub failed: u64,
}

#[derive(Debug)]
struct WorkerState {
    enabled: bool,
    paused: bool,
    cancelled: bool,
    shutdown: bool,
    revision: u64,
    progress: AnalysisProgress,
}

impl WorkerState {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            paused: false,
            cancelled: false,
            shutdown: false,
            revision: 1,
            progress: AnalysisProgress {
                activity: if enabled {
                    AnalysisActivity::Idle
                } else {
                    AnalysisActivity::Disabled
                },
                analyzed: 0,
                total: 0,
                failed: 0,
            },
        }
    }

    fn next_capability(&self) -> Option<WorkCapability> {
        if self.shutdown || self.paused || self.cancelled {
            None
        } else if self.enabled {
            Some(WorkCapability::CharacterAndWaveform)
        } else {
            Some(WorkCapability::WaveformOnly)
        }
    }
}

struct SharedState {
    state: Mutex<WorkerState>,
    changed: Condvar,
    decode_cancelled: AtomicBool,
    progress_sender: async_channel::Sender<AnalysisProgress>,
    stale_progress: async_channel::Receiver<AnalysisProgress>,
}

struct RuntimeInner {
    db_path: PathBuf,
    analysis_backend: Arc<dyn AudioAnalysisBackend>,
    waveform_backend: Arc<dyn WaveformBackend>,
    shared: Arc<SharedState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub(in crate::ui) struct AudioAnalysisRuntime {
    inner: Arc<RuntimeInner>,
}

impl AudioAnalysisRuntime {
    pub(in crate::ui) fn new(
        db_path: PathBuf,
        analysis_backend: Arc<dyn AudioAnalysisBackend>,
        waveform_backend: Arc<dyn WaveformBackend>,
        enabled: bool,
    ) -> Result<Self, std::io::Error> {
        let (progress_sender, progress_receiver) = async_channel::bounded(1);
        let inner = Arc::new(RuntimeInner {
            db_path,
            analysis_backend,
            waveform_backend,
            shared: Arc::new(SharedState {
                state: Mutex::new(WorkerState::new(enabled)),
                changed: Condvar::new(),
                decode_cancelled: AtomicBool::new(false),
                progress_sender,
                stale_progress: progress_receiver.clone(),
            }),
            worker: Mutex::new(None),
        });
        let runtime = Self { inner };
        runtime.start()?;
        Ok(runtime)
    }

    fn start(&self) -> Result<bool, std::io::Error> {
        let mut worker = lock(&self.inner.worker);
        if worker.is_some() {
            return Ok(false);
        }
        let db_path = self.inner.db_path.clone();
        let analysis_backend = self.inner.analysis_backend.clone();
        let waveform_backend = self.inner.waveform_backend.clone();
        let shared = self.inner.shared.clone();
        *worker = Some(
            std::thread::Builder::new()
                .name("reprise-audio-analysis".into())
                .spawn(move || {
                    worker_loop(
                        &db_path,
                        analysis_backend.as_ref(),
                        waveform_backend.as_ref(),
                        &shared,
                    );
                })?,
        );
        Ok(true)
    }

    #[allow(dead_code)] // Wired by the next Stage 1A settings task.
    pub(in crate::ui) fn set_enabled(&self, enabled: bool) {
        self.update_state(|state| {
            state.enabled = enabled;
            state.cancelled = false;
            state.progress.activity = if enabled {
                AnalysisActivity::Idle
            } else {
                AnalysisActivity::Disabled
            };
        });
    }

    #[allow(dead_code)] // Wired by the next Stage 1A settings task.
    pub(in crate::ui) fn pause(&self) {
        self.update_state(|state| {
            state.paused = true;
            state.progress.activity = AnalysisActivity::Paused;
        });
    }

    #[allow(dead_code)] // Wired by the next Stage 1A settings task.
    pub(in crate::ui) fn resume(&self) {
        self.inner
            .shared
            .decode_cancelled
            .store(false, Ordering::Release);
        self.update_state(|state| {
            state.paused = false;
            state.cancelled = false;
            state.progress.activity = if state.enabled {
                AnalysisActivity::Idle
            } else {
                AnalysisActivity::Disabled
            };
        });
    }

    #[allow(dead_code)] // Wired by the next Stage 1A settings task.
    pub(in crate::ui) fn cancel(&self) {
        self.inner
            .shared
            .decode_cancelled
            .store(true, Ordering::Release);
        self.update_state(|state| {
            state.cancelled = true;
            state.progress.activity = AnalysisActivity::Cancelled;
        });
    }

    pub(in crate::ui) fn wake(&self) {
        self.update_state(|_| {});
    }

    #[allow(dead_code)] // Wired by the next Stage 1A settings task.
    pub(in crate::ui) fn retry_failed(&self) -> Result<u64, String> {
        let conn = reprise_core::db::open_migrated(Some(&self.inner.db_path))
            .map_err(|error| error.to_string())?;
        let reset = reset_failed_analyses(&conn).map_err(|error| error.to_string())?;
        self.wake();
        Ok(reset)
    }

    #[allow(dead_code)] // Wired by the next Stage 1A settings task.
    pub(in crate::ui) fn progress(&self) -> AnalysisProgress {
        lock(&self.inner.shared.state).progress
    }

    pub(in crate::ui) fn progress_receiver(&self) -> async_channel::Receiver<AnalysisProgress> {
        self.inner.shared.stale_progress.clone()
    }

    pub(in crate::ui) fn shutdown(&self) {
        self.inner
            .shared
            .decode_cancelled
            .store(true, Ordering::Release);
        {
            let mut state = lock(&self.inner.shared.state);
            state.shutdown = true;
            state.revision = state.revision.wrapping_add(1);
            self.inner.shared.changed.notify_all();
        }
        if let Some(worker) = lock(&self.inner.worker).take() {
            let _ = worker.join();
        }
    }

    fn update_state(&self, update: impl FnOnce(&mut WorkerState)) {
        let progress = {
            let mut state = lock(&self.inner.shared.state);
            update(&mut state);
            state.revision = state.revision.wrapping_add(1);
            self.inner.shared.changed.notify_all();
            state.progress
        };
        publish_progress(&self.inner.shared, progress);
    }
}

impl Drop for RuntimeInner {
    fn drop(&mut self) {
        self.shared.decode_cancelled.store(true, Ordering::Release);
        {
            let mut state = lock(&self.shared.state);
            state.shutdown = true;
            self.shared.changed.notify_all();
        }
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    db_path: &std::path::Path,
    analysis_backend: &dyn AudioAnalysisBackend,
    waveform_backend: &dyn WaveformBackend,
    shared: &Arc<SharedState>,
) {
    let mut handled_revision = 0;
    loop {
        let Some((capability, revision)) = wait_for_work(shared, handled_revision) else {
            return;
        };
        handled_revision = revision;
        let result = match capability {
            WorkCapability::WaveformOnly => {
                process_waveforms(db_path, waveform_backend, shared, capability)
            }
            WorkCapability::CharacterAndWaveform => process_character_work(
                db_path,
                analysis_backend,
                waveform_backend,
                shared,
                capability,
            ),
        };
        if let Err(error) = result {
            tracing::error!(%error, "audio analysis worker pass failed");
        }
        finish_pass(db_path, shared);
    }
}

fn wait_for_work(shared: &SharedState, handled_revision: u64) -> Option<(WorkCapability, u64)> {
    let mut state = lock(&shared.state);
    loop {
        if state.shutdown {
            return None;
        }
        if state.revision != handled_revision {
            if let Some(capability) = state.next_capability() {
                shared.decode_cancelled.store(false, Ordering::Release);
                return Some((capability, state.revision));
            }
        }
        state = shared
            .changed
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

fn process_character_work(
    db_path: &std::path::Path,
    analysis_backend: &dyn AudioAnalysisBackend,
    waveform_backend: &dyn WaveformBackend,
    shared: &SharedState,
    capability: WorkCapability,
) -> Result<(), String> {
    let conn = reprise_core::db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    let versions = current_versions();
    let pending =
        sound_profile::pending_tracks(&conn, versions).map_err(|error| error.to_string())?;
    for track in pending {
        if !can_continue(shared, capability) {
            return Ok(());
        }
        publish_running(shared, track.id);
        process_character_track(&conn, analysis_backend, shared, versions, &track)?;
    }
    process_waveforms_with_connection(&conn, waveform_backend, shared, capability)
}

fn process_character_track(
    conn: &rusqlite::Connection,
    backend: &dyn AudioAnalysisBackend,
    shared: &SharedState,
    versions: AnalysisVersions,
    track: &PendingTrack,
) -> Result<(), String> {
    if track.work == PendingWork::Reproject {
        let Some(TrackAnalysis::Ready(stored)) =
            sound_profile::load_analysis(conn, track.id).map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let profile = project_profile(&stored.evidence).map_err(|error| error.to_string())?;
        let ready =
            ReadyAnalysis::new(track.source, versions, now_unix(), stored.evidence, profile)
                .map_err(|error| error.to_string())?;
        sound_profile::save_ready_analysis(conn, track.id, &ready)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    match backend.analyze(std::path::Path::new(&track.path), &shared.decode_cancelled) {
        Ok(output) => {
            if shared.decode_cancelled.load(Ordering::Acquire) {
                Ok(())
            } else {
                save_decoded_track(conn, track, versions, &output)
            }
        }
        Err(AudioAnalysisError::Cancelled) => Ok(()),
        Err(error) => save_failure(conn, track, versions, &error),
    }
}

fn save_decoded_track(
    conn: &rusqlite::Connection,
    track: &PendingTrack,
    versions: AnalysisVersions,
    output: &AnalysisOutput,
) -> Result<(), String> {
    save_waveform_if_current(conn, track.id, track.source, &output.waveform_peaks)
        .map_err(|error| error.to_string())?;
    let ready = ReadyAnalysis::new(
        track.source,
        versions,
        now_unix(),
        output.evidence,
        output.profile,
    )
    .map_err(|error| error.to_string())?;
    sound_profile::save_ready_analysis(conn, track.id, &ready)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn save_failure(
    conn: &rusqlite::Connection,
    track: &PendingTrack,
    versions: AnalysisVersions,
    error: &AudioAnalysisError,
) -> Result<(), String> {
    let kind = match error {
        AudioAnalysisError::FileNotFound(_) => FailureKind::Io,
        AudioAnalysisError::UnsupportedFormat(_) => FailureKind::UnsupportedFormat,
        AudioAnalysisError::Cancelled => FailureKind::Cancelled,
        AudioAnalysisError::DecodeFailed(_) | AudioAnalysisError::EmptyStream => {
            FailureKind::Decode
        }
    };
    let failure = FailedAnalysis::new(
        track.source,
        versions,
        now_unix(),
        kind,
        error.to_string(),
        0,
        None,
    )
    .map_err(|error| error.to_string())?;
    sound_profile::save_failed_analysis(conn, track.id, &failure)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn process_waveforms(
    db_path: &std::path::Path,
    backend: &dyn WaveformBackend,
    shared: &SharedState,
    capability: WorkCapability,
) -> Result<(), String> {
    let conn = reprise_core::db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    process_waveforms_with_connection(&conn, backend, shared, capability)
}

fn process_waveforms_with_connection(
    conn: &rusqlite::Connection,
    backend: &dyn WaveformBackend,
    shared: &SharedState,
    capability: WorkCapability,
) -> Result<(), String> {
    let pending = pending_waveform_work(conn).map_err(|error| error.to_string())?;
    for track in pending {
        if !can_continue(shared, capability) {
            break;
        }
        publish_running(shared, track.track_id);
        if let Ok(peaks) = backend.extract_peaks_cancellable(
            std::path::Path::new(&track.path),
            STORED_PEAK_COUNT,
            &shared.decode_cancelled,
        ) {
            save_waveform_if_current(conn, track.track_id, track.source, &peaks)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn finish_pass(db_path: &std::path::Path, shared: &SharedState) {
    let progress = if let Ok(conn) = reprise_core::db::open_migrated(Some(db_path)) {
        let versions = current_versions();
        let coverage = sound_profile::library_coverage(&conn, versions)
            .unwrap_or_else(|_| sound_profile::Coverage::new(0, 0));
        let failed = conn
            .query_row(
                "SELECT COUNT(*) FROM track_audio_analysis WHERE status = 'failed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_or(0, |count| u64::try_from(count).unwrap_or(0));
        let mut state = lock(&shared.state);
        let activity = if state.shutdown {
            return;
        } else if state.paused {
            AnalysisActivity::Paused
        } else if state.cancelled {
            AnalysisActivity::Cancelled
        } else if !state.enabled {
            AnalysisActivity::Disabled
        } else if failed > 0 {
            AnalysisActivity::Failed
        } else if coverage.analyzed == coverage.total {
            AnalysisActivity::Complete
        } else {
            AnalysisActivity::Idle
        };
        state.progress = AnalysisProgress {
            activity,
            analyzed: coverage.analyzed,
            total: coverage.total,
            failed,
        };
        state.progress
    } else {
        return;
    };
    publish_progress(shared, progress);
}

fn publish_running(shared: &SharedState, track_id: i64) {
    let progress = {
        let mut state = lock(&shared.state);
        state.progress.activity = AnalysisActivity::Running { track_id };
        state.progress
    };
    publish_progress(shared, progress);
}

fn publish_progress(shared: &SharedState, progress: AnalysisProgress) {
    match shared.progress_sender.try_send(progress) {
        Ok(()) => {}
        Err(async_channel::TrySendError::Full(progress)) => {
            let _ = shared.stale_progress.try_recv();
            let _ = shared.progress_sender.try_send(progress);
        }
        Err(async_channel::TrySendError::Closed(_)) => {}
    }
}

fn can_continue(shared: &SharedState, capability: WorkCapability) -> bool {
    lock(&shared.state).next_capability() == Some(capability)
}

fn current_versions() -> AnalysisVersions {
    AnalysisVersions::new(EXTRACTOR_VERSION, CURRENT_PROFILE_VERSION)
        .expect("built-in audio-analysis versions are nonzero")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "audio_analysis_runtime_tests.rs"]
mod tests;
