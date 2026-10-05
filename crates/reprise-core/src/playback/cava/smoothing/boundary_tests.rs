// The boundary measurement at the smoother, one rule at a time. The whole-
// processor behaviour across a boundary is judged in `playback::boundary_tests`.

use super::*;
use crate::playback::cava::boundary::{TARGET_HEIGHT, TRACKING_WINDOWS};

const WINDOW_SAMPLES: usize = 8_192;
const SAMPLES: usize = 800;
const SAMPLE_RATE_HZ: u32 = 48_000;
/// Frames in which fewer than a window of samples have arrived.
const FRAMES_BEFORE_THE_WINDOW_IS_FULL: usize = WINDOW_SAMPLES / SAMPLES;
const STEADY_RAW: f32 = 0.05;
/// `cavacore`'s up-step per frame at this cadence is below this, with room.
const ONE_CREEP_STEP: f32 = 1.002;
/// A bar held under the pending ceiling may show up to this much more when the
/// ceiling lifts; a gain ratio applied to its state would show far more.
const CEILING_RELEASE: f32 = 0.1;

fn smoother() -> Smoother {
    Smoother::new(64, 0.77, 1, WINDOW_SAMPLES)
}

fn frame(smoother: &mut Smoother, raw: f32) -> [f32; 64] {
    let mut bars = [raw; 64];
    smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
    bars
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

#[test]
fn until_a_window_of_the_new_stream_is_in_the_gain_holds_and_a_wall_is_scaled_down() {
    let mut smoother = smoother();
    smoother.adopt_sensitivity(5.0);
    smoother.rearm_boundary();

    for index in 0..FRAMES_BEFORE_THE_WINDOW_IS_FULL {
        let bars = frame(&mut smoother, STEADY_RAW * 60.0);

        assert!(
            frame_max(&bars) <= PENDING_CEILING + 1.0e-6,
            "frame {index} rose above the ceiling while waiting: {}",
            frame_max(&bars)
        );
        assert_eq!(smoother.sensitivity, 5.0, "the gain moved while waiting");
    }
}

#[test]
fn a_shape_below_the_wall_level_keeps_its_heights_while_waiting() {
    let mut waiting = smoother();
    waiting.adopt_sensitivity(1.0);
    waiting.rearm_boundary();
    let mut settled = smoother();
    settled.adopt_sensitivity(1.0);

    // Two frames stay under full height; the settled smoother's creep has
    // moved its gain by one 0.1 % step at most.
    for _ in 0..2 {
        let held = frame(&mut waiting, 0.5);
        let creeping = frame(&mut settled, 0.5);
        assert!(
            (held[0] - creeping[0]).abs() <= 0.002,
            "{held:?} against {creeping:?}"
        );
    }
}

#[test]
fn the_first_measurement_lands_the_loudest_bar_at_the_target_height() {
    let mut smoother = smoother();

    let gain = gain_after(
        &mut smoother,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL + 20,
    );
    let landed = frame_max(&frame(&mut smoother, STEADY_RAW));

    assert!(
        gain > 1.0,
        "a quiet stream needs more than the cold gain: {gain}"
    );
    assert!(
        (landed - TARGET_HEIGHT).abs() <= 0.02,
        "the loudest bar landed at {landed:.3}, not {TARGET_HEIGHT}"
    );
}

#[test]
fn a_gain_carried_from_a_louder_song_is_replaced_not_kept() {
    let mut carried = smoother();
    carried.adopt_sensitivity(0.01);
    carried.rearm_boundary();
    let mut fresh = smoother();

    let carried_gain = gain_after(
        &mut carried,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL + 20,
    );
    let fresh_gain = gain_after(
        &mut fresh,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL + 20,
    );

    assert!(
        (carried_gain / fresh_gain - 1.0).abs() <= 0.02,
        "the gain depends on the song before: carried={carried_gain}, fresh={fresh_gain}"
    );
}

#[test]
fn a_louder_bar_while_tracking_lowers_the_gain_at_once_and_a_quieter_one_never_raises_it() {
    let mut smoother = smoother();
    let measured = gain_after(
        &mut smoother,
        STEADY_RAW,
        FRAMES_BEFORE_THE_WINDOW_IS_FULL + 3,
    );

    let after_quiet = gain_after(&mut smoother, STEADY_RAW / 4.0, 2);
    frame(&mut smoother, STEADY_RAW * 2.0);

    assert!(
        after_quiet <= measured * ONE_CREEP_STEP,
        "a quieter bar raised the gain beyond the creep: {after_quiet} from {measured}"
    );
    assert!(
        (smoother.sensitivity / (measured / 2.0) - 1.0).abs() <= 0.02,
        "a bar twice as loud should halve the gain at once: {} from {measured}",
        smoother.sensitivity
    );
}

#[test]
fn after_the_tracking_span_the_creep_decides_and_nothing_jumps() {
    let mut smoother = smoother();
    let settled = gain_after(
        &mut smoother,
        STEADY_RAW,
        TRACKING_WINDOWS * FRAMES_BEFORE_THE_WINDOW_IS_FULL + 20,
    );

    frame(&mut smoother, STEADY_RAW * 4.0);

    assert!(
        smoother.sensitivity >= settled * 0.97,
        "the gain jumped after the tracking span: {} from {settled}",
        smoother.sensitivity
    );
}

#[test]
fn digital_silence_does_not_start_the_measurement() {
    let mut smoother = smoother();
    for _ in 0..100 {
        let mut silence = [0.0; 64];
        smoother.apply(&mut silence, SAMPLES, SAMPLE_RATE_HZ, false);
    }

    assert_eq!(smoother.sensitivity, 1.0, "silence moved the gain");
    for _ in 0..FRAMES_BEFORE_THE_WINDOW_IS_FULL {
        assert!(frame_max(&frame(&mut smoother, 3.0)) <= PENDING_CEILING + 1.0e-6);
    }
    assert_eq!(
        smoother.sensitivity, 1.0,
        "a measurement was taken before a window of sound had arrived"
    );
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

    let mut before_the_jump = f32::MAX;
    for _ in 0..FRAMES_BEFORE_THE_WINDOW_IS_FULL + 6 {
        let mut new_song = [0.0; 64];
        new_song[1] = STEADY_RAW;
        smoother.apply(&mut new_song, SAMPLES, SAMPLE_RATE_HZ, true);
        if smoother.sensitivity > old_gain * 2.0 {
            assert!(
                new_song[0] <= before_the_jump + CEILING_RELEASE,
                "the old song's bar rose from {before_the_jump:.2} to {:.2} when the gain jumped",
                new_song[0]
            );
        } else {
            before_the_jump = new_song[0];
        }
    }

    assert!(
        smoother.sensitivity > old_gain * 2.0,
        "the fixture never raised the gain, so it proved nothing"
    );
}
