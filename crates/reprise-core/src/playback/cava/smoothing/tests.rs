use super::*;

const SEED_FRAME_SAMPLES: usize = 800;
const SEED_SAMPLE_RATE_HZ: u32 = 48_000;
const SEED_TOLERANCE: f32 = 0.02;
/// The FFT input buffer of a 44.1 or 48 kHz processor: what a boundary
/// estimate waits for.
const WINDOW_SAMPLES: usize = 8_192;
/// The real device cadence: `tick()` runs once per display frame and feeds
/// `apply()` the samples read since the last one, 800 at 48 kHz (60 fps).
/// `update_framerate()` derives `framerate_mod`, which scales every creep step
/// and the gravity term, from exactly these two numbers, so a fixture with
/// other values exercises another regime than the one the bugs were measured in.
const SAMPLES: usize = 800;
const SAMPLE_RATE_HZ: u32 = 48_000;
const QUIET_LEVEL: f32 = 0.10;
const LOUD_LEVEL: f32 = 0.80;
const NEAR_FULL: f32 = 0.95;

fn smoother(autosensitivity: u32) -> Smoother {
    Smoother::new(64, 0.77, autosensitivity, WINDOW_SAMPLES)
}

fn frame(smoother: &mut Smoother, level: f32) -> [f32; 64] {
    let mut bars = [level; 64];
    smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
    bars
}

fn frame_max(bars: &[f32]) -> f32 {
    bars.iter().copied().fold(0.0, f32::max)
}

/// Runs `level` until the boundary measurement is over and the gain has been
/// handed to the creep, then another second.
fn settle(smoother: &mut Smoother, level: f32) {
    for _ in 0..(WINDOW_SAMPLES * 5 / SAMPLES + 60) {
        frame(smoother, level);
    }
}

/// The largest bar a steady `level` draws once settled.
fn plateau(smoother: &mut Smoother, level: f32) -> f32 {
    settle(smoother, level);
    (0..223)
        .map(|_| frame_max(&frame(smoother, level)))
        .fold(0.0, f32::max)
}

#[test]
fn a_rising_signal_from_a_cold_start_never_exposes_clipping() {
    let mut smoother = smoother(1);
    let mut max_mean = 0.0_f32;
    let mut max_near_full = 0;

    for raw_level in [
        0.01, 0.02, 0.04, 0.08, 0.12, 0.16, 0.18, 0.18, 0.18, 0.18, 0.18, 0.18,
    ] {
        let mut bars = [raw_level; 64];
        smoother.apply(&mut bars, 4_096, 44_100, true);
        max_mean = max_mean.max(bars.iter().sum::<f32>() / bars.len() as f32);
        max_near_full = max_near_full.max(bars.iter().filter(|bar| **bar >= NEAR_FULL).count());
    }

    assert!(
        max_mean < NEAR_FULL && max_near_full == 0,
        "cold rising signal saturated: max_mean={max_mean:.3}, max_near_full={max_near_full}"
    );
}

#[test]
fn the_first_frame_after_a_reset_does_not_inflate_a_quiet_signal() {
    let mut smoother = smoother(1);
    let settled_max = plateau(&mut smoother, QUIET_LEVEL);

    smoother.reset();
    let first = frame_max(&frame(&mut smoother, QUIET_LEVEL));

    assert!(
        first <= settled_max * 1.5,
        "first frame drew a quiet signal far above its settled level: \
         first={first:.3}, settled_max={settled_max:.3}"
    );
}

#[test]
fn a_reset_lands_a_steady_signal_at_its_known_plateau_and_never_above() {
    let mut smoother = smoother(1);
    let plateau_max = plateau(&mut smoother, QUIET_LEVEL);

    smoother.reset();

    for index in 0..446 {
        let frame_max = frame_max(&frame(&mut smoother, QUIET_LEVEL));
        assert!(
            frame_max <= plateau_max * 1.05,
            "frame {index} after the reset overshot the known plateau: \
             frame_max={frame_max:.3}, plateau_max={plateau_max:.3}"
        );
    }
}

#[test]
fn a_loud_to_quiet_change_finds_the_quiet_plateau_within_a_second() {
    let mut reference = smoother(1);
    let reference_plateau = plateau(&mut reference, QUIET_LEVEL);
    let mut smoother = smoother(1);
    settle(&mut smoother, LOUD_LEVEL);

    smoother.rearm_boundary();
    for _ in 0..60 {
        frame(&mut smoother, QUIET_LEVEL);
    }
    let found = (0..223)
        .map(|_| frame_max(&frame(&mut smoother, QUIET_LEVEL)))
        .fold(0.0, f32::max);

    assert!(
        (found - reference_plateau).abs() <= reference_plateau * 0.15 + 0.02,
        "the quiet plateau was not found within a second of the change: \
         found={found:.3}, reference_plateau={reference_plateau:.3}"
    );
}

// `seed_shape` takes the bars a viewer saw, which are the smoother's
// post-integral output (`bar + memory * integral_feedback`), while
// `previous`/`peaks` hold the pre-integral bar. Seeding the displayed
// shape straight into them made the next frame come out at about
// 1 / (1 - noise_reduction) times the seed: the whole spectrum jumped,
// clipped, and knocked autosensitivity down.
fn displayed_shape() -> [f32; 8] {
    [0.05, 0.2, 0.35, 0.5, 0.65, 0.8, 0.9, 0.3]
}

fn assert_continues(shape: &[f32], frame: &[f32]) {
    for (index, (seed, drawn)) in shape.iter().zip(frame).enumerate() {
        assert!(
            (drawn - seed).abs() <= seed * SEED_TOLERANCE + 1.0e-4,
            "band {index} did not continue the seeded shape: seed={seed:.4}, \
             drawn={drawn:.4}"
        );
    }
}

#[test]
fn a_seeded_shape_continues_on_the_next_frame_of_steady_input() {
    let shape = displayed_shape();
    let mut smoother = smoother_with_bars(shape.len(), 0.77, 0);
    // A smoother that has been running holds integral memory from its
    // old shape, as the live processor does across a track change.
    for _ in 0..30 {
        let mut earlier = [0.1; 8];
        smoother.apply(&mut earlier, SEED_FRAME_SAMPLES, SEED_SAMPLE_RATE_HZ, true);
    }
    // The raw input that reproduces `shape` at steady state is the
    // pre-integral bar: `shape * (1 - integral_feedback)`.
    let feedback = smoother.integral_feedback(CAVA_REFERENCE_FRAMERATE / smoother.framerate);
    let mut frame = shape.map(|bar| bar * (1.0 - feedback));

    smoother.seed_shape(&shape);
    smoother.apply(&mut frame, SEED_FRAME_SAMPLES, SEED_SAMPLE_RATE_HZ, true);

    assert_continues(&shape, &frame);
}

#[test]
fn a_seeded_shape_falls_from_where_it_stood_when_the_input_drops_away() {
    let shape = displayed_shape();
    let mut smoother = smoother_with_bars(shape.len(), 0.77, 0);
    // Leave `fall` mid-gravity: a seed must restart the fall, or the
    // first frame would already be below the shape the viewer saw.
    for _ in 0..10 {
        let mut loud = [0.8; 8];
        smoother.apply(&mut loud, SEED_FRAME_SAMPLES, SEED_SAMPLE_RATE_HZ, true);
        let mut silent = [0.0; 8];
        smoother.apply(&mut silent, SEED_FRAME_SAMPLES, SEED_SAMPLE_RATE_HZ, true);
    }
    let mut frame = [0.0; 8];

    smoother.seed_shape(&shape);
    smoother.apply(&mut frame, SEED_FRAME_SAMPLES, SEED_SAMPLE_RATE_HZ, false);

    assert_continues(&shape, &frame);
}

#[test]
fn a_hostile_seed_is_clamped_into_the_unit_range() {
    // Autosensitivity must be on: with it off the gain never moves, and
    // `apply`'s own final clamp and non-finite handling would hide a
    // missing seed clamp. An unclamped 7.0 comes out of `apply` far above
    // 1.0, which reads as an overshoot and lowers the gain.
    let mut smoother = smoother_with_bars(4, 0.77, 1);
    let mut frame = [0.0; 4];

    smoother.seed_shape(&[f32::NAN, 7.0, -3.0, f32::INFINITY]);
    smoother.apply(&mut frame, SEED_FRAME_SAMPLES, SEED_SAMPLE_RATE_HZ, false);

    assert!(
        frame.iter().all(|bar| (0.0..=1.0).contains(bar)),
        "{frame:?}"
    );
    assert_eq!(frame[0], 0.0);
    assert_eq!(frame[2], 0.0);
    assert_eq!(
        smoother.sensitivity, 1.0,
        "a hostile seed reached the autosensitivity gain"
    );
}

#[test]
fn disabled_autosensitivity_does_not_apply_initial_headroom() {
    let mut smoother = smoother_with_bars(1, 0.0, 0);
    let mut bars = [1.2];

    smoother.apply(&mut bars, 735, 44_100, true);

    assert_eq!(bars, [1.0]);
}

#[test]
fn ac_29_steady_state_overshoot_clips_only_the_overshooting_band() {
    let mut smoother = smoother_with_bars(2, 0.0, 1);
    smoother.adopt_sensitivity(1.0);

    let steady_sensitivity = smoother.sensitivity;
    let mut steady_overshoot = [1.2 / steady_sensitivity, 0.4 / steady_sensitivity];
    let expected_unscaled_band = steady_overshoot[1] * steady_sensitivity;
    smoother.apply(&mut steady_overshoot, 735, 44_100, true);

    assert_eq!(steady_overshoot[0], 1.0);
    assert_eq!(steady_overshoot[1], expected_unscaled_band);

    let following_sensitivity = smoother.sensitivity;
    let mut following_frame = [0.9 / following_sensitivity, 0.4 / following_sensitivity];
    let expected_following = following_frame.map(|bar| bar * following_sensitivity);
    smoother.apply(&mut following_frame, 735, 44_100, true);

    assert_eq!(following_frame, expected_following);
}

fn smoother_with_bars(bar_count: usize, noise_reduction: f32, autosensitivity: u32) -> Smoother {
    Smoother::new(bar_count, noise_reduction, autosensitivity, WINDOW_SAMPLES)
}
