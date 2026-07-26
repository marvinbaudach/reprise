use super::*;

#[test]
fn ac_21_cava_bars_cross_the_frame_boundary_without_remapping() {
    let mut bars = [0.0; SPECTRUM_BAND_COUNT];
    for (index, bar) in bars.iter_mut().enumerate() {
        *bar = index as f32 / (SPECTRUM_BAND_COUNT - 1) as f32;
    }

    let frame = SpectrumFrame::from_cava_bars(bars);

    assert_eq!(frame.bands(), &bars);
}

#[test]
fn ac_21_cava_frame_boundary_neutralizes_hostile_values() {
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
fn ac_21_coalescing_keeps_only_the_latest_cava_frame() {
    let older = SpectrumFrame::from_cava_bars([0.25; SPECTRUM_BAND_COUNT]);
    let latest = SpectrumFrame::from_cava_bars([0.75; SPECTRUM_BAND_COUNT]);

    assert_eq!(older.coalesce_latest(latest), latest);
}
