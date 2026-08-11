use crate::playback::SPECTRUM_BAND_COUNT;

use super::spectrum_frame_from_bands;

#[test]
fn constant_spectrogram_bands_stay_constant_at_engine_width() {
    let frame = spectrum_frame_from_bands(&[0.42; 24]);

    assert_eq!(frame.bands(), &[0.42; SPECTRUM_BAND_COUNT]);
}

#[test]
fn monotone_spectrogram_ramp_stays_monotone_and_keeps_its_edges() {
    let input: Vec<f32> = (0..24).map(|index| 0.1 + index as f32 / 30.0).collect();

    let frame = spectrum_frame_from_bands(&input);
    let output = frame.bands();

    assert_eq!(output.first(), input.first());
    assert_eq!(output.last(), input.last());
    assert!(output.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(output.iter().all(|value| (0.1..=input[23]).contains(value)));
}

#[test]
fn hostile_and_empty_inputs_never_leave_non_finite_values() {
    for input in [
        Vec::new(),
        vec![f32::NAN],
        vec![f32::NEG_INFINITY, -1.0, 0.5, 2.0, f32::INFINITY],
    ] {
        let frame = spectrum_frame_from_bands(&input);
        assert!(frame
            .bands()
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value)));
        let pressure = frame.bass_pressure();
        assert!(pressure.level_dbfs.is_finite());
        assert!(pressure.baseline_dbfs.is_finite());
        assert!([
            pressure.impact,
            pressure.aura,
            pressure.kick,
            pressure.pressure,
        ]
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value)));
    }
}
