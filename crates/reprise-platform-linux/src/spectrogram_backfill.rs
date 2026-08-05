//! Explicit Linux worker for the resumable rendering-data backfill.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use reprise_core::db::{Db, DbError};
use reprise_core::spectrogram_backfill::{
    run_render_data_backfill, BackfillProgress, BackfillSummary,
};

use crate::waveform::GstreamerWaveformBackend;

#[derive(Debug)]
pub enum BackfillWorkerError {
    Database(DbError),
    Panicked,
}

impl From<DbError> for BackfillWorkerError {
    fn from(error: DbError) -> Self {
        Self::Database(error)
    }
}

impl std::fmt::Display for BackfillWorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => error.fmt(formatter),
            Self::Panicked => formatter.write_str("spectrogram backfill worker panicked"),
        }
    }
}

impl std::error::Error for BackfillWorkerError {}

pub struct SpectrogramBackfillHandle {
    cancelled: Arc<AtomicBool>,
    progress: mpsc::Receiver<BackfillProgress>,
    worker: Option<JoinHandle<Result<BackfillSummary, DbError>>>,
}

impl SpectrogramBackfillHandle {
    /// Starts a resumable pass over whatever is still pending.
    ///
    /// The window starts one on launch, so this must stay cheap when there is
    /// nothing to do: it takes the pending list first and ends immediately when
    /// that list is empty.
    pub fn start(database_path: PathBuf) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (progress_tx, progress) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let db = Db::open_ready(&database_path)?;
            run_render_data_backfill(
                &db,
                &GstreamerWaveformBackend,
                &worker_cancelled,
                |update| {
                    let _ = progress_tx.send(update);
                },
            )
        });
        Self {
            cancelled,
            progress,
            worker: Some(worker),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn try_progress(&self) -> impl Iterator<Item = BackfillProgress> + '_ {
        self.progress.try_iter()
    }

    /// Whether the worker thread has run to its end.
    ///
    /// A caller that polls progress needs to know when to stop polling without
    /// blocking on `join`, which would freeze a UI thread for as long as the
    /// current track takes to decode. `join` after this returns `true` does not
    /// block.
    pub fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    pub fn join(mut self) -> Result<BackfillSummary, BackfillWorkerError> {
        self.worker
            .take()
            .expect("backfill worker is joined at most once")
            .join()
            .map_err(|_| BackfillWorkerError::Panicked)?
            .map_err(Into::into)
    }
}

impl Drop for SpectrogramBackfillHandle {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use reprise_core::db::{pending_render_data_tracks, Db};
    use reprise_core::library::scanner::scan_folder;

    use super::*;

    #[test]
    fn explicit_worker_populates_a_pending_track_and_then_has_nothing_to_resume() {
        let directory = tempfile::tempdir().unwrap();
        let audio_path = directory.path().join("track.flac");
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
            &audio_path,
        )
        .unwrap();
        let database_path = directory.path().join("reprise.db");
        let db = Db::open_migrated(Some(&database_path)).unwrap();
        scan_folder(&db, directory.path()).unwrap();
        drop(db);

        let summary = SpectrogramBackfillHandle::start(database_path.clone())
            .join()
            .unwrap();

        assert_eq!(summary.stored, 1);
        let db = Db::open_ready(&database_path).unwrap();
        assert!(pending_render_data_tracks(&db).unwrap().is_empty());
    }
}
