// The boundary measurement at the smoother, one rule at a time. The whole-
// processor behaviour across a boundary is judged in `playback::boundary_tests`.

use super::*;
use crate::playback::cava::boundary::{BRAKE_LEVEL, BRAKING_WINDOWS, CARRY_BAND, TARGET_HEIGHT};

const WINDOW_SAMPLES: usize = 8_192;
const SAMPLES: usize = 800;
const SAMPLE_RATE_HZ: u32 = 48_000;
/// Frames in which fewer than a window of samples have arrived.
const FRAMES_BEFORE_THE_WINDOW_IS_FULL: usize = WINDOW_SAMPLES / SAMPLES;
const STEADY_RAW: f32 = 0.05;
/// `cavacore`'s up-step per frame at this cadence is below this, with room.
const ONE_CREEP_STEP: f32 = 1.002;
/// Frames the braking span lasts at this cadence.
const BRAKING_FRAMES: usize = BRAKING_WINDOWS * WINDOW_SAMPLES / SAMPLES;

fn smoother() -> Smoother {
    Smoother::new(64, 0.77, 1, WINDOW_SAMPLES)
}

fn frame(smoother: &mut Smoother, raw: f32) -> [f32; 64] {
    let mut bars = [raw; 64];
    smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
    bars
}

fn silent_frame(smoother: &mut Smoother) {
    let mut bars = [0.0; 64];
    smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, false);
}

fn frame_max(bars: &[f32]) -> f32 {
    bars.iter().copied().fold(0.0, f32::max)
}

/// Runs `frames` frames of `raw`, then returns the smoother's gain.
fn gain_after(smoother: &mut Smoother, raw: f32, frames: usize) -> f32 {
    for _ in 0..frames {
        frame(smoother, raw);
    }
    smoother.sensitivity
}

/// A smoother whose measurement is over and whose braking span has just begun.
fn measured() -> Smoother {
    let mut smoother = smoother();
    gain_after(
        &mut smoother,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL + 3,
    );
    smoother
}

#[test]
fn the_window_fill_ramp_rises_to_the_target_without_swelling_past_it() {
    let mut smoother = smoother();

    let heights: Vec<f32> = (0..FRAMES_BEFORE_THE_WINDOW_IS_FULL + 8)
        .map(|_| frame_max(&frame(&mut smoother, STEADY_RAW)))
        .collect();

    for pair in heights.windows(2) {
        assert!(
            pair[1] >= pair[0] - 1.0e-4,
            "the ramp fell back: {heights:.3?}"
        );
    }
    assert!(
        heights.iter().all(|height| *height <= TARGET_HEIGHT + 0.03),
        "the ramp swelled past the target: {heights:.3?}"
    );
    let last = heights[heights.len() - 1];
    assert!(
        (last - TARGET_HEIGHT).abs() <= 0.03,
        "the ramp ended at {last:.3}, not {TARGET_HEIGHT}"
    );
}

/// The gain a steady `STEADY_RAW` is measured to need.
fn measured_gain() -> f32 {
    let mut fresh = smoother();
    gain_after(&mut fresh, STEADY_RAW, FRAMES_BEFORE_THE_WINDOW_IS_FULL + 1)
}

fn carrying(gain: f32) -> Smoother {
    let mut smoother = smoother();
    smoother.adopt_sensitivity(gain);
    smoother.rearm_boundary();
    smoother
}

#[test]
fn a_carried_gain_within_the_band_is_kept() {
    let needed = measured_gain();
    let carried = needed / (CARRY_BAND * 0.75);
    let mut smoother = carrying(carried);

    let after = gain_after(
        &mut smoother,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL + 20,
    );

    assert!(
        (after / carried - 1.0).abs() <= 0.03,
        "a gain inside the band was replaced: {after} from {carried} (measured {needed})"
    );
}

#[test]
fn a_carried_gain_far_too_low_rises_to_the_measurement_by_bounded_steps() {
    const MOST_A_FRAME: f32 = 1.7;
    let needed = measured_gain();
    let mut smoother = carrying(needed / (CARRY_BAND * 5.0));

    let mut gains = Vec::new();
    for _ in 0..FRAMES_BEFORE_THE_WINDOW_IS_FULL + 12 {
        frame(&mut smoother, STEADY_RAW);
        gains.push(smoother.sensitivity);
    }

    for pair in gains.windows(2) {
        assert!(
            pair[1] <= pair[0] * MOST_A_FRAME,
            "the gain jumped from {} to {}",
            pair[0],
            pair[1]
        );
    }
    let last = gains[gains.len() - 1];
    assert!(
        (last / needed - 1.0).abs() <= 0.05,
        "the gain stopped at {last}, short of the measured {needed}"
    );
}

#[test]
fn a_carried_gain_far_too_high_is_replaced_before_the_window_is_full() {
    let needed = measured_gain();
    let mut smoother = carrying(needed * CARRY_BAND * 20.0);

    let early = gain_after(
        &mut smoother,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL / 2,
    );

    assert!(
        early <= needed * CARRY_BAND * 20.0 / 4.0,
        "the carried gain still draws the new stream as a wall: {early}"
    );
}

#[test]
fn a_frame_far_above_full_height_brakes_the_gain_at_once() {
    let mut smoother = measured();
    let before = smoother.sensitivity;

    let bars = frame(&mut smoother, STEADY_RAW * 4.0 * BRAKE_LEVEL);

    assert!(
        smoother.sensitivity <= before / (4.0 * BRAKE_LEVEL) * 1.05,
        "the gain was not braked: {} from {before}",
        smoother.sensitivity
    );
    assert!(
        frame_max(&bars) < 1.0,
        "the braking frame still hit full height"
    );
}

#[test]
fn a_frame_a_little_above_the_gain_is_left_to_the_creep() {
    let mut smoother = measured();
    let before = smoother.sensitivity;

    frame(&mut smoother, STEADY_RAW * 1.5);

    assert!(
        smoother.sensitivity >= before * 0.97,
        "the gain jumped for a modest overshoot: {} from {before}",
        smoother.sensitivity
    );
}

#[test]
fn the_gain_can_rise_again_while_braking_is_armed() {
    let mut smoother = measured();
    let before = smoother.sensitivity;

    // A quiet stretch: nothing overshoots, so the creep raises the gain.
    let after = gain_after(&mut smoother, STEADY_RAW / 2.0, 30);

    assert!(
        after > before,
        "braking ratcheted the gain: {after} from {before}"
    );
}

#[test]
fn after_the_braking_span_the_creep_decides_and_nothing_jumps() {
    let mut smoother = smoother();
    let settled = gain_after(&mut smoother, STEADY_RAW, BRAKING_FRAMES + 20);

    frame(&mut smoother, STEADY_RAW * 4.0 * BRAKE_LEVEL);

    assert!(
        smoother.sensitivity >= settled * 0.97,
        "the gain jumped after the braking span: {} from {settled}",
        smoother.sensitivity
    );
}

#[test]
fn silence_while_braking_neither_restarts_nor_prolongs_it() {
    let mut with_gaps = measured();
    let mut without = measured();
    // A silent frame every tenth frame for the whole span.
    for index in 0..BRAKING_FRAMES {
        if index % 10 == 9 {
            silent_frame(&mut with_gaps);
        } else {
            frame(&mut with_gaps, STEADY_RAW);
        }
        frame(&mut without, STEADY_RAW);
    }

    // The span is over for both: a surge is the creep's to handle, and no frame
    // after a gap is held or capped.
    let after_a_gap = {
        silent_frame(&mut with_gaps);
        let before = with_gaps.sensitivity;
        frame(&mut with_gaps, STEADY_RAW);
        with_gaps.sensitivity / before
    };
    frame(&mut with_gaps, STEADY_RAW * 4.0 * BRAKE_LEVEL);
    frame(&mut without, STEADY_RAW * 4.0 * BRAKE_LEVEL);

    assert!(
        (0.97..=ONE_CREEP_STEP).contains(&after_a_gap),
        "a frame after a gap moved the gain by {after_a_gap}"
    );
    assert!(
        with_gaps.sensitivity >= 0.5 * without.sensitivity,
        "silence kept the braking going: {} against {}",
        with_gaps.sensitivity,
        without.sensitivity
    );
}

#[test]
fn silence_before_the_window_is_full_restarts_the_measurement() {
    let mut smoother = smoother();
    for _ in 0..FRAMES_BEFORE_THE_WINDOW_IS_FULL - 2 {
        frame(&mut smoother, STEADY_RAW);
    }
    silent_frame(&mut smoother);

    // Two more frames would have completed the window had the gap not
    // restarted it.
    for _ in 0..3 {
        frame(&mut smoother, STEADY_RAW);
        assert!(
            smoother.boundary.is_collecting(),
            "the window was taken as full across a silent gap"
        );
    }
}

#[test]
fn digital_silence_does_not_start_the_measurement() {
    let mut smoother = smoother();
    for _ in 0..100 {
        silent_frame(&mut smoother);
    }

    assert_eq!(smoother.sensitivity, 1.0, "silence moved the gain");
}

#[test]
fn bars_still_falling_from_the_previous_song_keep_their_units_when_the_gain_jumps() {
    // A loud song settles at a low gain with one tall bar; the next is 26 dB
    // quieter and lives in another bar, so while the window fills the tall bar
    // only falls, and the measurement raises the gain about twenty-fold. The
    // fallen bar's gravity state is in screen units already: converting it with
    // the gain's ratio would throw it back up twenty times too tall.
    let mut smoother = smoother();
    for _ in 0..100 {
        let mut old_song = [0.0; 64];
        old_song[0] = 1.0;
        smoother.apply(&mut old_song, SAMPLES, SAMPLE_RATE_HZ, true);
    }
    smoother.rearm_boundary();
    let old_gain = smoother.sensitivity;

    let mut tall_bar = f32::MAX;
    for _ in 0..FRAMES_BEFORE_THE_WINDOW_IS_FULL + 6 {
        let mut new_song = [0.0; 64];
        new_song[1] = STEADY_RAW;
        smoother.apply(&mut new_song, SAMPLES, SAMPLE_RATE_HZ, true);
        assert!(
            new_song[0] <= tall_bar + 1.0e-3,
            "the old song's bar rose from {tall_bar:.2} to {:.2}",
            new_song[0]
        );
        tall_bar = new_song[0];
    }

    assert!(
        smoother.sensitivity > old_gain * 2.0,
        "the fixture never raised the gain, so it proved nothing"
    );
}
