//! Explicit, resumable production of stored track rendering data.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::db::{
    pending_render_data_tracks, pending_segment_render_data_files, set_segment_render_data,
    set_track_render_data, Db, DbError, PendingSegmentFile, PendingSegmentTrack,
    SpectrogramStoreOutcome,
};
pub use crate::db_spectrogram::{
    clear_render_data_failure, record_render_data_failure, render_data_failed,
};
use crate::render_data_segments::SegmentBounds;
use crate::waveform::{
    RenderDataBackend, SegmentRenderData, TrackRenderData, WaveformError, STORED_PEAK_COUNT,
};

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
                    "spectrogram backfill could not decode a track; it waits for its file to change"
                );
                record_render_data_failure(db, track.track_id, &error.to_string())?;
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
        let datas = match extract_file(backend, &file, cancelled) {
            FileDecode::Decoded(datas) => datas,
            FileDecode::Cancelled => {
                summary.status = BackfillStatus::Cancelled;
                return Ok(summary);
            }
            FileDecode::Failed(reason) => {
                summary.failed += file.tracks.len();
                for track in &file.tracks {
                    record_render_data_failure(db, track.track_id, &reason)?;
                }
                report_file_done(&file, &mut completed, total, &mut on_progress);
                continue;
            }
        };
        for (track, data) in file.tracks.iter().zip(&datas) {
            match data {
                Ok(data) => {
                    let outcome = set_segment_render_data(
                        db,
                        track.track_id,
                        file.source,
                        track.bounds(),
                        data,
                    )?;
                    count_store(outcome, &mut summary);
                }
                Err(error) => {
                    tracing::warn!(
                        track_id = track.track_id,
                        error = %error,
                        "spectrogram backfill could not measure a CUE track; it waits for its file to change"
                    );
                    record_render_data_failure(db, track.track_id, &error.to_string())?;
                    summary.failed += 1;
                }
            }
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

/// Reports every track of a file whose decode failed as done with.
fn report_file_done(
    file: &PendingSegmentFile,
    completed: &mut usize,
    total: usize,
    on_progress: &mut impl FnMut(BackfillProgress),
) {
    for track in &file.tracks {
        *completed += 1;
        on_progress(BackfillProgress {
            completed: *completed,
            total,
            track_id: track.track_id,
        });
    }
}

/// What one decode of a CUE file produced.
enum FileDecode {
    /// One result per pending track, in their order.
    Decoded(Vec<SegmentRenderData>),
    Cancelled,
    /// Nothing at all, for this reason.
    Failed(String),
}

/// Decodes one CUE file once for all of its pending tracks. A track whose
/// stretch the decode never reached has its own error in the result.
fn extract_file(
    backend: &dyn RenderDataBackend,
    file: &PendingSegmentFile,
    cancelled: &AtomicBool,
) -> FileDecode {
    let bounds: Vec<SegmentBounds> = file
        .tracks
        .iter()
        .map(PendingSegmentTrack::bounds)
        .collect();
    match backend.extract_segment_render_data_cancellable(
        std::path::Path::new(&file.path),
        &bounds,
        STORED_PEAK_COUNT,
        cancelled,
    ) {
        Ok(datas) if datas.len() == file.tracks.len() => FileDecode::Decoded(datas),
        Ok(_) => {
            tracing::warn!(path = %file.path, "backend returned the wrong number of tracks");
            FileDecode::Failed("the backend returned the wrong number of tracks".into())
        }
        Err(WaveformError::Cancelled) => FileDecode::Cancelled,
        Err(error) => {
            tracing::warn!(
                path = %file.path,
                error = %error,
                "spectrogram backfill could not decode a CUE file; it waits for the file to change"
            );
            FileDecode::Failed(error.to_string())
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
#[path = "spectrogram_backfill_tests.rs"]
mod tests;
