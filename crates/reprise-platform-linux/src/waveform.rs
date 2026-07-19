//! GStreamer-backed waveform extraction for Linux frontends.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use reprise_core::audio_analysis::AudioAnalysisError;
use reprise_core::waveform::{WaveformBackend, WaveformError};

#[derive(Clone, Copy, Default)]
pub struct GstreamerWaveformBackend;

impl WaveformBackend for GstreamerWaveformBackend {
    fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        if buckets == 0 {
            return Ok(Vec::new());
        }
        super::audio_analysis::analyze_for_waveform(path, buckets, &AtomicBool::new(false))
            .map_err(map_error)
    }

    fn extract_peaks_cancellable(
        &self,
        path: &Path,
        buckets: usize,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, WaveformError> {
        if buckets == 0 {
            return Ok(Vec::new());
        }
        super::audio_analysis::analyze_for_waveform(path, buckets, cancelled).map_err(map_error)
    }
}

fn map_error(error: AudioAnalysisError) -> WaveformError {
    match error {
        AudioAnalysisError::FileNotFound(path) => WaveformError::FileNotFound(path),
        AudioAnalysisError::EmptyStream => WaveformError::EmptyStream,
        AudioAnalysisError::Cancelled => WaveformError::Cancelled,
        other => WaveformError::DecodeFailed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use reprise_core::waveform::WaveformBackend;

    use super::*;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reprise-core/tests/fixtures/sine.flac")
    }

    #[test]
    fn extract_peaks_returns_requested_bucket_count() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 64)
            .unwrap();
        assert_eq!(peaks.len(), 64);
    }

    #[test]
    fn extract_peaks_has_nonzero_amplitude() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 64)
            .unwrap();
        assert!(peaks.iter().any(|peak| *peak > 0));
    }

    #[test]
    fn extract_peaks_errors_on_missing_file() {
        let result = GstreamerWaveformBackend
            .extract_peaks(Path::new("/tmp/nonexistent_waveform_test.flac"), 64);
        assert!(result.is_err());
    }

    #[test]
    fn extraction_is_deterministic_and_zero_buckets_are_empty() {
        let first = GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 64)
            .unwrap();
        let second = GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 64)
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.iter().copied().max(), Some(255));
        assert!(GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 0)
            .unwrap()
            .is_empty());
    }
}
