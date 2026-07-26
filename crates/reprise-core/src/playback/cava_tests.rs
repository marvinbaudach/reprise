use super::*;

#[test]
fn processor_supports_two_hundred_fifty_six_bars() {
    let processor = CavaBarProcessor::new(CavaConfig::new(44_100, 256)).unwrap();

    assert_eq!(processor.bar_count(), 256);
    assert_eq!(processor.cutoff_frequencies_hz().len(), 257);
    assert!(processor
        .cutoff_frequencies_hz()
        .windows(2)
        .all(|pair| pair[0].is_finite() && pair[0] < pair[1]));
}

#[test]
fn four_bar_layout_matches_pinned_cava_cutoffs() {
    let processor = CavaBarProcessor::new(CavaConfig::new(44_100, 4)).unwrap();
    let expected = [48.449_707, 193.798_83, 710.595_7, 2_659.350_6, 10_002.173];

    assert_eq!(processor.cutoff_frequencies_hz().len(), expected.len());
    for (actual, expected) in processor
        .cutoff_frequencies_hz()
        .iter()
        .zip(expected.iter())
    {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected} Hz, got {actual} Hz"
        );
    }
}

#[test]
fn odd_sample_rate_preserves_cavas_fractional_nyquist_cutoffs() {
    let processor = CavaBarProcessor::new(CavaConfig::new(44_101, 4)).unwrap();
    let expected = [48.450_806, 193.803_22, 710.611_8, 2_659.411, 10_002.399];

    for (actual, expected) in processor
        .cutoff_frequencies_hz()
        .iter()
        .zip(expected.iter())
    {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected} Hz, got {actual} Hz"
        );
    }
}
