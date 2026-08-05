use crate::sound_distance::{sound_distance, DistanceWeights};
use crate::sound_features::SoundFeatures;
use crate::sound_stats::compute_sound_stats;
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

fn features(
    band: usize,
    centroid: f32,
    variance: f32,
    crest: f32,
    tempo: Option<f32>,
) -> SoundFeatures {
    let mut band_mean = [0.0; SPECTROGRAM_BAND_COUNT];
    band_mean[band] = 1.0;
    SoundFeatures {
        band_mean,
        centroid_mean: centroid,
        centroid_var: variance,
        frame_crest_db: crest,
        tempo,
    }
}

#[test]
fn sim_2_distance_uses_cosine_bands_and_standardized_scalars() {
    let first = features(0, 10.0, 2.0, 4.0, Some(100.0));
    let second = features(1, 12.0, 4.0, 8.0, Some(120.0));
    let stats = compute_sound_stats(&[first.clone(), second.clone()]);
    let distance = sound_distance(&first, &second, &stats, DistanceWeights::DEFAULT);

    assert_eq!(distance.band, 1.0);
    assert!((distance.timbre - 2.0).abs() < 1.0e-5);
    assert_eq!(distance.dynamics, 2.0);
    assert_eq!(distance.tempo, 2.0);
    assert!((distance.total - 1.5).abs() < 1.0e-5);
}

#[test]
fn sim_2_zero_spread_and_disabled_tempo_add_nothing() {
    let first = features(0, 10.0, 2.0, 4.0, Some(100.0));
    let second = features(0, 10.0, 2.0, 4.0, Some(200.0));
    let stats = compute_sound_stats(&[first.clone(), second.clone()]);
    let distance = sound_distance(&first, &second, &stats, DistanceWeights::DEFAULT);
    assert_eq!(distance.total, 0.0);
    assert_eq!(distance.tempo, 2.0);
}

#[test]
fn sim_2_tempo_assigns_a_nonzero_weight_only_when_enabled() {
    assert_eq!(DistanceWeights::DEFAULT.tempo, 0.0);
    let enabled = DistanceWeights::DEFAULT.with_tempo(true);
    assert_eq!(enabled.tempo, 0.2);
    assert!(
        (enabled.band + enabled.timbre + enabled.dynamics + enabled.tempo - 1.0).abs() < 1.0e-6
    );
}
