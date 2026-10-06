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
/// These are cavacore outputs for this signal; any deviation means the port drifted.
/// `docs/research/cava-oracle/` regenerates them: harness, pinned fetch, probe results.
fn ac_29_cava_bars_match_the_cavacore_reference_after_calibration() {
    const FRAMES: [usize; 4] = [172, 240, 255, 330];
    // The port estimates its gain from the new audio's level instead of
    // climbing from a cold start (AC-29), so its gain history before this
    // frame legitimately differs from cavacore's. From here both run the same
    // creep, which is what the reference pins. 0.745497722 is cavacore's own
    // gain after this frame (`oracle.c`'s fourth output).
    const CAVACORE_GAIN_FRAME: usize = 100;
    const CAVACORE_GAIN: f32 = 0.745_497_7;
    // cavacore's gain after later frames, so a drift of the creep is caught on
    // its own and not only through the bars it moves (`sens.txt` rows 201 and
    // 301 of the oracle's fourth output). Relative tolerance: both sides round
    // the same decisions, one in `f32`, one in `double`.
    // `sensitivity()` is a debug-build seam, so a release build checks the bars alone.
    #[cfg(debug_assertions)]
    const CAVACORE_GAINS: [(usize, f32); 2] = [(200, 0.774_628_73), (300, 0.806_024_94)];
    #[cfg(debug_assertions)]
    const GAIN_TOLERANCE: f32 = 1.0e-4;
    const REFERENCE: [[f32; 64]; 4] = [
        [
            0.315462, 0.538361, 0.432593, 0.227695, 0.133478, 0.099056, 0.077458, 0.061830,
            0.051015, 0.027866, 0.023555, 0.020010, 0.017583, 0.015646, 0.013820, 0.011710,
            0.010882, 0.009944, 0.008711, 0.007743, 0.006963, 0.006320, 0.005764, 0.005153,
            0.004577, 0.022583, 1.000000, 0.004843, 0.003091, 0.002839, 0.002604, 0.002353,
            0.002143, 0.001937, 0.001762, 0.001596, 0.001455, 0.001322, 0.001207, 0.001090,
            0.000992, 0.000902, 0.000820, 0.000746, 0.000680, 0.000619, 0.000949, 0.450923,
            0.000468, 0.000425, 0.000388, 0.000354, 0.000323, 0.000295, 0.000270, 0.000247,
            0.000226, 0.000207, 0.000190, 0.209014, 0.000162, 0.000150, 0.000139, 0.000129,
        ],
        [
            0.058821, 0.118953, 0.080272, 0.036906, 0.022331, 0.017028, 0.013680, 0.011222,
            0.009442, 0.008849, 0.007267, 0.005997, 0.005171, 0.004654, 0.004204, 0.003551,
            0.003248, 0.002986, 0.002650, 0.002336, 0.002131, 0.001950, 0.001833, 0.001793,
            0.002247, 0.022507, 1.000000, 0.004799, 0.001217, 0.000944, 0.000827, 0.000729,
            0.000656, 0.000589, 0.000534, 0.000482, 0.000439, 0.000399, 0.000364, 0.000328,
            0.000299, 0.000272, 0.000247, 0.000225, 0.000206, 0.000192, 0.000418, 0.123017,
            0.000157, 0.000130, 0.000117, 0.000107, 0.000097, 0.000089, 0.000081, 0.000074,
            0.000068, 0.000063, 0.000060, 0.209097, 0.000067, 0.000046, 0.000042, 0.000039,
        ],
        [
            0.429656, 0.687207, 0.587391, 0.333759, 0.196359, 0.145710, 0.114006, 0.091001,
            0.075086, 0.057741, 0.048798, 0.041457, 0.036423, 0.032416, 0.028631, 0.024266,
            0.022533, 0.020599, 0.018045, 0.016041, 0.014429, 0.013106, 0.011896, 0.010570,
            0.010168, 0.021761, 0.952236, 0.008296, 0.006561, 0.005914, 0.005384, 0.004882,
            0.004438, 0.004014, 0.003651, 0.003307, 0.003013, 0.002738, 0.002500, 0.002259,
            0.002056, 0.001869, 0.001700, 0.001546, 0.001408, 0.001284, 0.001266, 0.453540,
            0.000972, 0.000881, 0.000804, 0.000733, 0.000670, 0.000611, 0.000559, 0.000511,
            0.000468, 0.000429, 0.000394, 0.198836, 0.000337, 0.000310, 0.000287, 0.000268,
        ],
        [
            0.056308, 0.113861, 0.076799, 0.035371, 0.021457, 0.016393, 0.013195, 0.010844,
            0.009135, 0.009009, 0.007397, 0.006102, 0.005260, 0.004732, 0.004278, 0.003618,
            0.003308, 0.003033, 0.002696, 0.002386, 0.002149, 0.001996, 0.001845, 0.001783,
            0.002370, 0.022057, 0.983998, 0.004396, 0.001311, 0.000981, 0.000834, 0.000739,
            0.000666, 0.000599, 0.000543, 0.000491, 0.000447, 0.000405, 0.000370, 0.000334,
            0.000304, 0.000277, 0.000252, 0.000230, 0.000211, 0.000203, 0.000858, 0.408962,
            0.000192, 0.000136, 0.000120, 0.000109, 0.000099, 0.000090, 0.000083, 0.000076,
            0.000069, 0.000064, 0.000061, 0.205451, 0.000067, 0.000046, 0.000043, 0.000040,
        ],
    ];

    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 64)).unwrap();
    let mut bars = [0.0; 64];
    let mut reference_index = 0;

    for frame in 0..360 {
        if frame == CAVACORE_GAIN_FRAME + 1 {
            processor.adopt_sensitivity(CAVACORE_GAIN);
        }
        let chunk: Vec<f32> = (0..735)
            .map(|sample| {
                let n = frame * 735 + sample;
                let t = n as f64 / 44_100.0;
                let kick = (-(t % 0.5) / 0.06).exp();
                (0.45 * kick * (std::f64::consts::TAU * 55.0 * t).sin()
                    + 0.15 * (std::f64::consts::TAU * 440.0 * t).sin()
                    + 0.08
                        * (0.5 + 0.5 * (std::f64::consts::TAU * 1.5 * t).sin())
                        * (std::f64::consts::TAU * 2_500.0 * t).sin()
                    + 0.04 * (std::f64::consts::TAU * 7_000.0 * t).sin()) as f32
            })
            .collect();
        processor.process_into(&chunk, &mut bars);

        #[cfg(debug_assertions)]
        if let Some((_, expected)) = CAVACORE_GAINS.iter().find(|(at, _)| *at == frame) {
            let actual = processor.sensitivity();
            assert!(
                (actual - expected).abs() <= expected * GAIN_TOLERANCE,
                "after frame {frame} the gain is {actual}, cavacore's is {expected}"
            );
        }
        if reference_index < FRAMES.len() && frame == FRAMES[reference_index] {
            for (bar, (&actual, &expected)) in bars
                .iter()
                .zip(REFERENCE[reference_index].iter())
                .enumerate()
            {
                assert!(
                    (actual - expected).abs() <= 2.0e-3,
                    "frame {frame}, bar {bar}: expected {expected}, got {actual}"
                );
            }
            reference_index += 1;
        }
    }

    assert_eq!(reference_index, FRAMES.len());
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

// A constant tone settles in the limit cycle cavacore's creep pins it in: just
// under full height, stepping down 2 % on an overshoot and creeping back up.
// cavacore reached it within 300 chunks by climbing from a cold start; the port
// measures the tone instead, so it is in the cycle by 1500 chunks (about 17 s),
// and what is pinned is the cycle, not the phase it happens to be in.
#[test]
fn autosensitivity_matches_cavas_pinned_two_hundred_hertz_blueprint() {
    const CYCLE_CHUNKS: usize = 100;
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut tone_bar = Vec::new();

    for chunk in 0..1_500 {
        let bars = processor.process(&sine_chunk(200.0, chunk));
        if chunk >= 1_500 - CYCLE_CHUNKS {
            tone_bar.push(bars[2]);
            for (index, other) in [(3, 0.004), (0, 0.0), (1, 0.0), (4, 0.0)] {
                assert!(
                    (bars[index] - other).abs() <= 0.02,
                    "bar {index}: expected {other}, got {}",
                    bars[index]
                );
            }
        }
    }

    let highest = tone_bar.iter().copied().fold(0.0, f32::max);
    let lowest = tone_bar.iter().copied().fold(1.0, f32::min);
    assert!(
        highest >= 0.974 && lowest >= 0.93,
        "the tone left cavacore's limit cycle: {lowest}..{highest}"
    );
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

// Aged and fresh differ only in the framerate estimate the silent chunks moved,
// which the measured gain reads through the integral feedback; inflated gain
// would be orders of magnitude, not a few percent.
#[test]
fn silence_never_inflates_autosensitivity() {
    const FRAMERATE_DRIFT: f32 = 0.08;
    let mut fresh = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let mut aged = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let silence = vec![0.0; 512];

    for _ in 0..2_048 {
        assert!(aged.process(&silence).iter().all(|bar| *bar == 0.0));
    }

    let expected = fresh.process(&sine_chunk(200.0, 0));
    let actual = aged.process(&sine_chunk(200.0, 0));
    for (aged, fresh) in actual.iter().zip(&expected) {
        assert!(
            (aged - fresh).abs() <= fresh * FRAMERATE_DRIFT + 1.0e-4,
            "silence inflated the gain: {actual:?} against {expected:?}"
        );
    }
}

#[test]
fn faint_pcm_above_pcm_silence_still_ages_autosensitivity_into_view() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    // About -90 dBFS: far below the stored spectrogram's absolute floor, but
    // audible material for a renderer whose whole job is to find a gain for
    // whatever is playing. CAVA decides silence per sample, not per window.
    let faint = |chunk| {
        sine_chunk(200.0, chunk)
            .into_iter()
            .map(|sample| sample * 0.0001)
            .collect::<Vec<_>>()
    };
    let mut bars = Vec::new();

    for chunk in 0..300 {
        bars = processor.process(&faint(chunk));
    }

    assert!(
        bars[2] > 0.0,
        "faint playback must still find a gain, got {bars:?}"
    );
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
fn caller_owned_output_matches_the_allocating_compatibility_path() {
    let input = sine_chunk(2_000.0, 0);
    let mut allocating = CavaBarProcessor::new(CavaConfig::new(44_100, 64)).unwrap();
    let mut caller_owned = CavaBarProcessor::new(CavaConfig::new(44_100, 64)).unwrap();
    let expected = allocating.process(&input);
    let mut actual = [f32::NAN; 64];

    caller_owned.process_into(&input, &mut actual);

    assert_eq!(actual.as_slice(), expected);
}

#[test]
fn sub_fft_hops_expose_a_transient_that_one_decoder_sized_block_skips() {
    const DECODER_BLOCK_SIZE: usize = 4_608;
    const DISPLAY_HOP_SIZE: usize = 735;
    const SKIPPED_TRANSIENT: usize = 100;
    const VISIBLE_TRANSIENT: usize = 4_000;

    let skipped = transient_block(DECODER_BLOCK_SIZE, SKIPPED_TRANSIENT);
    let visible = transient_block(DECODER_BLOCK_SIZE, VISIBLE_TRANSIENT);

    assert_eq!(single_block_peak(&skipped), 0.0);
    assert!(hopped_peak(&skipped, DISPLAY_HOP_SIZE) > 0.0);
    assert!(single_block_peak(&visible) > 0.0);
    assert!(hopped_peak(&visible, DISPLAY_HOP_SIZE) > 0.0);
}

#[test]
fn one_call_keeps_every_sample_above_an_eight_thousand_sample_window() {
    assert_one_call_matches_consecutive_hops(44_100, 4_096, 10_000);
}

#[test]
fn one_call_uses_the_fft_hop_for_a_lower_sample_rate() {
    assert_one_call_matches_consecutive_hops(22_050, 2_048, 5_000);
}

#[test]
fn reset_clears_fft_and_smoothing_history() {
    let mut config = CavaConfig::new(44_100, 10);
    config.autosensitivity = 0;
    let mut processor = CavaBarProcessor::new(config).unwrap();
    let mut fresh = CavaBarProcessor::new(config).unwrap();
    for chunk in 0..40 {
        processor.process(&sine_chunk(200.0, chunk));
    }

    processor.reset();

    assert_eq!(
        processor.process(&sine_chunk(2_000.0, 0)),
        fresh.process(&sine_chunk(2_000.0, 0))
    );
}

// Neither reset path carries the settled autosensitivity gain into the next
// stream: another song's loudness says nothing about this one's, so the gain
// is measured again from its audio (AC-29). A gain settled on a faint tone is
// about two orders of magnitude too high for a loud one, which makes the
// carried gain visible as pinned bars. Control arm: a fresh processor fed the
// identical loud chunks.
#[test]
fn ac_29_resets_measure_the_next_streams_level_instead_of_keeping_the_old_gain() {
    const SETTLE_FRAMES: usize = 600;
    const PROBE_FRAMES: usize = 90;
    const FAINT: f32 = 0.002;
    const LOUD: f32 = 0.5;
    const PINNED: f32 = 0.99;

    let config = CavaConfig::new(44_100, 64);
    let mut full_reset = CavaBarProcessor::new(config).unwrap();
    let mut stream_reset = CavaBarProcessor::new(config).unwrap();
    let mut fresh = CavaBarProcessor::new(config).unwrap();
    for frame in 0..SETTLE_FRAMES {
        let chunk = tone_chunk(FAINT, frame);
        full_reset.process(&chunk);
        stream_reset.process(&chunk);
    }

    full_reset.reset();
    stream_reset.reset_stream();

    let mut processors = [&mut full_reset, &mut stream_reset, &mut fresh];
    let mut pinned_frames = [0_usize; 3];
    let mut loudest = [0.0_f32; 3];
    for frame in 0..PROBE_FRAMES {
        let chunk = tone_chunk(LOUD, frame);
        for (index, processor) in processors.iter_mut().enumerate() {
            let bars = processor.process(&chunk);
            pinned_frames[index] += usize::from(bars.iter().any(|bar| *bar >= PINNED));
            loudest[index] = bars.iter().copied().fold(0.0, f32::max);
        }
    }

    let [full, stream, cold] = pinned_frames;
    assert!(
        full <= cold + 5 && stream <= cold + 5,
        "a reset carried the faint tone's gain: pinned frames full={full}, \
         stream={stream}, fresh={cold}"
    );
    assert!(
        (loudest[0] - loudest[2]).abs() <= 0.1 && (loudest[1] - loudest[2]).abs() <= 0.1,
        "a reset left the loud tone at another height than a fresh processor: {loudest:?}"
    );
}

#[test]
// Regression test for the swipe bug this fix addresses: a stream boundary
// used to call the same full `reset()` a track change needs, which zeroed
// the smoother's bar shape along with the FFT input buffer. The very next
// analyzed block therefore reported bars near zero for one frame before
// climbing back up — visible on the phone as the peak caps freezing with no
// bars underneath. `reset_stream()` keeps the smoother's shape so the next
// block instead falls from the previous loud bar through the smoother's
// normal gravity. Control arm: the same fixture through the full `reset()`,
// which does still drop to (near) zero, proving the assertion below actually
// discriminates the two and is not vacuously true.
fn reset_stream_keeps_the_bar_shape_a_full_reset_would_drop() {
    let full_window = 8_192;
    let tone: Vec<f32> = (0..full_window)
        .map(|sample| {
            (std::f32::consts::TAU * 200.0 * sample as f32 / 44_100.0).sin() * (20_000.0 / 65_535.0)
        })
        .collect();
    // A short, quiet follow-up block: on its own it would never build up to
    // the previous loud bar, so keeping close to it can only be explained by
    // the retained smoother shape, not by the new block's own energy.
    let quiet_follow_up = vec![0.0; 512];

    let mut kept_shape = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    let loud_bar = kept_shape.process(&tone)[2];
    kept_shape.reset_stream();
    let after_reset_stream = kept_shape.process(&quiet_follow_up)[2];

    let mut dropped_shape = CavaBarProcessor::new(CavaConfig::new(44_100, 10)).unwrap();
    dropped_shape.process(&tone);
    dropped_shape.reset();
    let after_full_reset = dropped_shape.process(&quiet_follow_up)[2];

    assert!(loud_bar > 0.0, "fixture must actually produce a loud bar");
    assert!(
        after_reset_stream > loud_bar * 0.5,
        "reset_stream should keep the bar shape: loud_bar={loud_bar}, \
         after_reset_stream={after_reset_stream}"
    );
    assert!(
        after_full_reset < loud_bar * 0.5,
        "control arm: a full reset should still drop the bar, got \
         after_full_reset={after_full_reset} against loud_bar={loud_bar}"
    );
}

#[test]
fn reset_stream_clears_the_fft_window_so_a_different_track_does_not_bleed_in() {
    // noise_reduction = 0 disables the smoother's gravity/peak retention (see
    // `gravity_mod` in `Smoother::apply`), so a bar here reflects only the
    // current FFT window's own energy. That isolates the FFT input buffer's
    // continuity from the smoother-shape retention the fix above adds —
    // without this, the retained shape alone would keep the bar high and this
    // test could not tell the two apart.
    let mut config = CavaConfig::new(44_100, 10);
    config.noise_reduction = 0.0;
    let mut processor = CavaBarProcessor::new(config).unwrap();
    for chunk in 0..40 {
        processor.process(&sine_chunk(200.0, chunk));
    }

    processor.reset_stream();
    // A short silent block after `reset_stream` must read as silence: if the
    // FFT window still held the previous track's samples this would instead
    // resonate with them.
    let bars = processor.process(&vec![0.0; 512]);

    assert!(
        bars[2] < 0.05,
        "reset_stream left the previous track's samples in the FFT window: {bars:?}"
    );
}

#[test]
fn hostile_shape_seed_produces_only_finite_bars() {
    let mut processor = CavaBarProcessor::new(CavaConfig::new(44_100, 4)).unwrap();
    processor.seed_shape(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5]);

    let bars = processor.process(&vec![0.0; 512]);

    assert!(bars.iter().all(|bar| bar.is_finite()), "bars: {bars:?}");
}

fn test_transient_processor() -> CavaBarProcessor {
    let mut config = CavaConfig::new(44_100, 8);
    config.low_cutoff_hz = 1_000;
    config.noise_reduction = 0.0;
    config.autosensitivity = 0;
    CavaBarProcessor::new(config).unwrap()
}

fn transient_block(len: usize, transient_at: usize) -> Vec<f32> {
    let mut samples = vec![0.0; len];
    samples[transient_at] = 1.0;
    samples
}

fn single_block_peak(samples: &[f32]) -> f32 {
    test_transient_processor()
        .process(samples)
        .into_iter()
        .fold(0.0, f32::max)
}

fn hopped_peak(samples: &[f32], hop_size: usize) -> f32 {
    let mut processor = test_transient_processor();
    samples
        .chunks(hop_size)
        .flat_map(|hop| processor.process(hop))
        .fold(0.0, f32::max)
}

fn assert_one_call_matches_consecutive_hops(
    sample_rate_hz: u32,
    hop_size: usize,
    sample_count: usize,
) {
    let samples: Vec<f32> = (0..sample_count)
        .map(|sample| {
            let time = sample as f32 / sample_rate_hz as f32;
            ((std::f32::consts::TAU * 80.0 * time).sin() * 0.35)
                + ((std::f32::consts::TAU * 2_000.0 * time).sin() * 0.15)
        })
        .collect();
    let mut one_call = CavaBarProcessor::new(CavaConfig::new(sample_rate_hz, 64)).unwrap();
    let mut consecutive_hops = CavaBarProcessor::new(CavaConfig::new(sample_rate_hz, 64)).unwrap();
    let mut one_call_bars = [f32::NAN; 64];
    let mut hopped_bars = [f32::NAN; 64];

    one_call.process_into(&samples, &mut one_call_bars);
    for hop in samples.chunks(hop_size) {
        consecutive_hops.process_into(hop, &mut hopped_bars);
    }

    assert_eq!(one_call_bars, hopped_bars);
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

fn tone_chunk(amplitude: f32, chunk: usize) -> Vec<f32> {
    const CHUNK_SIZE: usize = 735;
    (0..CHUNK_SIZE)
        .map(|sample| {
            let absolute_sample = chunk * CHUNK_SIZE + sample;
            (std::f32::consts::TAU * 1_000.0 * absolute_sample as f32 / 44_100.0).sin() * amplitude
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
