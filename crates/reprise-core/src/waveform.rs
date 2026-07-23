//! Platform-neutral waveform extraction contract.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const STORED_PEAK_COUNT: usize = 1000;

#[derive(Debug, thiserror::Error)]
pub enum WaveformError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("decode failed: {0}")]
    DecodeFailed(String),
    #[error("empty audio stream")]
    EmptyStream,
    #[error("waveform extraction cancelled")]
    Cancelled,
}

pub trait WaveformBackend: Send + Sync {
    fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError>;

    fn extract_peaks_cancellable(
        &self,
        path: &Path,
        buckets: usize,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, WaveformError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(WaveformError::Cancelled);
        }
        let peaks = self.extract_peaks(path, buckets)?;
        if cancelled.load(Ordering::Acquire) {
            Err(WaveformError::Cancelled)
        } else {
            Ok(peaks)
        }
    }
}

/// Streaming bucketed-RMS reducer for seek-bar waveforms. Fold decoded mono
/// PCM in via [`push`](Self::push) and finalize with [`finish`](Self::finish);
/// the byte peaks it emits match what the old audio-character extractor
/// produced for the waveform half, so cached peaks stay comparable.
pub struct WaveformAccumulator {
    expected_samples: u64,
    samples_seen: u64,
    sum_squares: Vec<f64>,
    counts: Vec<u64>,
}

impl WaveformAccumulator {
    /// `expected_samples` is the stream's upper-bound sample count (used to map
    /// each sample to its bucket); `buckets` is the requested peak resolution.
    pub fn new(expected_samples: u64, buckets: usize) -> Result<Self, WaveformError> {
        if expected_samples == 0 || buckets == 0 {
            return Err(WaveformError::DecodeFailed(
                "waveform bucket count and expected sample count must be greater than zero".into(),
            ));
        }
        Ok(Self {
            expected_samples,
            samples_seen: 0,
            sum_squares: vec![0.0; buckets],
            counts: vec![0; buckets],
        })
    }

    /// Accumulates one decoded chunk. Samples are clamped to `[-1.0, 1.0]`;
    /// a non-finite sample or a total exceeding `expected_samples` is an error.
    pub fn push(&mut self, samples: &[f32]) -> Result<(), WaveformError> {
        let new_total = self
            .samples_seen
            .checked_add(samples.len() as u64)
            .filter(|total| *total <= self.expected_samples)
            .ok_or_else(|| {
                WaveformError::DecodeFailed(
                    "audio stream contains more samples than declared".into(),
                )
            })?;
        let buckets = self.sum_squares.len() as u64;
        for &sample in samples {
            if !sample.is_finite() {
                return Err(WaveformError::DecodeFailed(
                    "audio stream contains a non-finite sample".into(),
                ));
            }
            let sample = f64::from(sample.clamp(-1.0, 1.0));
            let bucket = ((self.samples_seen * buckets) / self.expected_samples)
                .min(buckets.saturating_sub(1)) as usize;
            self.sum_squares[bucket] += sample * sample;
            self.counts[bucket] += 1;
            self.samples_seen += 1;
        }
        debug_assert_eq!(self.samples_seen, new_total);
        Ok(())
    }

    /// Finalizes the per-bucket RMS into normalized `0..=255` byte peaks.
    pub fn finish(self) -> Result<Vec<u8>, WaveformError> {
        if self.samples_seen == 0 {
            return Err(WaveformError::EmptyStream);
        }
        Ok(finish_waveform(&self.sum_squares, &self.counts))
    }
}

fn finish_waveform(sum_squares: &[f64], counts: &[u64]) -> Vec<u8> {
    let rms = sum_squares
        .iter()
        .zip(counts)
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                (sum / *count as f64).sqrt()
            }
        })
        .collect::<Vec<_>>();
    let maximum = rms.iter().copied().fold(0.0_f64, f64::max);
    if maximum <= f64::EPSILON {
        return vec![0; rms.len()];
    }
    rms.into_iter()
        .map(|value| ((value / maximum).sqrt() * 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    struct FakeWaveformBackend;

    impl WaveformBackend for FakeWaveformBackend {
        fn extract_peaks(&self, _path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Ok(vec![42; buckets])
        }
    }

    #[test]
    fn trait_object_preserves_requested_bucket_count() {
        let backend: &dyn WaveformBackend = &FakeWaveformBackend;
        let peaks = backend.extract_peaks(Path::new("fixture.flac"), 7).unwrap();
        assert_eq!(peaks, vec![42; 7]);
    }
}
