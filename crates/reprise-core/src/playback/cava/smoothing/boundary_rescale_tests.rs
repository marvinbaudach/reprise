// The rule the boundary measurement gained for #1142, at the smoother: with
// nothing on screen the bar history follows the gain down while the window
// fills, and under a shape that is kept it does not. The whole-processor
// behaviour is judged in `playback::boundary_tests`.

use super::*;

const WINDOW_SAMPLES: usize = 8_192;
const SAMPLES: usize = 800;
const SAMPLE_RATE_HZ: u32 = 48_000;
/// Where the first of two frames reads every bar, and how much louder one bar
/// of the second reads.
const QUIET_RAW: f32 = 0.01;
const RISE: f32 = 5.0;

fn smoother() -> Smoother {
    Smoother::new(64, 0.77, 1, WINDOW_SAMPLES)
}

fn frame(smoother: &mut Smoother, bars: [f32; 64]) -> [f32; 64] {
    let mut bars = bars;
    smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
    bars
}

fn carrying(gain: f32) -> Smoother {
    let mut smoother = smoother();
    smoother.adopt_sensitivity(gain);
    smoother.rearm_boundary();
    smoother
}

/// The mean of the bars that do not carry the peak in the second of two frames,
/// the first reading every bar at `QUIET_RAW`, the second one bar `RISE` times
/// louder: the gain falls fivefold between them, and so must the history those
/// bars carry. With it rescaled they read `quiet / RISE * (1 + feedback)`; left
/// in the units of the gain before, `quiet / RISE + feedback * quiet`, where
/// `quiet` is the height the first frame drew them at. Returns the two readings
/// that bracket the cases, and what was drawn.
fn bars_beside_a_rising_peak(smoother: &mut Smoother) -> (f32, f32, f32) {
    let first = frame(smoother, [QUIET_RAW; 64]);
    let quiet = first[1];
    let mut second = [QUIET_RAW; 64];
    second[0] = QUIET_RAW * RISE;
    let drawn = frame(smoother, second);
    let feedback = smoother.integral_feedback(CAVA_REFERENCE_FRAMERATE / smoother.framerate);
    let rescaled = quiet / RISE * (1.0 + feedback);
    let stale = quiet / RISE + feedback * quiet;
    let mean = drawn[1..].iter().sum::<f32>() / 63.0;
    ((rescaled + stale) / 2.0, mean, stale)
}

#[test]
fn with_nothing_on_screen_the_history_follows_the_gain_down() {
    let (halfway, drawn, stale) = bars_beside_a_rising_peak(&mut smoother());

    assert!(
        drawn <= halfway,
        "the bars beside the peak drew {drawn:.3}: the history stayed in the units of the \
         gain before ({stale:.3} if left alone, {halfway:.3} the most it may read)"
    );
}

#[test]
fn a_track_change_does_not_rescale_the_history_the_measurement_replaces_the_gain_under() {
    // A shape is on screen, so the bars it draws must not jump when the gain is
    // replaced: that is the cliff the settle span of the acceptance tests bounds.
    let (halfway, drawn, _) = bars_beside_a_rising_peak(&mut carrying(1_000.0));

    assert!(
        drawn >= halfway,
        "a replaced gain on a track change rescaled the history: the bars drew {drawn:.3}, \
         at least {halfway:.3} wanted"
    );
}

#[test]
fn a_hard_restart_after_a_track_change_rescales_the_history_again() {
    // The desktop stage restarts hard (`reset`) whenever the visualizer was off,
    // whatever the boundary before it was: nothing is on screen then, and the
    // measurement must not remember the shape the track change before it kept.
    let mut smoother = carrying(1_000.0);
    smoother.reset();

    let (halfway, drawn, stale) = bars_beside_a_rising_peak(&mut smoother);

    assert!(
        drawn <= halfway,
        "after a track change and a hard restart the bars beside the peak drew {drawn:.3}: the \
         history stayed in the units of the gain before ({stale:.3} if left alone, {halfway:.3} \
         the most it may read)"
    );
}
