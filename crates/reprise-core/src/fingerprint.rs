//! Platform-neutral audio fingerprinting contract.
//!
//! Core owns only the typed capability, progress, cancellation, and result
//! vocabulary. Decoding and Chromaprint integration remain platform work.

use std::path::{Path, PathBuf};

/// Revision of the decoded-audio pipeline presented to Chromaprint.
///
/// A platform backend combines this with its actual runtime plugin version;
/// this token deliberately does not claim to identify libchromaprint itself.
pub const GST_CHROMAPRINT_PIPELINE_REVISION: &str = "pipeline-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FingerprintCapability {
    Available { cache_namespace: String },
    MissingPlugin { elements: Vec<String> },
    BackendInitFailed { detail: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FingerprintProgress {
    pub processed_seconds: u64,
    /// Full source duration, not the portion processed by a capped backend.
    pub duration_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerprintControl {
    Continue,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    pub encoded: String,
    /// Full source duration, even when fingerprinting uses only a prefix.
    pub duration_seconds: u64,
    pub cache_namespace: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FingerprintOutcome {
    Completed(Fingerprint),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FingerprintError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("fingerprint backend unavailable: {0:?}")]
    BackendUnavailable(FingerprintCapability),
    #[error("audio decode failed: {0}")]
    DecodeFailed(String),
    #[error("source duration is unavailable")]
    DurationUnavailable,
    #[error("fingerprint backend returned an empty fingerprint")]
    EmptyFingerprint,
}

pub trait FingerprintBackend: Send + Sync {
    fn capability(&self) -> FingerprintCapability;

    fn fingerprint(
        &self,
        path: &Path,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError>;
}
