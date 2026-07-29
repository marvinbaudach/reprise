use super::*;

#[test]
fn ac_23_cava_bars_cross_the_frame_boundary_without_remapping() {
    let mut bars = [0.0; SPECTRUM_BAND_COUNT];
    for (index, bar) in bars.iter_mut().enumerate() {
        *bar = index as f32 / (SPECTRUM_BAND_COUNT - 1) as f32;
    }

    let frame = SpectrumFrame::from_cava_bars(bars);

    assert_eq!(frame.bands(), &bars);
}

#[test]
fn ac_23_cava_frame_boundary_neutralizes_hostile_values() {
    let mut bars = [0.5; SPECTRUM_BAND_COUNT];
    bars[..5].copy_from_slice(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 2.0]);

    let frame = SpectrumFrame::from_cava_bars(bars);

    assert_eq!(&frame.bands()[..5], &[0.0, 0.0, 0.0, 0.0, 1.0]);
    assert!(frame
        .bands()
        .iter()
        .all(|bar| bar.is_finite() && (0.0..=1.0).contains(bar)));
}

#[test]
fn ac_23_the_frame_carries_the_measured_bass_pressure() {
    let pressure = BassPressure {
        level_dbfs: -12.0,
        baseline_dbfs: -20.5,
        impact: 0.8,
        aura: 0.4,
    };

    let frame =
        SpectrumFrame::from_cava_bars([0.5; SPECTRUM_BAND_COUNT]).with_bass_pressure(pressure);

    assert_eq!(frame.bass_pressure(), pressure);
}

#[test]
fn ac_23_a_frame_without_a_measurement_stays_dark() {
    let frame = SpectrumFrame::from_cava_bars([0.9; SPECTRUM_BAND_COUNT]);

    assert_eq!(frame.bass_pressure().impact, 0.0);
    assert_eq!(frame.bass_pressure().aura, 0.0);
}

#[test]
fn ac_23_the_frame_boundary_neutralizes_hostile_bass_pressure() {
    let frame = SpectrumFrame::from_cava_bars([0.5; SPECTRUM_BAND_COUNT]).with_bass_pressure(
        BassPressure {
            level_dbfs: f32::NAN,
            baseline_dbfs: f32::INFINITY,
            impact: 2.0,
            aura: -1.0,
        },
    );

    let carried = frame.bass_pressure();

    assert!(carried.level_dbfs.is_finite());
    assert!(carried.baseline_dbfs.is_finite());
    assert_eq!((carried.impact, carried.aura), (1.0, 0.0));
}

#[test]
fn ac_23_coalescing_keeps_only_the_latest_cava_frame() {
    let older = SpectrumFrame::from_cava_bars([0.25; SPECTRUM_BAND_COUNT]);
    let latest = SpectrumFrame::from_cava_bars([0.75; SPECTRUM_BAND_COUNT]);

    assert_eq!(older.coalesce_latest(latest), latest);
}
