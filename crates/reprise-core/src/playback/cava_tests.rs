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

#[test]
fn pcm_sines_land_in_the_same_bands_as_cavas_standalone_test() {
    let mut bass = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut mids = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut bass_bars = Vec::new();
    let mut mid_bars = Vec::new();

    for chunk in 0..20 {
        bass_bars = bass.process(&sine_chunk(200.0, chunk));
        mid_bars = mids.process(&sine_chunk(2_000.0, chunk));
    }

    assert_eq!(peak_index(&bass_bars), 2);
    assert_eq!(peak_index(&mid_bars), 6);
    assert!(
        bass_bars[2] > bass_bars[1] * 5.0,
        "bass target={}, neighbor={}",
        bass_bars[2],
        bass_bars[1]
    );
    assert!(
        mid_bars[6] > mid_bars[5] * 5.0,
        "mid target={}, neighbor={}",
        mid_bars[6],
        mid_bars[5]
    );
}

#[test]
fn gravity_keeps_a_peak_alive_then_releases_it_to_zero() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let full_window = 8_192;
    let tone: Vec<f32> = (0..full_window)
        .map(|sample| {
            (std::f32::consts::TAU * 200.0 * sample as f32 / 44_100.0).sin() * (20_000.0 / 65_535.0)
        })
        .collect();
    let silence = vec![0.0; 512];

    let peak = processor.process(&tone)[2];
    let first_release = processor.process(&silence)[2];
    let mut tail = first_release;
    for _ in 0..240 {
        tail = processor.process(&silence)[2];
    }

    assert!(peak > 0.0);
    assert!(
        first_release > peak * 0.5,
        "CAVA gravity should prevent an abrupt drop: peak={peak}, release={first_release}"
    );
    assert!(tail < 0.001, "gravity tail should settle, got {tail}");
}

#[test]
fn autosensitivity_matches_cavas_pinned_two_hundred_hertz_blueprint() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut bars = Vec::new();

    for chunk in 0..300 {
        bars = processor.process(&sine_chunk(200.0, chunk));
    }

    let expected = [0.0, 0.0, 0.994, 0.004, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    for (index, (actual, expected)) in bars.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= 0.02,
            "bar {index}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn maximum_noise_reduction_still_releases_after_silence() {
    let mut config = CavaConfig::new(44_100, 10);
    config.noise_reduction = 1.0;
    let mut processor = CavaBarProcessor::new(config).unwrap();
    let full_window = 8_192;
    let tone: Vec<f32> = (0..full_window)
        .map(|sample| {
            (std::f32::consts::TAU * 200.0 * sample as f32 / 44_100.0).sin() * (20_000.0 / 65_535.0)
        })
        .collect();
    let silence = vec![0.0; 512];

    processor.process(&tone);
    let mut tail = 1.0;
    for _ in 0..800 {
        tail = processor.process(&silence)[2];
    }

    assert!(tail < 0.001, "maximum smoothing must settle, got {tail}");
}

#[test]
fn silence_never_inflates_autosensitivity() {
    let mut fresh = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut aged = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let silence = vec![0.0; 512];

    for _ in 0..2_048 {
        assert!(aged.process(&silence).iter().all(|bar| *bar == 0.0));
    }

    let expected = fresh.process(&sine_chunk(200.0, 0));
    let actual = aged.process(&sine_chunk(200.0, 0));
    assert_eq!(actual, expected);
}

#[test]
fn noise_floor_cuts_subthreshold_fft_leakage() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let whisper: Vec<f32> = sine_chunk(200.0, 0)
        .into_iter()
        .map(|sample| sample * 1.0e-5)
        .collect();

    let bars = processor.process(&whisper);

    assert!(bars.iter().all(|bar| *bar == 0.0));
}

#[test]
fn hostile_pcm_and_high_resolution_always_return_finite_bounded_bars() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 256)).unwrap();
    let hostile = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 10.0, -10.0];

    let bars = processor.process(&hostile.repeat(103));

    assert_eq!(bars.len(), 256);
    assert!(bars
        .iter()
        .all(|bar| bar.is_finite() && (0.0..=1.0).contains(bar)));
}

#[test]
fn reset_restores_a_fresh_processor_state() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut fresh = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    for chunk in 0..40 {
        processor.process(&sine_chunk(200.0, chunk));
    }

    processor.reset();

    assert_eq!(
        processor.process(&sine_chunk(2_000.0, 0)),
        fresh.process(&sine_chunk(2_000.0, 0))
    );
}

fn sine_chunk(frequency_hz: f32, chunk: usize) -> Vec<f32> {
    const CHUNK_SIZE: usize = 512;
    (0..CHUNK_SIZE)
        .map(|sample| {
            let absolute_sample = chunk * CHUNK_SIZE + sample;
            (std::f32::consts::TAU * frequency_hz * absolute_sample as f32 / 44_100.0).sin()
                * (20_000.0 / 65_535.0)
        })
        .collect()
}

fn peak_index(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap()
}
