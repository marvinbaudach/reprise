//! Library-wide scalar statistics for sound-profile comparison and display.

use crate::db::{Db, DbError};
use crate::sound_features::SoundFeatures;

const INVENTORY_RECOMPUTE_FRACTION: f64 = 0.05;

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

#[derive(Debug, Clone, PartialEq)]
pub struct SoundStats {
    pub feature_count: usize,
    pub centroid_mean: ScalarStats,
    pub centroid_var: ScalarStats,
    pub frame_crest_db: ScalarStats,
    pub tempo: ScalarStats,
}

pub fn compute_sound_stats(features: &[SoundFeatures]) -> SoundStats {
    SoundStats {
        feature_count: features.len(),
        centroid_mean: scalar_stats(features.iter().map(|feature| feature.centroid_mean)),
        centroid_var: scalar_stats(features.iter().map(|feature| feature.centroid_var)),
        frame_crest_db: scalar_stats(features.iter().map(|feature| feature.frame_crest_db)),
        tempo: scalar_stats(features.iter().filter_map(|feature| feature.tempo)),
    }
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
