use super::*;

#[test]
fn one_kilohertz_quarter_scale_tone_lands_in_its_absolute_level_band() {
    let samples = (0..SPECTROGRAM_SAMPLE_RATE_HZ)
        .map(|sample| {
            let phase =
                std::f32::consts::TAU * 1_000.0 * sample as f32 / SPECTROGRAM_SAMPLE_RATE_HZ as f32;
            phase.sin() * 0.25
        })
        .collect::<Vec<_>>();
    let mut accumulator = SpectrogramAccumulator::new();
    accumulator.push(&samples);

    let spectrogram = accumulator.finish();
    let frame = spectrogram.frame(spectrogram.frame_count() - 1).unwrap();
    let peak = frame
        .iter()
        .enumerate()
        .max_by_key(|(_, level)| *level)
        .map(|(index, level)| (index, *level))
        .unwrap();

    // Band 14 spans about 987..1305 Hz. A 0.25-peak sine is -15.05 dBFS
    // in RMS terms, which maps to 219 in the shared -70..-6 dBFS window.
    assert_eq!(peak.0, 14);
    assert!((216..=222).contains(&peak.1), "unexpected level {peak:?}");
}

#[test]
fn lowest_log_band_is_measured_on_the_long_fft_grid() {
    let frequency_hz = 23.4375;
    let samples = (0..SPECTROGRAM_SAMPLE_RATE_HZ)
        .map(|sample| {
            let phase = std::f32::consts::TAU * frequency_hz * sample as f32
                / SPECTROGRAM_SAMPLE_RATE_HZ as f32;
            phase.sin() * 0.5
        })
        .collect::<Vec<_>>();
    let mut accumulator = SpectrogramAccumulator::new();
    accumulator.push(&samples);

    let spectrogram = accumulator.finish();
    let frame = spectrogram.frame(spectrogram.frame_count() - 1).unwrap();
    let peak_band = frame
        .iter()
        .enumerate()
        .max_by_key(|(_, level)| *level)
        .map(|(index, _)| index)
        .unwrap();

    assert_eq!(peak_band, 0, "low-frequency frame was {frame:?}");
    assert!(
        frame[0] > frame[1].saturating_add(20),
        "band 0 was not resolved from band 1: {frame:?}"
    );
}

#[test]
fn absolute_scale_keeps_twenty_decibels_of_track_loudness_difference() {
    let level_at = |amplitude: f32| {
        let mut accumulator = SpectrogramAccumulator::new();
        let samples = (0..SPECTROGRAM_SAMPLE_RATE_HZ)
            .map(|sample| {
                let phase = std::f32::consts::TAU * 1_000.0 * sample as f32
                    / SPECTROGRAM_SAMPLE_RATE_HZ as f32;
                phase.sin() * amplitude
            })
            .collect::<Vec<_>>();
        accumulator.push(&samples);
        let spectrogram = accumulator.finish();
        spectrogram.frame(spectrogram.frame_count() - 1).unwrap()[14]
    };

    let loud = level_at(0.5);
    let quiet = level_at(0.05);

    assert!(loud > quiet, "loud={loud}, quiet={quiet}");
    assert!(
        (77..=82).contains(&(loud - quiet)),
        "20 dB should span about 80 bytes: loud={loud}, quiet={quiet}"
    );
}

#[test]
fn a_partial_final_interval_is_computed_not_mistaken_for_empty_audio() {
    let mut accumulator = SpectrogramAccumulator::new();
    accumulator.push(&[0.5]);

    assert_eq!(accumulator.finish().frame_count(), 1);
}
