//! Platform-neutral values shared by local audio-analysis backends.
//!
//! Extraction itself is added behind this module's backend seam by the Linux
//! adapter task. Keeping callers on this module prevents a platform frontend
//! from depending on the sound-profile storage implementation.

pub use crate::sound_profile::{AnalysisVersions, AudioEvidence, SourceFingerprint, TempoEstimate};

#[path = "audio_analysis_accumulator.rs"]
mod accumulator;
pub use accumulator::{
    project_profile, AnalysisOutput, AudioEvidenceAccumulator, AudioExtractionError,
};

#[path = "audio_analysis_backend.rs"]
mod backend;
pub use backend::{AudioAnalysisBackend, AudioAnalysisError};

#[path = "audio_analysis_storage.rs"]
mod storage;
pub use storage::{
    pending_waveform_work, reset_all_analyses, reset_failed_analyses, save_waveform_if_current,
    PendingWaveform,
};

/// Bump only when decoding or evidence extraction changes and cached audio
/// must be read again.
pub const CURRENT_EXTRACTOR_VERSION: u32 = 1;

#[cfg(test)]
#[path = "audio_analysis_tests.rs"]
mod tests;
