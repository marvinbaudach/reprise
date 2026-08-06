use super::*;

#[test]
fn occupied_upper_frequency_uses_the_highest_band_above_the_stored_floor() {
    let mut cells = vec![0; SPECTROGRAM_BAND_COUNT * 2];
    cells[SPECTROGRAM_BAND_COUNT + 11] = 1;
    let spectrogram = TrackSpectrogram::from_cells(cells).unwrap();

    let upper = spectrogram.occupied_upper_hz().unwrap();

    assert!(
        (upper as i64 - 566).abs() <= 2,
        "unexpected upper edge: {upper}"
    );
    assert_eq!(TrackSpectrogram::empty().occupied_upper_hz(), None);
}

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

/// One frame whose energy sits in `band`, everything else silent.
fn frame_at_band(band: usize) -> Vec<u8> {
    let mut cells = vec![0u8; SPECTROGRAM_BAND_COUNT];
    cells[band] = 220;
    cells
}

#[test]
fn the_colour_curve_travels_from_bass_to_treble_across_a_track() {
    // Energy climbs from the lowest band to the highest over the track, so the
    // curve has to run from one end of the axis to the other.
    let cells: Vec<u8> = (0..SPECTROGRAM_BAND_COUNT)
        .flat_map(frame_at_band)
        .collect();
    let spectrogram = TrackSpectrogram::from_cells(cells).unwrap();

    let curve = spectrogram.centroid_curve(SPECTROGRAM_BAND_COUNT);

    assert_eq!(curve.len(), SPECTROGRAM_BAND_COUNT);
    assert!(
        curve[0] < 24,
        "the curve did not start at the bass end: {curve:?}"
    );
    assert!(
        *curve.last().unwrap() > 231,
        "the curve did not reach the treble end: {curve:?}"
    );
    assert!(
        curve.windows(2).all(|pair| pair[0] <= pair[1]),
        "a rising sweep produced a curve that falls: {curve:?}"
    );
}

#[test]
fn a_track_that_holds_one_band_does_not_flicker_across_the_axis() {
    // Without the minimum span, a track with no spectral movement would have
    // its own rounding noise stretched over the whole axis.
    let cells: Vec<u8> = (0..40).flat_map(|_| frame_at_band(9)).collect();
    let spectrogram = TrackSpectrogram::from_cells(cells).unwrap();

    let curve = spectrogram.centroid_curve(20);

    let spread = curve.iter().max().unwrap() - curve.iter().min().unwrap();
    assert_eq!(spread, 0, "a held band moved on the axis: {curve:?}");
}

#[test]
fn silence_carries_the_last_colour_rather_than_jumping() {
    let mut cells: Vec<u8> = (0..4).flat_map(|_| frame_at_band(4)).collect();
    cells.extend(std::iter::repeat_n(0u8, SPECTROGRAM_BAND_COUNT * 4));
    cells.extend((0..4).flat_map(|_| frame_at_band(18)));
    let spectrogram = TrackSpectrogram::from_cells(cells).unwrap();

    let curve = spectrogram.centroid_curve(12);

    // The silent middle repeats the colour that led into it; it never becomes
    // a statement about frequency of its own.
    assert_eq!(curve[4], curve[3]);
    assert_eq!(curve[5], curve[3]);
}

#[test]
fn an_empty_spectrogram_yields_no_curve() {
    assert!(TrackSpectrogram::empty().centroid_curve(100).is_empty());
    let spectrogram = TrackSpectrogram::from_cells(frame_at_band(3)).unwrap();
    assert!(spectrogram.centroid_curve(0).is_empty());
}
