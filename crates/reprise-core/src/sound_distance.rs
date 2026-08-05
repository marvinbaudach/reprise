//! Weighted distance in the sound-profile comparison space.

use crate::sound_features::SoundFeatures;
use crate::sound_stats::SoundStats;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceWeights {
    pub band: f32,
    pub timbre: f32,
    pub dynamics: f32,
    pub tempo: f32,
}

impl DistanceWeights {
    pub const DEFAULT: Self = Self {
        band: 0.5,
        timbre: 0.25,
        dynamics: 0.25,
        tempo: 0.0,
    };
    pub const TIMBRE: Self = Self {
        band: 0.35,
        timbre: 0.5,
        dynamics: 0.15,
        tempo: 0.0,
    };
    pub const DYNAMICS: Self = Self {
        band: 0.35,
        timbre: 0.15,
        dynamics: 0.5,
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
    pub tempo: f32,
}

pub fn sound_distance(
    left: &SoundFeatures,
    right: &SoundFeatures,
    stats: &SoundStats,
    weights: DistanceWeights,
) -> SoundDistance {
    let band = cosine_distance(&left.band_mean, &right.band_mean);
    let centroid = standardized_delta(
        left.centroid_mean,
        right.centroid_mean,
        &stats.centroid_mean,
    );
    let variance = standardized_delta(left.centroid_var, right.centroid_var, &stats.centroid_var);
    let timbre = ((centroid * centroid + variance * variance) / 2.0).sqrt();
    let dynamics = standardized_delta(
        left.frame_crest_db,
        right.frame_crest_db,
        &stats.frame_crest_db,
    )
    .abs();
    let tempo = left.tempo.zip(right.tempo).map_or(0.0, |(left, right)| {
        standardized_delta(left, right, &stats.tempo).abs()
    });
    SoundDistance {
        total: weights.band * band
            + weights.timbre * timbre
            + weights.dynamics * dynamics
            + weights.tempo * tempo,
        band,
        timbre,
        dynamics,
        tempo,
    }
}

fn standardized_delta(left: f32, right: f32, stats: &crate::sound_stats::ScalarStats) -> f32 {
    stats.z_score(left) - stats.z_score(right)
}

fn cosine_distance(left: &[f32], right: &[f32]) -> f32 {
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
