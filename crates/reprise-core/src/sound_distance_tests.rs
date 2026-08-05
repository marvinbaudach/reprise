use crate::sound_distance::{sound_distance, DistanceWeights};
use crate::sound_features::SoundFeatures;
use crate::sound_rhythm::RhythmFeatures;
use crate::sound_stats::compute_sound_stats;
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

fn one_hot(band: usize) -> [f32; SPECTROGRAM_BAND_COUNT] {
    let mut vector = [0.0; SPECTROGRAM_BAND_COUNT];
    vector[band] = 1.0;
    vector
}

/// A profile that does not move: only its production half carries anything.
fn features(
    band: usize,
    centroid: f32,
    variance: f32,
    crest: f32,
    tempo: Option<f32>,
) -> SoundFeatures {
    SoundFeatures {
        band_mean: one_hot(band),
        centroid_mean: centroid,
        centroid_var: variance,
        frame_crest_db: crest,
        rhythm: RhythmFeatures::still(),
        tempo,
    }
}

/// The same profile with movement in `flux_band` at `onset_rate` onsets a
/// second.
fn moving(mut features: SoundFeatures, flux_band: usize, onset_rate: f32) -> SoundFeatures {
    features.rhythm = RhythmFeatures {
        band_flux: one_hot(flux_band),
        onset_rate,
        flux_mean: 10.0,
        flux_variation: 0.5,
        pulse_strength: 0.5,
    };
    features
}

#[test]
fn sim_9_distance_scales_cosine_shapes_and_standardizes_scalars() {
    let first = features(0, 10.0, 2.0, 4.0, Some(100.0));
    let second = features(1, 12.0, 4.0, 8.0, Some(120.0));
    let stats = compute_sound_stats(&[first.clone(), second.clone()]);
    let distance = sound_distance(&first, &second, &stats, DistanceWeights::DEFAULT);

    // Two one-hot vectors sit 1 - 1/sqrt(2) from the direction they share, so
    // their full cosine distance of 1.0 reads as 1 / (2 * 0.29289) once it is
    // put on the scale of a standardized difference.
    assert!(
        (distance.band - 1.707_1).abs() < 1.0e-4,
        "{}",
        distance.band
    );
    assert!((distance.timbre - 2.0).abs() < 1.0e-5);
    assert_eq!(distance.dynamics, 2.0);
    assert_eq!(distance.tempo, 2.0);
    assert_eq!(distance.rhythm, 0.0, "neither track moves");
    assert!(
        (distance.total - 0.912_1).abs() < 1.0e-4,
        "{}",
        distance.total
    );
}

#[test]
fn sim_9_zero_spread_and_disabled_tempo_add_nothing() {
    let first = features(0, 10.0, 2.0, 4.0, Some(100.0));
    let second = features(0, 10.0, 2.0, 4.0, Some(200.0));
    let stats = compute_sound_stats(&[first.clone(), second.clone()]);
    let distance = sound_distance(&first, &second, &stats, DistanceWeights::DEFAULT);
    assert_eq!(distance.total, 0.0);
    assert_eq!(distance.tempo, 2.0);
}

#[test]
fn sim_9_rhythm_reorders_two_candidates_the_production_terms_call_a_tie() {
    // One seed and two candidates with exactly the same production. The only
    // thing that separates them is where their movement sits and how often it
    // lands — and that is what decides which one ranks first.
    let seed = moving(features(0, 10.0, 2.0, 4.0, None), 2, 8.0);
    let same_movement = moving(features(1, 14.0, 6.0, 9.0, None), 2, 8.0);
    let other_movement = moving(features(1, 14.0, 6.0, 9.0, None), 18, 1.0);
    let stats = compute_sound_stats(&[seed.clone(), same_movement.clone(), other_movement.clone()]);

    let near = sound_distance(&seed, &same_movement, &stats, DistanceWeights::DEFAULT);
    let far = sound_distance(&seed, &other_movement, &stats, DistanceWeights::DEFAULT);

    assert_eq!(near.band, far.band);
    assert_eq!(near.timbre, far.timbre);
    assert_eq!(near.dynamics, far.dynamics);
    assert_eq!(near.rhythm, 0.0);
    assert!(far.rhythm > 1.0, "{}", far.rhythm);
    assert!(near.total < far.total);

    // Proof that the order came from the rhythm term and nothing else: take
    // its weight away and the two candidates are indistinguishable again.
    let without_rhythm = DistanceWeights {
        rhythm: 0.0,
        ..DistanceWeights::DEFAULT
    };
    assert_eq!(
        sound_distance(&seed, &same_movement, &stats, without_rhythm).total,
        sound_distance(&seed, &other_movement, &stats, without_rhythm).total
    );
}

#[test]
fn sim_9_every_weighting_spends_a_full_share_and_keeps_rhythm_in_it() {
    for weights in [
        DistanceWeights::DEFAULT,
        DistanceWeights::TIMBRE,
        DistanceWeights::DYNAMICS,
    ] {
        let sum = weights.band + weights.timbre + weights.dynamics + weights.rhythm + weights.tempo;
        assert!((sum - 1.0).abs() < 1.0e-6, "{weights:?} sums to {sum}");
        assert!(weights.rhythm > 0.0, "{weights:?} drops rhythm entirely");
    }
    let default = DistanceWeights::DEFAULT;
    assert!(
        default.rhythm >= default.band + default.timbre + default.dynamics,
        "rhythm has to weigh as much as the production half it was missing"
    );
}

#[test]
fn sim_9_tempo_assigns_a_nonzero_weight_only_when_enabled() {
    assert_eq!(DistanceWeights::DEFAULT.tempo, 0.0);
    let enabled = DistanceWeights::DEFAULT.with_tempo(true);
    assert_eq!(enabled.tempo, 0.2);
    assert!(
        (enabled.band + enabled.timbre + enabled.dynamics + enabled.rhythm + enabled.tempo - 1.0)
            .abs()
            < 1.0e-6
    );
}
