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

/// What a snapshot is built from, copied out of a session so the picture can
/// be computed without holding whatever lock guards the session: the decoder
/// pushes through that lock, and only the copy has to happen under it.
#[derive(Debug, Clone)]
pub struct PartialSource {
    frames: Vec<(f64, u64)>,
    spectrogram: Option<TrackSpectrogram>,
    peak_count: usize,
}

impl RenderDataSession {
    /// Snapshots what has been decoded so far; see [`PartialSource::render`].
    #[must_use]
    pub fn partial(&self, expected_frames: usize) -> Option<PartialRenderData> {
        self.partial_source().render(expected_frames)
    }

    /// Copies the frames decoded so far, whole frames only: the frame still
    /// filling (shorter than 1600 samples) is left out.
    #[must_use]
    pub fn partial_source(&self) -> PartialSource {
        PartialSource {
            frames: self.frames.clone(),
            spectrogram: self.full_analysis.then(|| self.spectrogram.snapshot()),
            peak_count: self.peak_count,
        }
    }
}

impl PartialSource {
    /// The decoded left part of the track, or `None` while not even one peak
    /// bucket is complete.
    ///
    /// `expected_frames` is the track's length in spectrogram frames, known
    /// from its duration before the decode ends. Bucket boundaries are mapped
    /// from that fixed length, not from the frames seen so far: bucket `b`
    /// covers frames `[b*E/n, (b+1)*E/n)` and is emitted only when all of them
    /// are decoded, so a later snapshot never moves an emitted bucket's
    /// frames. Peaks are then normalised to the loudest emitted bucket, the
    /// rule `finish()` applies, so an emitted value changes only when a louder
    /// bucket arrives, and a finished stream's snapshot equals its final
    /// peaks.
    ///
    /// A stream that runs past `expected_frames` (a duration that was too
    /// short) is mapped over the frames actually seen instead and reports full
    /// coverage. From then on the boundaries follow the growing frame count,
    /// so in that case emitted buckets do shift, as they do in `finish()`.
    ///
    /// The spectrogram is cut to the frames the emitted buckets cover, so
    /// colour read from it lines up with the peaks.
    #[must_use]
    pub fn render(self, expected_frames: usize) -> Option<PartialRenderData> {
        let decoded = self.frames.len();
        let total = expected_frames.max(decoded);
        let buckets = self.peak_count;
        if total == 0 || buckets == 0 {
            return None;
        }
        let complete = (0..buckets)
            .take_while(|&bucket| bucket_frames(bucket, total, buckets).end <= decoded)
            .count();
        if complete == 0 {
            return None;
        }

        let mut sum_squares = Vec::with_capacity(complete);
        let mut counts = Vec::with_capacity(complete);
        for bucket in 0..complete {
            let frames = &self.frames[bucket_frames(bucket, total, buckets)];
            sum_squares.push(frames.iter().map(|(sum, _)| sum).sum());
            counts.push(frames.iter().map(|(_, count)| count).sum());
        }
        let covered_frames = bucket_frames(complete - 1, total, buckets).end;
        Some(PartialRenderData {
            waveform_peaks: finish_waveform_peaks(&sum_squares, &counts),
            spectrogram: self
                .spectrogram
                .map_or_else(TrackSpectrogram::empty, |spectrogram| {
                    spectrogram.truncated(covered_frames)
                }),
            covered_fraction: (complete as f32 / buckets as f32).clamp(0.0, 1.0),
        })
    }
}

/// The frames of `bucket` when `total` frames are spread over `buckets`:
/// `[bucket*total/buckets, (bucket+1)*total/buckets)`, widened to the nearest
/// frame for a track shorter than `buckets` frames, as `rebucket_peaks` does.
/// The products are formed in `u128`: `total` comes from a duration tag, and
/// an absurd one must not overflow on a 32-bit phone.
fn bucket_frames(bucket: usize, total: usize, buckets: usize) -> std::ops::Range<usize> {
    let boundary = |index: usize| {
        let frame = index as u128 * total as u128 / buckets as u128;
        usize::try_from(frame).unwrap_or(usize::MAX)
    };
    let start = boundary(bucket);
    start..boundary(bucket + 1).max(start.saturating_add(1))
}
