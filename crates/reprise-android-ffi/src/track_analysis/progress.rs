//! The partial picture of a track whose analysis is still being computed.
//! Read from the running decode's own session, in memory: the stored reads
//! (`track_spectrogram`, `track_render_bars`) keep meaning "final".

use reprise_core::db::{get_track_spectrogram, get_waveform_peaks};
use reprise_core::queries::query_present_track_by_id;

use crate::{LibraryError, MusicLibrary};

use super::{
    android_spectrogram, database_error, query_error, shaped_render_bars, AndroidTrackRenderBar,
    AndroidTrackSpectrogram,
};

/// How much of a track the phone has analysed so far, drawn the same way as
/// the finished analysis.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AndroidTrackAnalysisProgress {
    /// The decoded share of the track in `0.0..=1.0`; the bars span exactly
    /// this part, from the left.
    pub covered_fraction: f32,
    /// About `bar_count * covered_fraction` bars for the decoded part.
    pub bars: Vec<AndroidTrackRenderBar>,
    /// The decoded prefix of the spectrogram, in the final format.
    pub spectrogram: AndroidTrackSpectrogram,
}

#[uniffi::export]
impl MusicLibrary {
    /// Returns the part of `track_id` decoded so far while its analysis runs.
    ///
    /// `Ok(None)` when no decode is running for the track, when it has no
    /// known duration, when not even one bucket is decoded yet, or when the
    /// final analysis is already stored (read that instead). The partial
    /// picture is never stored: a process restart loses it.
    pub fn track_analysis_progress(
        &self,
        track_id: i64,
        bar_count: u32,
    ) -> Result<Option<AndroidTrackAnalysisProgress>, LibraryError> {
        let Some(decode) = self.analysis_in_flight.decodes().lookup(track_id) else {
            return Ok(None);
        };
        let duration_ms = {
            let reader = self.reader()?;
            let track = query_present_track_by_id(&reader, track_id)
                .map_err(query_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            let stored_peaks = get_waveform_peaks(&reader, track_id).map_err(database_error)?;
            let stored_spectrogram =
                get_track_spectrogram(&reader, track_id).map_err(database_error)?;
            if stored_peaks.is_some() && stored_spectrogram.is_some() {
                return Ok(None);
            }
            track.duration_ms
        };
        let Some(partial) = decode.sink.partial(decode.expected_frames) else {
            return Ok(None);
        };

        let wanted = (f64::from(bar_count) * f64::from(partial.covered_fraction)).round() as usize;
        let bars = shaped_render_bars(
            &partial.waveform_peaks,
            &partial.spectrogram,
            wanted.max(1).min(bar_count as usize),
            f64::from(partial.covered_fraction) * duration_ms as f64 / 1_000.0,
        );
        if bars.is_empty() {
            return Ok(None);
        }
        Ok(Some(AndroidTrackAnalysisProgress {
            covered_fraction: partial.covered_fraction,
            bars,
            spectrogram: android_spectrogram(partial.spectrogram),
        }))
    }
}
