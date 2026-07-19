//! User-facing copy for local Audio Character analysis.

pub const AUDIO_CHARACTER: &str = N_!("Audio Character");
pub const AUDIO_ANALYSIS_TITLE: &str = N_!("Analyze audio locally");
pub const AUDIO_ANALYSIS_PRIVACY: &str = N_!(
    "Reprise reads audio files only on this device. Nothing is uploaded. Existing profiles are kept when this is turned off."
);
pub const AUDIO_ANALYSIS_PROGRESS: &str = N_!("Analysis progress");
pub const AUDIO_ANALYSIS_EMPTY: &str = N_!("No eligible tracks");
pub const AUDIO_ANALYSIS_OFF: &str =
    N_!("Analysis is off · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_READY: &str =
    N_!("Ready · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_RUNNING: &str =
    N_!("Analyzing · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_PAUSED: &str =
    N_!("Paused · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_CANCELLED: &str =
    N_!("Cancelled · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_FAILED: &str =
    N_!("Failed · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_COMPLETE: &str =
    N_!("Complete · {analyzed} of {total} analyzed · {failed} failed");
pub const AUDIO_ANALYSIS_PAUSE: &str = N_!("Pause analysis");
pub const AUDIO_ANALYSIS_RESUME: &str = N_!("Resume analysis");
pub const AUDIO_ANALYSIS_CANCEL: &str = N_!("Cancel analysis");
pub const AUDIO_ANALYSIS_RETRY: &str = N_!("Retry failed tracks");
pub const AUDIO_ANALYSIS_REANALYZE: &str = N_!("Reanalyze library");
pub const AUDIO_ANALYSIS_REANALYZE_HEADING: &str = N_!("Reanalyze the library?");
pub const AUDIO_ANALYSIS_REANALYZE_BODY: &str = N_!(
    "Existing Audio Character profiles will be replaced. This can take time. Music files and track metadata are not changed."
);
pub const AUDIO_ANALYSIS_REANALYZE_CONFIRM: &str = N_!("Reanalyze");
