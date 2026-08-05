//! Explicit, resumable production of stored track rendering data.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::db::{
    pending_render_data_tracks, set_track_render_data, Db, DbError, SpectrogramStoreOutcome,
};
use crate::waveform::{RenderDataBackend, TrackRenderData, WaveformError, STORED_PEAK_COUNT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillStatus {
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackfillProgress {
    pub completed: usize,
    pub total: usize,
    pub track_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillSummary {
    pub status: BackfillStatus,
    pub stored: usize,
    pub failed: usize,
    pub source_changed: usize,
}

pub fn run_render_data_backfill(
    db: &Db,
    backend: &dyn RenderDataBackend,
    cancelled: &AtomicBool,
    mut on_progress: impl FnMut(BackfillProgress),
) -> Result<BackfillSummary, DbError> {
    let pending = pending_render_data_tracks(db)?;
    let total = pending.len();
    let mut summary = BackfillSummary {
        status: BackfillStatus::Completed,
        stored: 0,
        failed: 0,
        source_changed: 0,
    };

    for (index, track) in pending.into_iter().enumerate() {
        if cancelled.load(Ordering::Acquire) {
            summary.status = BackfillStatus::Cancelled;
            break;
        }
        let data = match backend.extract_render_data_cancellable(
            std::path::Path::new(&track.path),
            STORED_PEAK_COUNT,
            cancelled,
        ) {
            Ok(data) => data,
            Err(WaveformError::EmptyStream) => TrackRenderData::empty(),
            Err(WaveformError::Cancelled) => {
                summary.status = BackfillStatus::Cancelled;
                break;
            }
            Err(error) => {
                tracing::warn!(
                    track_id = track.track_id,
                    error = %error,
                    "spectrogram backfill left a track pending after decode failure"
                );
                summary.failed += 1;
                on_progress(BackfillProgress {
                    completed: index + 1,
                    total,
                    track_id: track.track_id,
                });
                continue;
            }
        };
        match set_track_render_data(db, track.track_id, track.source, &data)? {
            SpectrogramStoreOutcome::Stored => summary.stored += 1,
            SpectrogramStoreOutcome::SourceChanged => summary.source_changed += 1,
        }
        on_progress(BackfillProgress {
            completed: index + 1,
            total,
            track_id: track.track_id,
        });
        if cancelled.load(Ordering::Acquire) {
            summary.status = BackfillStatus::Cancelled;
            break;
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::db::{get_track_spectrogram, pending_render_data_tracks};
    use crate::spectrogram::TrackSpectrogram;
    use crate::waveform::{RenderDataBackend, TrackRenderData, WaveformBackend, WaveformError};

    struct FakeBackend {
        calls: AtomicUsize,
        cancel_after_first: bool,
    }

    struct EmptyBackend;

    impl WaveformBackend for EmptyBackend {
        fn extract_peaks(&self, _path: &Path, _buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Err(WaveformError::EmptyStream)
        }
    }

    impl RenderDataBackend for EmptyBackend {
        fn extract_render_data_cancellable(
            &self,
            _path: &Path,
            _buckets: usize,
            _cancelled: &AtomicBool,
        ) -> Result<TrackRenderData, WaveformError> {
            Err(WaveformError::EmptyStream)
        }
    }

    impl WaveformBackend for FakeBackend {
        fn extract_peaks(&self, _path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Ok(vec![1; buckets])
        }
    }

    impl RenderDataBackend for FakeBackend {
        fn extract_render_data_cancellable(
            &self,
            _path: &Path,
            buckets: usize,
            cancelled: &AtomicBool,
        ) -> Result<TrackRenderData, WaveformError> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            if self.cancel_after_first && call == 0 {
                cancelled.store(true, Ordering::Release);
            }
            Ok(TrackRenderData {
                waveform_peaks: vec![call as u8 + 1; buckets],
                spectrogram: TrackSpectrogram::from_cells(vec![call as u8 + 1; 24]).unwrap(),
            })
        }
    }

    fn database() -> Db {
        let db = Db::open_in_memory().unwrap();
        for id in 1..=3 {
            db.conn()
                .execute(
                    "INSERT INTO tracks \
                     (id, path, title, added_at, file_mtime, file_size, device, inode) \
                     VALUES (?1, ?2, '', 0, 11, 22, 33, ?3)",
                    rusqlite::params![id, format!("/{id}.flac"), 40 + id],
                )
                .unwrap();
        }
        db
    }

    #[test]
    fn cancelled_run_persists_one_track_and_resumes_the_remaining_rows() {
        let db = database();
        let cancelled = AtomicBool::new(false);
        let first_backend = FakeBackend {
            calls: AtomicUsize::new(0),
            cancel_after_first: true,
        };

        let first = run_render_data_backfill(&db, &first_backend, &cancelled, |_| {}).unwrap();

        assert_eq!(
            first,
            BackfillSummary {
                status: BackfillStatus::Cancelled,
                stored: 1,
                failed: 0,
                source_changed: 0,
            }
        );
        assert!(get_track_spectrogram(&db, 1).unwrap().is_some());
        assert_eq!(pending_render_data_tracks(&db).unwrap().len(), 2);

        cancelled.store(false, Ordering::Release);
        let second_backend = FakeBackend {
            calls: AtomicUsize::new(0),
            cancel_after_first: false,
        };
        let second = run_render_data_backfill(&db, &second_backend, &cancelled, |_| {}).unwrap();

        assert_eq!(second.status, BackfillStatus::Completed);
        assert_eq!(second.stored, 2);
        assert!(pending_render_data_tracks(&db).unwrap().is_empty());
    }

    #[test]
    fn decoded_empty_tracks_are_complete_and_are_not_retried() {
        let db = database();
        let summary =
            run_render_data_backfill(&db, &EmptyBackend, &AtomicBool::new(false), |_| {}).unwrap();

        assert_eq!(summary.stored, 3);
        assert_eq!(summary.failed, 0);
        assert_eq!(
            get_track_spectrogram(&db, 1).unwrap(),
            Some(TrackSpectrogram::empty())
        );
        assert!(pending_render_data_tracks(&db).unwrap().is_empty());
    }
}
