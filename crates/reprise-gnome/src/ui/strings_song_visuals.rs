//! User-facing copy for the Song Visuals plugin (audio-reactive Now Playing
//! visuals in the inline panel).

pub const VISUAL: &str = N_!("Visual");
pub const SONG_VISUALS: &str = N_!("Song Visuals");
pub const SONG_VISUALS_DESCRIPTION: &str = N_!("Show local audio-reactive visuals in Now Playing");
pub const SONG_VISUALS_ACCESSIBLE: &str = N_!("Audio-reactive song visual");

// The analysis readout under the canvas: exactly the values the glow layer
// reacts to, so what the visual does stays traceable.
pub const SONG_VISUALS_ANALYSIS_ACCESSIBLE: &str = N_!("Live song analysis driving the visual");
pub const SONG_VISUALS_BASS: &str = N_!("Bass");
pub const SONG_VISUALS_BASELINE: &str = N_!("Baseline");
pub const SONG_VISUALS_IMPACT: &str = N_!("Impact");
pub const SONG_VISUALS_BREAKDOWN: &str = N_!("Breakdown");
