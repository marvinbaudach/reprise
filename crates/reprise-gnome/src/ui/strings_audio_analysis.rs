//! User-facing copy for local Audio Character analysis.

pub const AUDIO_CHARACTER: &str = N_!("Audio Character");
pub const SONG_ANALYSIS: &str = N_!("Song analysis");
pub const VISUAL: &str = N_!("Visual");
pub const SONG_VISUALS: &str = N_!("Song Visuals");
pub const SONG_VISUALS_DESCRIPTION: &str = N_!("Show local audio-reactive visuals in Now Playing");
pub const SONG_VISUALS_RINGS: &str = N_!("Rings");
pub const SONG_VISUALS_FLOW: &str = N_!("Flow");
pub const SONG_VISUALS_PULSE: &str = N_!("Pulse");
pub const SONG_VISUALS_ACCESSIBLE: &str = N_!("Audio-reactive song visual");
pub const SONG_VISUALS_FULLSCREEN_HINT: &str =
    N_!("F11 Fullscreen · color follows the cover accent");
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
pub const AUDIO_CHARACTER_EMPTY: &str = N_!("Play a track to see its Audio Character");
pub const AUDIO_CHARACTER_EMPTY_DESCRIPTION: &str =
    N_!("The loaded track's local audio profile will appear here.");
pub const AUDIO_CHARACTER_DISABLED: &str = N_!("Local audio analysis is disabled");
pub const AUDIO_CHARACTER_DISABLED_DESCRIPTION: &str =
    N_!("Enable Analyze audio locally in Library Settings.");
pub const AUDIO_CHARACTER_PENDING: &str = N_!("Audio Character is pending");
pub const AUDIO_CHARACTER_PENDING_DESCRIPTION: &str =
    N_!("This track is waiting for local analysis.");
pub const AUDIO_CHARACTER_FAILED: &str = N_!("Audio Character analysis failed");
pub const AUDIO_CHARACTER_FAILED_DESCRIPTION: &str =
    N_!("Retry failed tracks from Library Settings.");
pub const AUDIO_CHARACTER_STALE: &str = N_!("Audio Character is stale");
pub const AUDIO_CHARACTER_STALE_DESCRIPTION: &str =
    N_!("The local profile will be updated in the background.");
pub const AUDIO_CHARACTER_INTENSITY: &str = N_!("Intensity");
pub const AUDIO_CHARACTER_BRIGHTNESS: &str = N_!("Brightness");
pub const AUDIO_CHARACTER_DYNAMICITY: &str = N_!("Dynamicity");
pub const AUDIO_CHARACTER_RHYTHMICITY: &str = N_!("Rhythmicity");
pub const AUDIO_CHARACTER_CONFIDENCE: &str = N_!("Confidence {confidence}%");
pub const AUDIO_CHARACTER_DIMENSION_ACCESSIBLE: &str =
    N_!("{dimension}, {value}%, confidence {confidence}%");
pub const AUDIO_CHARACTER_TEMPO: &str = N_!("Tempo");
pub const AUDIO_CHARACTER_BPM: &str = N_!("{bpm} BPM");
pub const AUDIO_CHARACTER_TEMPO_ACCESSIBLE: &str =
    N_!("Tempo, {bpm} BPM, confidence {confidence}%");
