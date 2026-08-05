use crate::sound_features::SoundFeatures;
use crate::sound_rhythm::RhythmFeatures;
use crate::sound_stats::{
    compute_sound_stats, count_changed_more_than_five_percent, SoundStatsCache,
};
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

fn features(centroid: f32, variance: f32, crest: f32, tempo: Option<f32>) -> SoundFeatures {
    SoundFeatures {
        band_mean: [SPECTROGRAM_BAND_COUNT as f32; SPECTROGRAM_BAND_COUNT],
        centroid_mean: centroid,
        centroid_var: variance,
        frame_crest_db: crest,
        rhythm: RhythmFeatures::still(),
        tempo,
    }
}

/// A profile whose only interesting content is its movement.
fn moving(band: usize, onset_rate: f32, pulse_strength: f32) -> SoundFeatures {
    let mut band_flux = [0.0; SPECTROGRAM_BAND_COUNT];
    band_flux[band] = 1.0;
    SoundFeatures {
        rhythm: RhythmFeatures {
            band_flux,
            onset_rate,
            flux_mean: 10.0,
            flux_variation: 0.5,
            pulse_strength,
        },
        ..features(1.0, 1.0, 1.0, None)
    }
}

#[test]
fn sound_stats_standardizes_scalars_and_caches_sorted_axis_columns() {
    let stats = compute_sound_stats(&[
        features(1.0, 4.0, 10.0, Some(100.0)),
        features(3.0, 8.0, 30.0, None),
    ]);

    assert_eq!(stats.feature_count, 2);
    assert_eq!(stats.centroid_mean.mean, 2.0);
    assert_eq!(stats.centroid_mean.std_dev, 1.0);
    assert_eq!(stats.centroid_mean.sorted, vec![1.0, 3.0]);
    assert_eq!(stats.centroid_var.sorted, vec![4.0, 8.0]);
    assert_eq!(stats.frame_crest_db.sorted, vec![10.0, 30.0]);
    assert_eq!(stats.tempo.sorted, vec![100.0]);
    assert_eq!(stats.centroid_mean.z_score(3.0), 1.0);
}

#[test]
fn sound_stats_zero_spread_contributes_zero_and_percentiles_span_the_axis() {
    let stats = compute_sound_stats(&[
        features(2.0, 1.0, 7.0, None),
        features(2.0, 2.0, 7.0, None),
        features(2.0, 3.0, 7.0, None),
    ]);

    assert_eq!(stats.centroid_mean.z_score(99.0), 0.0);
    assert_eq!(stats.centroid_var.percentile(1.0), 0.0);
    assert_eq!(stats.centroid_var.percentile(2.0), 50.0);
    assert_eq!(stats.centroid_var.percentile(3.0), 100.0);
}

#[test]
fn sound_stats_refresh_threshold_is_strictly_more_than_five_percent() {
    assert!(!count_changed_more_than_five_percent(100, 105));
    assert!(!count_changed_more_than_five_percent(100, 95));
    assert!(count_changed_more_than_five_percent(100, 106));
    assert!(count_changed_more_than_five_percent(100, 94));
    assert!(count_changed_more_than_five_percent(0, 1));
}

#[test]
fn sound_stats_cache_records_the_inventory_count_in_library_settings() {
    let db = crate::db::Db::open_in_memory().unwrap();
    for id in 1..=2 {
        db.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, added_at) VALUES (?1, ?2, '', 0)",
                rusqlite::params![id, format!("/{id}.flac")],
            )
            .unwrap();
        crate::db::set_track_sound_features(&db, id, &features(id as f32, 1.0, 1.0, None)).unwrap();
    }

    let mut cache = SoundStatsCache::default();
    assert!(cache.refresh(&db).unwrap());
    assert_eq!(cache.stats().unwrap().feature_count, 2);
    assert_eq!(
        crate::library::settings::get_sound_stats_feature_count(&db).unwrap(),
        Some(2)
    );
    assert!(!cache.refresh(&db).unwrap());
}

#[test]
fn sound_stats_standardize_the_rhythm_scalars_too() {
    let stats = compute_sound_stats(&[moving(1, 2.0, 0.2), moving(1, 4.0, 0.6)]);

    assert_eq!(stats.rhythm.onset_rate.mean, 3.0);
    assert_eq!(stats.rhythm.onset_rate.std_dev, 1.0);
    assert_eq!(stats.rhythm.onset_rate.z_score(4.0), 1.0);
    assert_eq!(stats.rhythm.pulse_strength.sorted, vec![0.2, 0.6]);
    assert_eq!(stats.rhythm.flux_variation.std_dev, 0.0);
}

#[test]
fn sound_stats_put_a_cosine_distance_on_the_scale_of_a_standardized_difference() {
    // Two vectors pointing at different bands. Each sits 1 - 1/sqrt(2) away
    // from the direction they share, so the distance between them — a full
    // 1.0 — is a touch more than one standardized step apart, not a fiftieth
    // of one as the raw cosine distance would have made it.
    let stats = compute_sound_stats(&[moving(1, 1.0, 0.5), moving(5, 1.0, 0.5)]);

    let spread = stats.rhythm.band_flux.spread;
    assert!((spread - (1.0 - 0.5_f32.sqrt())).abs() < 1.0e-5, "{spread}");
    assert!((stats.rhythm.band_flux.standardize(1.0) - 1.0 / (2.0 * spread)).abs() < 1.0e-5);
}

#[test]
fn sound_stats_zero_vector_spread_contributes_zero() {
    let stats = compute_sound_stats(&[moving(3, 1.0, 0.5), moving(3, 2.0, 0.5)]);

    assert_eq!(stats.rhythm.band_flux.spread, 0.0);
    assert_eq!(stats.rhythm.band_flux.standardize(1.0), 0.0);
}
