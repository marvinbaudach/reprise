//! A snapshot of a [`RenderDataSession`] whose stream is still being decoded.
//! Nothing here is ever stored: the partial picture lives in memory only and
//! is replaced by the real analysis once `finish()` has run.

use super::{finish_waveform_peaks, RenderDataSession};
use crate::spectrogram::TrackSpectrogram;

/// The decoded left part of a track's render data.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialRenderData {
    /// The complete leading buckets, normalised to the loudest of them.
    pub waveform_peaks: Vec<u8>,
    /// The decoded prefix of the final spectrogram, whole frames only.
    pub spectrogram: TrackSpectrogram,
    /// How much of the track the peaks cover, in `0.0..=1.0`.
    pub covered_fraction: f32,
}

impl RenderDataSession {
    /// Snapshots what has been decoded so far, or `None` while not even one
    /// peak bucket is complete.
    ///
    /// `expected_frames` is the track's length in spectrogram frames, known
    /// from its duration before the decode ends. Buckets are mapped from that
    /// fixed length, not from the frames seen so far, so a bucket never moves
    /// once emitted: bucket `b` covers frames `[b*E/n, (b+1)*E/n)` and is
    /// emitted only when all of them are decoded. Peaks are then normalised
    /// to the loudest emitted bucket, the rule `finish()` applies, so a
    /// finished stream's snapshot equals its final peaks. A stream that runs
    /// past `expected_frames` (a duration that was too short) is mapped over
    /// the frames actually seen and reports full coverage.
    ///
    /// The frame still filling (shorter than 1600 samples) is left out.
    #[must_use]
    pub fn partial(&self, expected_frames: usize) -> Option<PartialRenderData> {
        let decoded = self.frames.len();
        let total = expected_frames.max(decoded);
        if total == 0 {
            return None;
        }
        let buckets = self.peak_count;
        let complete = (0..buckets)
            .take_while(|&bucket| {
                let (start, end) = bucket_range(bucket, total, buckets);
                end.max(start + 1) <= decoded
            })
            .count();
        if complete == 0 {
            return None;
        }

        let mut sum_squares = Vec::with_capacity(complete);
        let mut counts = Vec::with_capacity(complete);
        for bucket in 0..complete {
            let (start, end) = bucket_range(bucket, total, buckets);
            let frames = &self.frames[start..end.max(start + 1)];
            sum_squares.push(frames.iter().map(|(sum, _)| sum).sum());
            counts.push(frames.iter().map(|(_, count)| count).sum());
        }
        Some(PartialRenderData {
            waveform_peaks: finish_waveform_peaks(&sum_squares, &counts),
            spectrogram: if self.full_analysis {
                self.spectrogram.snapshot()
            } else {
                TrackSpectrogram::empty()
            },
            covered_fraction: (complete as f32 / buckets as f32).clamp(0.0, 1.0),
        })
    }
}

/// The `[start, end)` frames of `bucket` when `total` frames are spread over
/// `buckets`. Empty (`start == end`) for a track shorter than `buckets`
/// frames; the caller then takes the bucket's nearest frame, as
/// `rebucket_peaks` does.
fn bucket_range(bucket: usize, total: usize, buckets: usize) -> (usize, usize) {
    ((bucket * total) / buckets, ((bucket + 1) * total) / buckets)
}
