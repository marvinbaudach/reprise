use crate::sound_rhythm::{derive_rhythm_features, estimate_tempo, RhythmFeatures};
use crate::spectrogram::{TrackSpectrogram, SPECTROGRAM_BAND_COUNT, SPECTROGRAM_FRAME_RATE_HZ};

/// One frame with a single occupied band.
fn frame(active_band: usize, level: u8) -> Vec<u8> {
    let mut cells = vec![0; SPECTROGRAM_BAND_COUNT];
    cells[active_band] = level;
    cells
}

fn spectrogram(frames: impl IntoIterator<Item = Vec<u8>>) -> TrackSpectrogram {
    TrackSpectrogram::from_cells(frames.into_iter().flatten().collect()).unwrap()
}

/// A pulse every `period` frames, starting at the first frame that has a
/// predecessor — so every pulse is a rise the flux can see, and the onset
/// count is exactly the pulse count.
fn pulse_train(band: usize, period: usize, pulses: usize) -> TrackSpectrogram {
    spectrogram((0..=period * pulses).map(|index| {
        frame(
            band,
            if index > 0 && index % period == 0 {
                255
            } else {
                0
            },
        )
    }))
}

fn assert_near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {actual} to be within {tolerance} of {expected}"
    );
}

#[test]
fn rhythm_of_silence_is_a_track_that_does_not_move() {
    let silent = spectrogram((0..120).map(|_| vec![0; SPECTROGRAM_BAND_COUNT]));

    assert_eq!(derive_rhythm_features(&silent), RhythmFeatures::still());
    assert_eq!(estimate_tempo(&silent), None);
}

#[test]
fn rhythm_of_a_steady_tone_is_the_same_as_of_silence() {
    // Loud, but never changing: there is no movement to describe, so a held
    // organ note and a silent track answer the same.
    let steady = derive_rhythm_features(&spectrogram((0..120).map(|_| frame(9, 200))));

    assert_eq!(steady, RhythmFeatures::still());
}

#[test]
fn rhythm_counts_a_known_pulse_train_at_its_own_rate() {
    // 12 pulses, one every 10 frames at 20 fps: a pulse every 0.5 s over
    // exactly 6 s of flux frames.
    let features = derive_rhythm_features(&pulse_train(2, 10, 12));

    assert_eq!(features.onset_rate, 2.0);
    assert!(
        features.pulse_strength > 0.5,
        "a metronomic train must read as metronomic, got {}",
        features.pulse_strength
    );
    assert_near(
        estimate_tempo(&pulse_train(2, 10, 12)).expect("a periodic train has a tempo"),
        f32::from(SPECTROGRAM_FRAME_RATE_HZ as u16) * 60.0 / 10.0,
        0.1,
    );
}

#[test]
fn rhythm_puts_the_flux_in_the_band_that_actually_moves() {
    // Band 1 pulses, band 20 is loud and completely static. All movement is
    // in the low band, so the flux vector points there and nowhere else.
    let moving = spectrogram((0..120).map(|index| {
        let mut cells = vec![0; SPECTROGRAM_BAND_COUNT];
        cells[1] = if index % 2 == 0 { 0 } else { 255 };
        cells[20] = 220;
        cells
    }));

    let features = derive_rhythm_features(&moving);

    assert_near(features.band_flux[1], 1.0, 1.0e-5);
    assert_eq!(features.band_flux[20], 0.0);
    assert_eq!(
        features
            .band_flux
            .iter()
            .filter(|value| **value > 0.0)
            .count(),
        1
    );
}

#[test]
fn rhythm_reports_how_much_and_how_unevenly_a_track_moves() {
    // Alternating full-scale band: every second frame carries the whole
    // dynamic range of the cell scale as a rise, every other frame nothing.
    // Mean and standard deviation of that flux are equal, so the variation is 1.
    let alternating = derive_rhythm_features(&spectrogram(
        (0..121).map(|index| frame(3, if index % 2 == 0 { 0 } else { 255 })),
    ));

    let full_scale_db = 64.0; // -70 dBFS floor to -6 dBFS ceiling
    assert_near(alternating.flux_mean, full_scale_db / 2.0, 0.5);
    assert_near(alternating.flux_variation, 1.0, 0.02);

    // A track whose movement really is spread over every frame: one step up
    // the cell scale per frame, so every frame carries the same rise. It moves
    // far less than the alternating one — the question here is not how much
    // but how evenly, and a constant flux does not vary at all.
    let even = derive_rhythm_features(&spectrogram((0..121).map(|index| frame(3, index as u8))));
    assert_near(even.flux_variation, 0.0, 1.0e-3);
    assert!(
        even.flux_variation < alternating.flux_variation,
        "an even mover must vary less than a stop-start one: {} vs {}",
        even.flux_variation,
        alternating.flux_variation
    );
}

#[test]
fn rhythm_tells_a_ballad_from_a_blast_beat() {
    let ballad = derive_rhythm_features(&pulse_train(2, 20, 6));
    let blast = derive_rhythm_features(&pulse_train(2, 3, 40));

    assert_eq!(ballad.onset_rate, 1.0);
    assert!(
        blast.onset_rate > 5.0,
        "a pulse every 150 ms is more than five onsets a second, got {}",
        blast.onset_rate
    );
}

#[test]
fn rhythm_of_an_unsteady_track_is_less_metronomic_than_a_train() {
    // The same twelve pulses over the same 121 frames, placed irregularly.
    let hits = [7, 13, 26, 31, 49, 58, 61, 77, 88, 95, 103, 118];
    let uneven =
        spectrogram((0..=120).map(|index| frame(2, if hits.contains(&index) { 255 } else { 0 })));

    let train = derive_rhythm_features(&pulse_train(2, 10, 12));
    let irregular = derive_rhythm_features(&uneven);

    assert!(
        irregular.pulse_strength < train.pulse_strength,
        "irregular {} must be less metronomic than a train {}",
        irregular.pulse_strength,
        train.pulse_strength
    );
    // Both still land the same number of onsets: how often a track hits and
    // how regularly it hits are two different questions.
    assert_eq!(irregular.onset_rate, 2.0);
}

#[test]
fn rhythm_of_a_track_without_a_beat_is_not_metronomic_at_all() {
    // Twelve seconds of one slow swell: the bass rises and never repeats, so
    // every lag in the musical window correlates about as well as every other
    // and the best of them is the best of fifteen, not a pulse. Without the
    // significance test this reads as 0.90 — near-perfectly metronomic for a
    // track that has no beat in it.
    let swell = spectrogram((0..240).map(|index| frame(2, (index * 255 / 239) as u8)));
    // … and the other way a peak can be meaningless: bass activity that jumps
    // about at random, where no lag correlates at all.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let noisy = spectrogram((0..240).map(|_| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        frame(2, (state >> 33) as u8)
    }));

    for (name, arrhythmic) in [("a swell", &swell), ("noise", &noisy)] {
        let features = derive_rhythm_features(arrhythmic);
        assert_eq!(
            features.pulse_strength, 0.0,
            "{name} has no periodicity, so it must not read as metronomic"
        );
        assert_eq!(estimate_tempo(arrhythmic), None);
    }
}

#[test]
fn rhythm_needs_two_frames_before_it_can_say_anything() {
    assert_eq!(
        derive_rhythm_features(&TrackSpectrogram::empty()),
        RhythmFeatures::still()
    );
    assert_eq!(
        derive_rhythm_features(&spectrogram([frame(4, 255)])),
        RhythmFeatures::still()
    );
}
