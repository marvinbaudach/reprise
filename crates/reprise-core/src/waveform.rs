//! Platform-neutral waveform extraction contract.

use std::path::{Path, PathBuf};

pub const STORED_PEAK_COUNT: usize = 1000;

#[derive(Debug, thiserror::Error)]
pub enum WaveformError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("decode failed: {0}")]
    DecodeFailed(String),
    #[error("empty audio stream")]
    EmptyStream,
}

pub trait WaveformBackend: Send + Sync {
    fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError>;
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
