// How long silence may keep a boundary measurement restarting, and what an empty
// chunk does to a settled one, at the estimator itself. The same rules at the
// whole processor are judged in `playback::boundary_gated_tests`.

use crate::playback::cava::boundary::{
    BoundaryEstimator, Step, MEASURING_CAP_WINDOWS, TARGET_HEIGHT,
};

const WINDOW: usize = 8_192;
/// A quarter of a window: four hops of signal fill it.
const HOP: usize = WINDOW / 4;
const CAP: usize = MEASURING_CAP_WINDOWS * WINDOW;
const LOUD_BAR: f32 = 0.5;
const QUIET_BAR: f32 = 0.1;
/// What an estimator with nothing on screen asks for while the loudest bar it
/// has seen is `peak`.
fn measure_for(peak: f32) -> Step {
    Step::Measure {
        sensitivity: TARGET_HEIGHT / peak,
        rescales_history: true,
    }
}

fn estimator(keeps_shape: bool) -> BoundaryEstimator {
    let mut estimator = BoundaryEstimator::new(WINDOW);
    estimator.arm(keeps_shape);
    estimator
}

fn signal(estimator: &mut BoundaryEstimator, samples: usize, bar: f32) -> Step {
    estimator.advance(samples, true, bar, 0.0, 1.0)
}

fn silence(estimator: &mut BoundaryEstimator, samples: usize) -> Step {
    estimator.advance(samples, false, 0.0, 0.0, 1.0)
}

/// Feeds `samples` of audio of which only a single sample carries signal, so
/// that no silence in it is as long as a window (which would be a break) and
/// the count of the cap advances by exactly `samples`.
fn filler(estimator: &mut BoundaryEstimator, samples: usize) {
    let mut left = samples;
    while left > 0 {
        let silent = (left - 1).min(WINDOW - 1);
        if silent > 0 {
            silence(estimator, silent);
        }
        signal(estimator, 1, QUIET_BAR);
        left -= silent + 1;
    }
}

/// A hop of signal, a silent stretch of `gap` samples, then three hops more:
/// four hops of signal in all, which is a full window unless the gap restarted
/// the count. The first hop is the first signal, so the gap ends `HOP + gap`
/// samples into the count of the cap. Returns whether the measurement is still
/// collecting.
fn collecting_after_a_short_gap(estimator: &mut BoundaryEstimator, gap: usize) -> bool {
    signal(estimator, HOP, LOUD_BAR);
    silence(estimator, gap);
    for _ in 0..3 {
        signal(estimator, HOP, LOUD_BAR);
    }
    estimator.is_collecting()
}

/// The same with a gap of one hop that ends `gap_ends_at` samples after the
/// first signal: the time before it is filler, so that nothing in it is a
/// break. Returns whether the measurement is still collecting.
fn collecting_after_a_gap_ending_at(estimator: &mut BoundaryEstimator, gap_ends_at: usize) -> bool {
    signal(estimator, HOP, QUIET_BAR);
    filler(estimator, gap_ends_at - 3 * HOP);
    signal(estimator, HOP, LOUD_BAR);
    silence(estimator, HOP);
    for _ in 0..3 {
        signal(estimator, HOP, LOUD_BAR);
    }
    estimator.is_collecting()
}

/// Lets a measurement gather the cap and a little more, a hop of signal first.
fn use_up_the_cap(estimator: &mut BoundaryEstimator) {
    signal(estimator, HOP, QUIET_BAR);
    filler(estimator, CAP);
}

#[test]
fn a_settled_estimator_fed_no_samples_leaves_the_creep_to_cavacore() {
    let mut estimator = estimator(false);
    estimator.settle();

    assert_eq!(estimator.advance(0, false, 0.0, 0.0, 1.0), Step::Done);
    assert_eq!(estimator.advance(0, true, LOUD_BAR, 0.0, 1.0), Step::Done);
}

#[test]
fn an_empty_chunk_holds_a_measurement_that_is_still_collecting() {
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);

        assert_eq!(estimator.advance(0, true, LOUD_BAR, 0.0, 1.0), Step::Hold);
        assert!(estimator.is_collecting());
    }
}

#[test]
fn silence_before_the_cap_restarts_the_measurement() {
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);

        // The gap ends one sample short of the cap.
        let collecting = collecting_after_a_gap_ending_at(&mut estimator, CAP - 1);

        assert!(
            collecting,
            "a gap under the cap (keeps shape {keeps_shape}) did not restart the count"
        );
    }
}

#[test]
fn silence_that_reaches_the_cap_pauses_the_measurement_instead() {
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);

        // The gap ends on the cap.
        let collecting = collecting_after_a_gap_ending_at(&mut estimator, CAP);

        assert!(
            !collecting,
            "a gap at the cap (keeps shape {keeps_shape}) still restarted the count"
        );
    }
}

// The cap itself, written out in samples of the 8192 window: a gap three and a
// half windows after the first signal is still a break in the music, one four
// and a half windows after it is the music's rhythm. A changed constant cannot
// move these.
#[test]
fn a_gap_three_and_a_half_windows_after_the_first_signal_still_restarts() {
    const GAP_ENDS_AT: usize = 28_672;
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);

        assert!(
            collecting_after_a_gap_ending_at(&mut estimator, GAP_ENDS_AT),
            "the cap ran out before {GAP_ENDS_AT} samples (keeps shape {keeps_shape})"
        );
    }
}

#[test]
fn a_gap_four_and_a_half_windows_after_the_first_signal_pauses() {
    const GAP_ENDS_AT: usize = 36_864;
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);

        assert!(
            !collecting_after_a_gap_ending_at(&mut estimator, GAP_ENDS_AT),
            "the cap outlasted {GAP_ENDS_AT} samples (keeps shape {keeps_shape})"
        );
    }
}

#[test]
fn past_the_cap_silence_holds_the_gain_and_keeps_the_loudest_bar() {
    let mut estimator = estimator(false);
    use_up_the_cap(&mut estimator);

    assert_eq!(signal(&mut estimator, HOP, LOUD_BAR), measure_for(LOUD_BAR));
    assert_eq!(silence(&mut estimator, HOP), Step::Hold);
    // A quieter bar after the gap does not replace the one before it.
    assert_eq!(
        signal(&mut estimator, HOP, QUIET_BAR),
        measure_for(LOUD_BAR)
    );
}

#[test]
fn a_new_boundary_counts_its_own_cap() {
    for keeps_shape in [false, true] {
        let mut estimator = estimator(false);
        use_up_the_cap(&mut estimator);
        estimator.arm(keeps_shape);

        let collecting = collecting_after_a_short_gap(&mut estimator, HOP);

        assert!(
            collecting,
            "the cap of the boundary before was carried into this one (keeps shape {keeps_shape})"
        );
    }
}

/// What the estimator answers hop by hop to music whose every `every`-th hop is
/// digital silence (`0` for none), after a lead-in of `lead_in` samples of it.
fn answers_to_music(keeps_shape: bool, lead_in: usize, every: usize) -> Vec<(Step, bool)> {
    const MUSIC_SAMPLES: usize = 735;
    const MUSIC_HOPS: usize = 200;
    let mut estimator = estimator(keeps_shape);
    for _ in 0..lead_in / MUSIC_SAMPLES {
        silence(&mut estimator, MUSIC_SAMPLES);
    }
    (0..MUSIC_HOPS)
        .map(|hop| {
            let step = if every > 0 && hop % every == every - 1 {
                silence(&mut estimator, MUSIC_SAMPLES)
            } else {
                let bar = 0.2 + 0.03 * (hop % 7) as f32;
                signal(&mut estimator, MUSIC_SAMPLES, bar)
            };
            (step, estimator.is_collecting())
        })
        .collect()
}

#[test]
fn a_long_lead_in_of_digital_silence_does_not_use_up_the_cap() {
    // Five seconds at 48 kHz, written out: far longer than any cap of a few
    // windows, and longer than the 1.8 s the longest of 1861 real tracks opens
    // with.
    const LEAD_IN: usize = 240_000;
    const NO_GAPS: usize = 0;
    const GAP_EVERY_TENTH_HOP: usize = 10;
    for keeps_shape in [false, true] {
        for every in [NO_GAPS, GAP_EVERY_TENTH_HOP] {
            assert_eq!(
                answers_to_music(keeps_shape, LEAD_IN, every),
                answers_to_music(keeps_shape, 0, every),
                "a lead-in changed the measurement of music with a gap every {every} hops \
                 (keeps shape {keeps_shape})"
            );
        }
    }
}

/// The hop, counting from 1, on which the measurement of music with one silent
/// hop of 735 samples in ten ends at 44.1 kHz: the window is 8192 samples and
/// the cap four of them. The gap on the 50th hop is the first to reach the cap,
/// and the signal of the nine hops before it, 6615 samples, is kept, so three
/// more hops fill the window. Written out, so that nothing that moves the end by
/// a hop can pass.
const GATED_MEASUREMENT_ENDS_ON_HOP: usize = 53;

fn hops_until_the_measurement_ends(estimator: &mut BoundaryEstimator, wall_gain: f32) -> usize {
    const HOPS_BETWEEN_GAPS: usize = 10;
    const HOP_SAMPLES: usize = 735;
    let mut hops = 0;
    while estimator.is_collecting() && hops < 1_000 {
        if hops % HOPS_BETWEEN_GAPS == HOPS_BETWEEN_GAPS - 1 {
            silence(estimator, HOP_SAMPLES);
        } else {
            estimator.advance(HOP_SAMPLES, true, LOUD_BAR, 0.0, wall_gain);
        }
        hops += 1;
    }
    hops
}

#[test]
fn a_measurement_of_music_with_a_silent_hop_in_ten_ends_on_the_fifty_third_hop() {
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);

        assert_eq!(
            hops_until_the_measurement_ends(&mut estimator, 1.0),
            GATED_MEASUREMENT_ENDS_ON_HOP,
            "(keeps shape {keeps_shape})"
        );
    }
}

#[test]
fn a_carried_gain_that_draws_the_music_as_a_wall_does_not_restart_the_cap() {
    // The carried gain is so high that the first hop shows it to be a wall: the
    // measurement takes over at once, and must end where a fresh one does.
    const WALL_GAIN: f32 = 1_000.0;
    let mut estimator = estimator(true);

    assert_eq!(
        hops_until_the_measurement_ends(&mut estimator, WALL_GAIN),
        GATED_MEASUREMENT_ENDS_ON_HOP
    );
}

#[test]
fn a_wall_past_the_cap_leaves_the_cap_in_force() {
    // Past the cap the carried gain, which has passed for the music so far,
    // turns out to draw a louder bar as a wall. A gap after that must still
    // pause the measurement, not restart it.
    const CARRIED_GAIN: f32 = 100.0;
    let mut estimator = estimator(true);
    signal(&mut estimator, HOP, QUIET_BAR);
    filler(&mut estimator, CAP);

    estimator.advance(HOP, true, LOUD_BAR, 0.0, CARRIED_GAIN);
    silence(&mut estimator, HOP);
    for _ in 0..3 {
        signal(&mut estimator, HOP, LOUD_BAR);
    }

    assert!(
        !estimator.is_collecting(),
        "the wall started the cap over, and the gap after it restarted the window"
    );
}

#[test]
fn the_time_spent_collecting_is_not_charged_to_the_span_of_braking() {
    // Written out, as the span is a rule of AC-29: a frame the gain would draw at
    // 1.3 times full height is braked in the first half second after the window.
    const EARLY_BRAKE_LEVEL: f32 = 1.3;
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);
        // Four windows of collecting, most of it a pause, and then the window of
        // signal.
        signal(&mut estimator, HOP, LOUD_BAR);
        filler(&mut estimator, 4 * WINDOW);
        for _ in 0..4 {
            signal(&mut estimator, HOP, LOUD_BAR);
        }
        assert!(!estimator.is_collecting());

        let step = signal(&mut estimator, HOP, LOUD_BAR);

        assert_eq!(
            step,
            Step::Brake {
                trigger: EARLY_BRAKE_LEVEL / LOUD_BAR,
                target: TARGET_HEIGHT / LOUD_BAR,
            },
            "the time spent collecting used up the early braking (keeps shape {keeps_shape})"
        );
    }
}

// A silence of a whole window is a break whenever it comes: nothing of what was
// gathered before it is in the FFT window any more.
#[test]
fn a_break_after_the_cap_restarts_the_measurement() {
    const TWO_SECONDS_AT_48_KHZ: usize = 96_000;
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);
        // Loud audio just under the cap, a gap that reaches it, and then a break.
        signal(&mut estimator, HOP, QUIET_BAR);
        filler(&mut estimator, CAP - 3 * HOP);
        signal(&mut estimator, HOP, LOUD_BAR);
        signal(&mut estimator, HOP, LOUD_BAR);
        silence(&mut estimator, HOP);
        silence(&mut estimator, TWO_SECONDS_AT_48_KHZ);

        // What comes after it is a new measurement: a window of its own to
        // fill, and its own, quiet level to find.
        for _ in 0..3 {
            signal(&mut estimator, HOP, QUIET_BAR);
        }
        assert!(
            estimator.is_collecting(),
            "the signal from before the break filled the window (keeps shape {keeps_shape})"
        );
        let found = signal(&mut estimator, HOP, QUIET_BAR);
        let expected_step = if keeps_shape {
            Step::Goal(TARGET_HEIGHT / QUIET_BAR)
        } else {
            measure_for(QUIET_BAR)
        };
        assert_eq!(
            found, expected_step,
            "the loud bar from before the break outlived it (keeps shape {keeps_shape})"
        );
    }
}

#[test]
fn a_break_does_not_take_back_what_the_cap_has_counted() {
    const HOPS_BETWEEN_GAPS: usize = 10;
    const HOP_SAMPLES: usize = 735;
    // Written out: nine hops of signal, a gap that pauses, three more.
    const ENDS_ON_HOP: usize = 13;
    const TWO_SECONDS_AT_48_KHZ: usize = 96_000;
    for keeps_shape in [false, true] {
        let mut estimator = estimator(keeps_shape);
        use_up_the_cap(&mut estimator);
        silence(&mut estimator, TWO_SECONDS_AT_48_KHZ);

        let mut hops = 0;
        while estimator.is_collecting() && hops < 1_000 {
            if hops % HOPS_BETWEEN_GAPS == HOPS_BETWEEN_GAPS - 1 {
                silence(&mut estimator, HOP_SAMPLES);
            } else {
                signal(&mut estimator, HOP_SAMPLES, LOUD_BAR);
            }
            hops += 1;
        }

        assert_eq!(
            hops, ENDS_ON_HOP,
            "gated music after a break did not pause at once (keeps shape {keeps_shape})"
        );
    }
}

/// Two hops of signal and a silence of `gap` samples, `cycles` times, on a
/// measurement that is past its cap. Returns whether it is still collecting.
fn collecting_after_cycles(keeps_shape: bool, gap: usize, cycles: usize) -> bool {
    let mut estimator = estimator(keeps_shape);
    use_up_the_cap(&mut estimator);
    for _ in 0..cycles {
        signal(&mut estimator, HOP, LOUD_BAR);
        signal(&mut estimator, HOP, LOUD_BAR);
        silence(&mut estimator, gap);
    }
    estimator.is_collecting()
}

#[test]
fn gaps_just_short_of_a_window_still_pause_the_measurement() {
    // Half a window of signal between gaps of 8191 samples: the second cycle
    // completes the window, because no gap is a break.
    for keeps_shape in [false, true] {
        assert!(
            !collecting_after_cycles(keeps_shape, 8_191, 2),
            "a gap one sample short of a window restarted the measurement (keeps shape {keeps_shape})"
        );
    }
}

#[test]
fn stretches_of_signal_shorter_than_a_window_between_breaks_never_finish() {
    // The one pattern that can keep a measurement going for good, and it is
    // meant to: half a window of signal and then a window of silence is audio
    // that is three quarters silence, which the restart is for, and each stretch
    // after the silence starts the FFT window over.
    for keeps_shape in [false, true] {
        assert!(
            collecting_after_cycles(keeps_shape, 8_192, 50),
            "a silence of a whole window did not restart the measurement (keeps shape {keeps_shape})"
        );
    }
}
