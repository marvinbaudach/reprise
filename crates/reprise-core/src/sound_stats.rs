//! Library-wide scalar statistics for sound-profile comparison and display.

use crate::db::{Db, DbError};
use crate::sound_distance::cosine_distance;
use crate::sound_features::SoundFeatures;
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

const INVENTORY_RECOMPUTE_FRACTION: f64 = 0.05;
/// Two tracks scatter around the library's centre independently, so the
/// distance between them is about twice the distance of one of them to that
/// centre — cosine distance is half the squared angle for the small angles
/// real tracks span.
const PAIR_SPREAD_FACTOR: f32 = 2.0;
/// Below this spread the library holds one shape, and a distance inside it is
/// quantization noise rather than a difference.
const SPREAD_EPSILON: f32 = 1.0e-6;

#[derive(Debug, Clone, PartialEq)]
pub struct ScalarStats {
    pub mean: f32,
    pub std_dev: f32,
    pub sorted: Vec<f32>,
}

impl ScalarStats {
    pub fn z_score(&self, value: f32) -> f32 {
        if self.std_dev == 0.0 {
            0.0
        } else {
            (value - self.mean) / self.std_dev
        }
    }

    pub fn percentile(&self, value: f32) -> f32 {
        if self.sorted.is_empty() {
            return 0.0;
        }
        if self.sorted.len() == 1 {
            return 50.0;
        }
        let below = self.sorted.partition_point(|candidate| *candidate < value);
        let through = self.sorted.partition_point(|candidate| *candidate <= value);
        let rank = if through > below {
            (below + through - 1) as f32 / 2.0
        } else {
            below.min(self.sorted.len() - 1) as f32
        };
        rank / (self.sorted.len() - 1) as f32 * 100.0
    }
}

/// How far the library spreads around one shared direction of a normalized
/// feature vector.
///
/// It exists because a nominal weight has to be an effective one. Cosine
/// distances between real `band_mean` vectors sit in the hundredths while a
/// standardized scalar difference is around one, so a nominal band weight of
/// 0.5 moved the ranking by a fiftieth of what it claimed and the scalars
/// decided everything. Dividing by the library's own spread puts both kinds of
/// term into the same units, which is the same service [`ScalarStats::z_score`]
/// performs for the scalars.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorStats {
    /// Mean cosine distance of a stored vector to the library's mean vector.
    pub spread: f32,
}

impl VectorStats {
    /// A cosine distance on the scale of a standardized scalar difference.
    /// Zero spread contributes zero, exactly as it does for a scalar.
    pub fn standardize(&self, distance: f32) -> f32 {
        if self.spread <= SPREAD_EPSILON {
            0.0
        } else {
            distance / (PAIR_SPREAD_FACTOR * self.spread)
        }
    }
}

/// Library spread of the temporal features.
#[derive(Debug, Clone, PartialEq)]
pub struct RhythmStats {
    pub band_flux: VectorStats,
    pub onset_rate: ScalarStats,
    pub flux_mean: ScalarStats,
    pub flux_variation: ScalarStats,
    pub pulse_strength: ScalarStats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoundStats {
    pub feature_count: usize,
    pub band_mean: VectorStats,
    pub centroid_mean: ScalarStats,
    pub centroid_var: ScalarStats,
    pub frame_crest_db: ScalarStats,
    pub tempo: ScalarStats,
    pub rhythm: RhythmStats,
}

pub fn compute_sound_stats(features: &[SoundFeatures]) -> SoundStats {
    SoundStats {
        feature_count: features.len(),
        band_mean: vector_stats(features.iter().map(|feature| &feature.band_mean)),
        centroid_mean: scalar_stats(features.iter().map(|feature| feature.centroid_mean)),
        centroid_var: scalar_stats(features.iter().map(|feature| feature.centroid_var)),
        frame_crest_db: scalar_stats(features.iter().map(|feature| feature.frame_crest_db)),
        tempo: scalar_stats(features.iter().filter_map(|feature| feature.tempo)),
        rhythm: RhythmStats {
            band_flux: vector_stats(features.iter().map(|feature| &feature.rhythm.band_flux)),
            onset_rate: scalar_stats(features.iter().map(|feature| feature.rhythm.onset_rate)),
            flux_mean: scalar_stats(features.iter().map(|feature| feature.rhythm.flux_mean)),
            flux_variation: scalar_stats(
                features.iter().map(|feature| feature.rhythm.flux_variation),
            ),
            pulse_strength: scalar_stats(
                features.iter().map(|feature| feature.rhythm.pulse_strength),
            ),
        },
    }
}

fn vector_stats<'a>(
    vectors: impl Iterator<Item = &'a [f32; SPECTROGRAM_BAND_COUNT]> + Clone,
) -> VectorStats {
    let mut centre = [0.0_f32; SPECTROGRAM_BAND_COUNT];
    let mut count = 0_usize;
    for vector in vectors.clone() {
        for (slot, value) in centre.iter_mut().zip(vector) {
            *slot += value;
        }
        count += 1;
    }
    if count == 0 {
        return VectorStats { spread: 0.0 };
    }
    for slot in &mut centre {
        *slot /= count as f32;
    }
    let spread = vectors
        .map(|vector| cosine_distance(vector, &centre))
        .sum::<f32>()
        / count as f32;
    VectorStats { spread }
}

fn scalar_stats(values: impl IntoIterator<Item = f32>) -> ScalarStats {
    let mut sorted: Vec<f32> = values
        .into_iter()
        .filter(|value| value.is_finite())
        .collect();
    sorted.sort_by(f32::total_cmp);
    if sorted.is_empty() {
        return ScalarStats {
            mean: 0.0,
            std_dev: 0.0,
            sorted,
        };
    }
    let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
    let variance = sorted
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / sorted.len() as f32;
    ScalarStats {
        mean,
        std_dev: variance.sqrt(),
        sorted,
    }
}

pub fn count_changed_more_than_five_percent(previous: usize, current: usize) -> bool {
    if previous == 0 {
        return current > 0;
    }
    previous.abs_diff(current) as f64 / previous as f64 > INVENTORY_RECOMPUTE_FRACTION
}

#[derive(Debug, Default)]
pub struct SoundStatsCache {
    stats: Option<SoundStats>,
}

impl SoundStatsCache {
    pub fn stats(&self) -> Option<&SoundStats> {
        self.stats.as_ref()
    }

    /// Rebuilds only on first use or after a strict greater-than-five-percent
    /// change in the valid feature inventory. Returns whether it rebuilt.
    pub fn refresh(&mut self, db: &Db) -> Result<bool, DbError> {
        let current_count = crate::db_sound_features::sound_feature_count(db)?;
        let previous_count = crate::library::settings::get_sound_stats_feature_count(db)?;
        if self.stats.is_some()
            && previous_count.is_some_and(|previous| {
                !count_changed_more_than_five_percent(previous, current_count)
            })
        {
            return Ok(false);
        }
        let rows = crate::db_sound_features::all_track_sound_features(db)?;
        self.stats = Some(compute_sound_stats(
            &rows.into_iter().map(|row| row.features).collect::<Vec<_>>(),
        ));
        crate::library::settings::set_sound_stats_feature_count(db, current_count)?;
        Ok(true)
    }
}
