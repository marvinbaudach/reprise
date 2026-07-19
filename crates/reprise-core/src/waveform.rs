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
