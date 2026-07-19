use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::AnalysisOutput;

pub trait AudioAnalysisBackend: Send + Sync {
    fn analyze(
        &self,
        path: &Path,
        cancelled: &AtomicBool,
    ) -> Result<AnalysisOutput, AudioAnalysisError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AudioAnalysisError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("audio decode failed: {0}")]
    DecodeFailed(String),
    #[error("empty audio stream")]
    EmptyStream,
    #[error("audio analysis cancelled")]
    Cancelled,
}
