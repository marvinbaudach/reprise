use crate::playback::SPECTRUM_BAND_COUNT;

use super::spectrogram_frame::band_neighbours;
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

/// The band count is whatever a caller hands over, and the interpolation
/// indexes the slice directly — so every length has to stay in bounds, not
/// only the 24 the mobile analysis happens to carry today. Past f32's
/// exact-integer range the position rounds above the last index instead of
/// landing on it, which read one element past the end.
#[test]
fn every_band_count_reads_inside_the_slice() {
    for len in [
        2usize,
        3,
        23,
        24,
        25,
        64,
        1_000,
        1 << 24,
        20_000_000,
        usize::from(u16::MAX) << 12,
    ] {
        for output_index in [0usize, 1, SPECTRUM_BAND_COUNT / 2, SPECTRUM_BAND_COUNT - 1] {
            let (left, right, fraction) = band_neighbours(len, output_index);
            assert!(
                left < len,
                "left {left} out of {len} bands at bar {output_index}"
            );
            assert!(
                right < len,
                "right {right} out of {len} bands at bar {output_index}"
            );
            assert!(
                fraction.is_finite(),
                "non-finite fraction at bar {output_index}"
            );
        }
    }
}

/// The last bar reads the last band: the clamp must not shift the mapping for
/// the lengths that actually occur.
#[test]
fn the_last_bar_still_lands_on_the_last_band() {
    for len in [2usize, 24, 64] {
        let (left, right, _) = band_neighbours(len, SPECTRUM_BAND_COUNT - 1);
        assert_eq!((left, right), (len - 1, len - 1), "for {len} bands");
    }
}
