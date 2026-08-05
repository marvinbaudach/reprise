//! Where one track sits on the three profile axes the Sound surfaces show.
//!
//! It lives here rather than in a frontend because more than one surface asks
//! the question: the GTK Sound tab draws the three bars, and the MCP server
//! answers `music_sound_profile` with the same numbers. A second derivation
//! would drift from the first the moment either changed (`SIM-4`).

use crate::sound_features::SoundFeatures;
use crate::sound_stats::SoundStats;

/// One track's position on each axis, as a library-wide percentile
/// (`0.0..=100.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfilePositions {
    /// Timbre, dark to bright — the frame centroid's percentile.
    pub timbre: f32,
    /// Dynamics, dense to open — the frame crest's percentile.
    pub dynamics: f32,
    /// Tempo, slow to fast. `None` when tempo is excluded or when the track
    /// carries no stable tempo estimate.
    pub tempo: Option<f32>,
}

/// The three axis positions of `features` against the library `stats` they were
/// computed over. `include_tempo` is the caller's tempo setting: excluding
/// tempo drops the axis rather than placing it at zero.
pub fn profile_positions(
    features: &SoundFeatures,
    stats: &SoundStats,
    include_tempo: bool,
) -> ProfilePositions {
    ProfilePositions {
        timbre: stats.centroid_mean.percentile(features.centroid_mean),
        dynamics: stats.frame_crest_db.percentile(features.frame_crest_db),
        tempo: include_tempo
            .then(|| features.tempo.map(|tempo| stats.tempo.percentile(tempo)))
            .flatten(),
    }
}
