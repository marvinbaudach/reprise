//! Platform-neutral values shared by local audio-analysis backends.
//!
//! Extraction itself is added behind this module's backend seam by the Linux
//! adapter task. Keeping callers on this module prevents a platform frontend
//! from depending on the sound-profile storage implementation.

pub use crate::sound_profile::{AnalysisVersions, AudioEvidence, SourceFingerprint, TempoEstimate};

/// Bump only when decoding or evidence extraction changes and cached audio
/// must be read again.
pub const CURRENT_EXTRACTOR_VERSION: u32 = 1;
