//! Weighted distance in the sound-profile comparison space.

use crate::sound_features::SoundFeatures;
use crate::sound_rhythm::RhythmFeatures;
use crate::sound_stats::{RhythmStats, ScalarStats, SoundStats};

/// How much of the rhythm term is *where* the movement sits rather than how
/// much of it there is.
///
/// The per-band flux vector is the richest of the temporal derivations — 24
/// numbers against four — and it is the one that tells a kick-driven track
/// from a strummed one. The scalars sharpen it (a blast beat against a
/// ballad); they do not carry it.
const RHYTHM_SHAPE_SHARE: f32 = 0.6;

/// The weights of the three offered weightings.
///
/// Chosen after measuring the previous set over a real 1793-track library:
/// band means plus the mastering scalars found the same album 11.98x above
/// chance and the same genre 1.07x, and moving weight between timbre and
/// dynamics did not change that (0.95x / 0.98x). That half is an EQ and
/// mastering fingerprint; genre lives in movement.
///
/// `DEFAULT` therefore gives rhythm 0.50 — as much as the whole production
/// half together, because it is the half that was missing. Inside that half
/// `band` keeps the largest share (0.30): the band means are what found the
/// album and the artist, and that is the part of the result a listener
/// recognizes. The two mastering scalars drop to 0.12 and 0.08 — they are the
/// most production-bound of the terms and the measurement showed that moving
/// them changes nothing about genre, so they are tie-breakers, not voices.
///
/// `TIMBRE` and `DYNAMICS` still lean hard where their names say — they are
/// the "more like this colour" and "more like this density" answers — but each
/// keeps a fifth to a third of rhythm, so neither can fall back to comparing
/// masterings alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceWeights {
    pub band: f32,
    pub timbre: f32,
    pub dynamics: f32,
    pub rhythm: f32,
    pub tempo: f32,
}

impl DistanceWeights {
    pub const DEFAULT: Self = Self {
        band: 0.30,
        timbre: 0.12,
        dynamics: 0.08,
        rhythm: 0.50,
        tempo: 0.0,
    };
    pub const TIMBRE: Self = Self {
        band: 0.30,
        timbre: 0.45,
        dynamics: 0.05,
        rhythm: 0.20,
        tempo: 0.0,
    };
    pub const DYNAMICS: Self = Self {
        band: 0.20,
        timbre: 0.05,
        dynamics: 0.45,
        rhythm: 0.30,
        tempo: 0.0,
    };

    pub fn with_tempo(self, enabled: bool) -> Self {
        if !enabled {
            return Self { tempo: 0.0, ..self };
        }
        Self {
            band: self.band * 0.8,
            timbre: self.timbre * 0.8,
            dynamics: self.dynamics * 0.8,
            rhythm: self.rhythm * 0.8,
            tempo: 0.2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundDistance {
    pub total: f32,
    pub band: f32,
    pub timbre: f32,
    pub dynamics: f32,
    pub rhythm: f32,
    pub tempo: f32,
}

pub fn sound_distance(
    left: &SoundFeatures,
    right: &SoundFeatures,
    stats: &SoundStats,
    weights: DistanceWeights,
) -> SoundDistance {
    let band = stats
        .band_mean
        .standardize(cosine_distance(&left.band_mean, &right.band_mean));
    let centroid = standardized_delta(
        left.centroid_mean,
        right.centroid_mean,
        &stats.centroid_mean,
    );
    let variance = standardized_delta(left.centroid_var, right.centroid_var, &stats.centroid_var);
    let timbre = root_mean_square(&[centroid, variance]);
    let dynamics = standardized_delta(
        left.frame_crest_db,
        right.frame_crest_db,
        &stats.frame_crest_db,
    )
    .abs();
    let rhythm = rhythm_distance(&left.rhythm, &right.rhythm, &stats.rhythm);
    let tempo = left.tempo.zip(right.tempo).map_or(0.0, |(left, right)| {
        standardized_delta(left, right, &stats.tempo).abs()
    });
    SoundDistance {
        total: weights.band * band
            + weights.timbre * timbre
            + weights.dynamics * dynamics
            + weights.rhythm * rhythm
            + weights.tempo * tempo,
        band,
        timbre,
        dynamics,
        rhythm,
        tempo,
    }
}

/// Where the movement sits, then how much of it there is, how often it lands
/// and how metronomic it stays.
fn rhythm_distance(left: &RhythmFeatures, right: &RhythmFeatures, stats: &RhythmStats) -> f32 {
    let shape = stats
        .band_flux
        .standardize(cosine_distance(&left.band_flux, &right.band_flux));
    let scalars = root_mean_square(&[
        standardized_delta(left.onset_rate, right.onset_rate, &stats.onset_rate),
        standardized_delta(left.flux_mean, right.flux_mean, &stats.flux_mean),
        standardized_delta(
            left.flux_variation,
            right.flux_variation,
            &stats.flux_variation,
        ),
        standardized_delta(
            left.pulse_strength,
            right.pulse_strength,
            &stats.pulse_strength,
        ),
    ]);
    RHYTHM_SHAPE_SHARE * shape + (1.0 - RHYTHM_SHAPE_SHARE) * scalars
}

fn standardized_delta(left: f32, right: f32, stats: &ScalarStats) -> f32 {
    stats.z_score(left) - stats.z_score(right)
}

/// Folds several standardized differences into one, so a term built from four
/// scalars is not four times the size of one built from a single scalar.
fn root_mean_square(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|value| value * value).sum::<f32>() / values.len() as f32).sqrt()
}

pub(crate) fn cosine_distance(left: &[f32], right: &[f32]) -> f32 {
    let dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        (1.0 - dot / (left_norm * right_norm)).clamp(0.0, 2.0)
    }
}
