//! One decode of a CUE file on the phone: the track asked for, and every
//! other track of its file that still needs its analysis, measured in the
//! same pass (finding C7). Skipping through an album therefore costs one
//! decode of its file, not one per track — the desktop's
//! `waveform_cache::measure_cue_file` does the same.
//!
//! A foreground request for a track whose file the backfill is already
//! decoding under another track's id preempts that decode and decodes the
//! file again: the in-flight registry is keyed by track. That costs a second
//! decode in a rare case and is accepted rather than optimised.

use reprise_core::db::{
    pending_segment_tracks_of, set_segment_render_data, track_is_last_in_file, Db,
};
use reprise_core::models::Track;
use reprise_core::render_data_segments::SegmentBounds;
use reprise_core::spectrogram::TrackSourceFingerprint;
use reprise_core::waveform::TrackRenderData;

use crate::LibraryError;

use super::compute::{failure_outcome, AndroidAnalysisOutcome};

/// The tracks one decode of a CUE file measures, the one asked for first.
pub(super) struct SegmentJob {
    tracks: Vec<(i64, SegmentBounds)>,
}

impl SegmentJob {
    /// The job for `track`, `None` for a whole-file track. Read under the
    /// reader lock, before the decode starts.
    pub(super) fn plan(reader: &Db, track: &Track) -> Result<Option<Self>, LibraryError> {
        let Some(segment) = &track.segment else {
            return Ok(None);
        };
        let last_in_file = track_is_last_in_file(reader, track.id).map_err(database_error)?;
        let asked = SegmentBounds {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            last_in_file,
        };
        let siblings = pending_segment_tracks_of(reader, &track.path)
            .map_err(database_error)?
            .into_iter()
            .filter(|sibling| sibling.track_id != track.id)
            .map(|sibling| (sibling.track_id, sibling.bounds()));
        Ok(Some(Self {
            tracks: std::iter::once((track.id, asked)).chain(siblings).collect(),
        }))
    }

    /// The stretches to cut, in the job's order.
    pub(super) fn bounds(&self) -> Vec<SegmentBounds> {
        self.tracks.iter().map(|(_, bounds)| *bounds).collect()
    }

    /// Every track the job measures, with the stretch it measures for it.
    pub(super) fn measured(&self) -> impl Iterator<Item = (i64, SegmentBounds)> + '_ {
        self.tracks.iter().copied()
    }

    /// Stores what the decode measured for each track, each only while the
    /// file and that track's cut are what they were when the decode began,
    /// and remembers a track the stream never reached as failed, under the
    /// same condition. Returns the
    /// outcome for the track asked for.
    pub(super) fn store(
        &self,
        writer: &Db,
        source: TrackSourceFingerprint,
        results: Vec<Result<TrackRenderData, String>>,
    ) -> Result<AndroidAnalysisOutcome, LibraryError> {
        let mut asked = AndroidAnalysisOutcome::DecodeFailed;
        for (index, ((track_id, bounds), result)) in self.tracks.iter().zip(results).enumerate() {
            let outcome = match result {
                Ok(data) => {
                    match set_segment_render_data(writer, *track_id, source, *bounds, &data)
                        .map_err(database_error)?
                    {
                        reprise_core::db::SpectrogramStoreOutcome::Stored => {
                            AndroidAnalysisOutcome::Computed
                        }
                        reprise_core::db::SpectrogramStoreOutcome::SourceChanged => {
                            AndroidAnalysisOutcome::PhoneSourceChanged
                        }
                    }
                }
                Err(reason) => {
                    tracing::debug!(track_id, reason, "a CUE track's stretch was not decoded");
                    failure_outcome(
                        reprise_core::spectrogram_backfill::record_render_data_failure(
                            writer,
                            *track_id,
                            source,
                            Some(*bounds),
                            &reason,
                        )
                        .map_err(database_error)?,
                    )
                }
            };
            if index == 0 {
                asked = outcome;
            }
        }
        Ok(asked)
    }
}

fn database_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}
