//! Explicit, resumable production of stored track rendering data.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::db::{
    pending_render_data_tracks, pending_segment_render_data_files, set_segment_render_data,
    set_track_render_data, Db, DbError, PendingSegmentFile, SpectrogramStoreOutcome,
};
use crate::render_data_segments::SegmentBounds;
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
    let segment_files = pending_segment_render_data_files(db)?;
    let total = pending.len()
        + segment_files
            .iter()
            .map(|file| file.tracks.len())
            .sum::<usize>();
    let mut summary = BackfillSummary {
        status: BackfillStatus::Completed,
        stored: 0,
        failed: 0,
        source_changed: 0,
    };
    let mut completed = 0;

    for track in pending {
        if cancelled.load(Ordering::Acquire) {
            summary.status = BackfillStatus::Cancelled;
            return Ok(summary);
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
                return Ok(summary);
            }
            Err(error) => {
                tracing::warn!(
                    track_id = track.track_id,
                    error = %error,
                    "spectrogram backfill left a track pending after decode failure"
                );
                summary.failed += 1;
                completed += 1;
                on_progress(BackfillProgress {
                    completed,
                    total,
                    track_id: track.track_id,
                });
                continue;
            }
        };
        let outcome = set_track_render_data(db, track.track_id, track.source, &data)?;
        count_store(outcome, &mut summary);
        completed += 1;
        on_progress(BackfillProgress {
            completed,
            total,
            track_id: track.track_id,
        });
        if cancelled.load(Ordering::Acquire) {
            summary.status = BackfillStatus::Cancelled;
            return Ok(summary);
        }
    }

    for file in segment_files {
        if cancelled.load(Ordering::Acquire) {
            summary.status = BackfillStatus::Cancelled;
            return Ok(summary);
        }
        let Some(datas) = extract_file(backend, &file, cancelled, &mut summary) else {
            if summary.status == BackfillStatus::Cancelled {
                return Ok(summary);
            }
            for track in &file.tracks {
                completed += 1;
                on_progress(BackfillProgress {
                    completed,
                    total,
                    track_id: track.track_id,
                });
            }
            continue;
        };
        for (track, data) in file.tracks.iter().zip(&datas) {
            let bounds = SegmentBounds {
                start_ms: track.start_ms,
                end_ms: track.end_ms,
            };
            let outcome = set_segment_render_data(db, track.track_id, file.source, bounds, data)?;
            count_store(outcome, &mut summary);
            completed += 1;
            on_progress(BackfillProgress {
                completed,
                total,
                track_id: track.track_id,
            });
        }
        if cancelled.load(Ordering::Acquire) {
            summary.status = BackfillStatus::Cancelled;
            return Ok(summary);
        }
    }

    Ok(summary)
}

/// Decodes one CUE file once for all of its pending tracks. `None` when nothing
/// was produced: the scan was cancelled (the summary says so) or the decode
/// failed (every track of the file counts as failed and stays pending).
fn extract_file(
    backend: &dyn RenderDataBackend,
    file: &PendingSegmentFile,
    cancelled: &AtomicBool,
    summary: &mut BackfillSummary,
) -> Option<Vec<TrackRenderData>> {
    let bounds: Vec<SegmentBounds> = file
        .tracks
        .iter()
        .map(|track| SegmentBounds {
            start_ms: track.start_ms,
            end_ms: track.end_ms,
        })
        .collect();
    match backend.extract_segment_render_data_cancellable(
        std::path::Path::new(&file.path),
        &bounds,
        STORED_PEAK_COUNT,
        cancelled,
    ) {
        Ok(datas) if datas.len() == file.tracks.len() => Some(datas),
        Ok(_) => {
            tracing::warn!(path = %file.path, "backend returned the wrong number of tracks");
            summary.failed += file.tracks.len();
            None
        }
        Err(WaveformError::EmptyStream) => Some(
            file.tracks
                .iter()
                .map(|_| TrackRenderData::empty())
                .collect(),
        ),
        Err(WaveformError::Cancelled) => {
            summary.status = BackfillStatus::Cancelled;
            None
        }
        Err(error) => {
            tracing::warn!(
                path = %file.path,
                error = %error,
                "spectrogram backfill left a CUE file pending after decode failure"
            );
            summary.failed += file.tracks.len();
            None
        }
    }
}

/// Counts a store. A track whose file or cut changed during the decode stays
/// pending, and the next run measures it as it is then.
fn count_store(outcome: SpectrogramStoreOutcome, summary: &mut BackfillSummary) {
    match outcome {
        SpectrogramStoreOutcome::Stored => summary.stored += 1,
        SpectrogramStoreOutcome::SourceChanged => summary.source_changed += 1,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::db::set_track_spectrogram;
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
                loudness: None,
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

    #[test]
    fn stored_spectrogram_does_not_skip_a_still_missing_waveform() {
        let db = database();
        let source = pending_render_data_tracks(&db).unwrap()[0].source;
        set_track_spectrogram(
            &db,
            1,
            source,
            &TrackSpectrogram::from_cells(vec![180; 48]).unwrap(),
        )
        .unwrap();
        let backend = FakeBackend {
            calls: AtomicUsize::new(0),
            cancel_after_first: false,
        };

        run_render_data_backfill(&db, &backend, &AtomicBool::new(false), |_| {}).unwrap();

        assert_eq!(backend.calls.load(Ordering::Relaxed), 3);
        assert_eq!(
            crate::db::get_waveform_peaks(&db, 1).unwrap(),
            Some(vec![1; STORED_PEAK_COUNT])
        );
    }

    /// Hands each of a file's tracks data that says which track it is.
    struct CuePerTrackBackend {
        file_calls: AtomicUsize,
        whole_calls: AtomicUsize,
        bounds_seen: std::sync::Mutex<Vec<Vec<SegmentBounds>>>,
    }

    impl WaveformBackend for CuePerTrackBackend {
        fn extract_peaks(&self, _path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Ok(vec![1; buckets])
        }
    }

    impl RenderDataBackend for CuePerTrackBackend {
        fn extract_render_data_cancellable(
            &self,
            _path: &Path,
            buckets: usize,
            _cancelled: &AtomicBool,
        ) -> Result<TrackRenderData, WaveformError> {
            self.whole_calls.fetch_add(1, Ordering::Relaxed);
            Ok(TrackRenderData {
                waveform_peaks: vec![200; buckets],
                spectrogram: TrackSpectrogram::from_cells(vec![9; 24]).unwrap(),
                loudness: None,
            })
        }

        fn extract_segment_render_data_cancellable(
            &self,
            _path: &Path,
            segments: &[SegmentBounds],
            buckets: usize,
            _cancelled: &AtomicBool,
        ) -> Result<Vec<TrackRenderData>, WaveformError> {
            self.file_calls.fetch_add(1, Ordering::Relaxed);
            self.bounds_seen.lock().unwrap().push(segments.to_vec());
            Ok(segments
                .iter()
                .map(|segment| TrackRenderData {
                    waveform_peaks: vec![(segment.start_ms / 1_000) as u8; buckets],
                    spectrogram: TrackSpectrogram::from_cells(vec![1; 24]).unwrap(),
                    loudness: None,
                })
                .collect())
        }
    }

    fn database_with_a_cue_file() -> Db {
        let db = database();
        db.conn()
            .execute_batch(
                "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size, device, inode,
                                     segment_index, segment_start_ms, segment_end_ms)
                 VALUES (10, '/live.flac', 'One', 0, 11, 22, 33, 90, 1, 0, 3000),
                        (11, '/live.flac', 'Two', 0, 11, 22, 33, 90, 2, 3000, 8000),
                        (12, '/live.flac', 'Three', 0, 11, 22, 33, 90, 3, 8000, 9000);",
            )
            .unwrap();
        db
    }

    fn cue_backend() -> CuePerTrackBackend {
        CuePerTrackBackend {
            file_calls: AtomicUsize::new(0),
            whole_calls: AtomicUsize::new(0),
            bounds_seen: std::sync::Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn cue_9_a_cue_file_is_decoded_once_and_each_of_its_tracks_gets_its_own_data() {
        let db = database_with_a_cue_file();
        let backend = cue_backend();
        let mut progress = Vec::new();

        let summary = run_render_data_backfill(&db, &backend, &AtomicBool::new(false), |step| {
            progress.push((step.completed, step.total));
        })
        .unwrap();

        assert_eq!(backend.whole_calls.load(Ordering::Relaxed), 3);
        assert_eq!(
            backend.file_calls.load(Ordering::Relaxed),
            1,
            "one decode for the file"
        );
        assert_eq!(
            backend.bounds_seen.lock().unwrap()[0],
            [
                SegmentBounds {
                    start_ms: 0,
                    end_ms: 3_000
                },
                SegmentBounds {
                    start_ms: 3_000,
                    end_ms: 8_000
                },
                SegmentBounds {
                    start_ms: 8_000,
                    end_ms: 9_000
                },
            ]
        );
        assert_eq!(summary.stored, 6);
        assert_eq!(progress.last(), Some(&(6, 6)));
        assert!(pending_render_data_tracks(&db).unwrap().is_empty());
        assert!(crate::db::pending_segment_render_data_files(&db)
            .unwrap()
            .is_empty());
        for (track_id, first_peak) in [(10, 0), (11, 3), (12, 8)] {
            let peaks = crate::db::get_waveform_peaks(&db, track_id)
                .unwrap()
                .unwrap();
            assert_eq!(peaks[0], first_peak, "track {track_id}");
        }
    }

    #[test]
    fn cue_9_a_cue_file_that_cannot_be_decoded_leaves_every_track_pending() {
        let db = database_with_a_cue_file();

        let summary = run_render_data_backfill(
            &db,
            &FakeBackend {
                calls: AtomicUsize::new(0),
                cancel_after_first: false,
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();

        assert_eq!(summary.stored, 3, "the plain tracks");
        assert_eq!(summary.failed, 3, "the cue tracks");
        let pending = crate::db::pending_segment_render_data_files(&db).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].tracks.len(), 3);
    }

    #[test]
    fn cue_9_only_the_tracks_that_still_need_data_are_cut_out_again() {
        let db = database_with_a_cue_file();
        run_render_data_backfill(&db, &cue_backend(), &AtomicBool::new(false), |_| {}).unwrap();
        db.conn()
            .execute("UPDATE tracks SET segment_end_ms = 8500 WHERE id = 11", [])
            .unwrap();
        let backend = cue_backend();

        let summary =
            run_render_data_backfill(&db, &backend, &AtomicBool::new(false), |_| {}).unwrap();

        assert_eq!(summary.stored, 1);
        assert_eq!(
            backend.bounds_seen.lock().unwrap().as_slice(),
            [vec![SegmentBounds {
                start_ms: 3_000,
                end_ms: 8_500
            }]]
        );
    }

    /// Re-cuts track 11 through its own connection while the file decodes, as a
    /// rescan that applies an edited sheet does.
    struct RecuttingBackend {
        rescanner: std::sync::Mutex<Db>,
        inner: CuePerTrackBackend,
    }

    impl WaveformBackend for RecuttingBackend {
        fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
            self.inner.extract_peaks(path, buckets)
        }
    }

    impl RenderDataBackend for RecuttingBackend {
        fn extract_render_data_cancellable(
            &self,
            path: &Path,
            buckets: usize,
            cancelled: &AtomicBool,
        ) -> Result<TrackRenderData, WaveformError> {
            self.inner
                .extract_render_data_cancellable(path, buckets, cancelled)
        }

        fn extract_segment_render_data_cancellable(
            &self,
            path: &Path,
            segments: &[SegmentBounds],
            buckets: usize,
            cancelled: &AtomicBool,
        ) -> Result<Vec<TrackRenderData>, WaveformError> {
            self.rescanner
                .lock()
                .unwrap()
                .conn()
                .execute("UPDATE tracks SET segment_end_ms = 8500 WHERE id = 11", [])
                .unwrap();
            self.inner
                .extract_segment_render_data_cancellable(path, segments, buckets, cancelled)
        }
    }

    #[test]
    fn cue_9_a_track_re_cut_while_its_file_decodes_keeps_nothing_from_the_old_cut() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recut.db");
        let db = Db::open_migrated(Some(&path)).unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size, device, inode,
                                     segment_index, segment_start_ms, segment_end_ms)
                 VALUES (10, '/live.flac', 'One', 0, 11, 22, 33, 90, 1, 0, 3000),
                        (11, '/live.flac', 'Two', 0, 11, 22, 33, 90, 2, 3000, 8000);",
            )
            .unwrap();
        let backend = RecuttingBackend {
            rescanner: std::sync::Mutex::new(Db::open_migrated(Some(&path)).unwrap()),
            inner: cue_backend(),
        };

        let summary =
            run_render_data_backfill(&db, &backend, &AtomicBool::new(false), |_| {}).unwrap();

        assert_eq!((summary.stored, summary.source_changed), (1, 1));
        assert_eq!(crate::db::get_waveform_peaks(&db, 11).unwrap(), None);
        let pending = crate::db::pending_segment_render_data_files(&db).unwrap();
        assert_eq!(pending[0].tracks[0].track_id, 11, "measured again later");
    }
}
