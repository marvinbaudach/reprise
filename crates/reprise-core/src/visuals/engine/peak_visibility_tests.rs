use std::time::Duration;

use super::*;

const FRAME_ELAPSED: Duration = Duration::from_micros(16_667);
const CLOSED_FRAMES: usize = 600;
const CONTROL_ARM_TOLERANCE: f32 = 0.05;

fn music_frame(frame: usize) -> SpectrumFrame {
    let time_phase = frame as f32 * 0.045;
    SpectrumFrame::from_cava_bars(std::array::from_fn(|band| {
        let band_phase = band as f32 * 0.31;
        0.05 + 0.85 * (0.5 + 0.5 * (time_phase + band_phase).sin())
    }))
}

fn median(mut values: [f32; SPECTRUM_BAND_COUNT]) -> f32 {
    values.sort_by(f32::total_cmp);
    (values[SPECTRUM_BAND_COUNT / 2 - 1] + values[SPECTRUM_BAND_COUNT / 2]) / 2.0
}

fn largest_peak_gap(engine: &VisualEngine) -> f32 {
    engine
        .bands_peaks
        .iter()
        .zip(engine.bands_current)
        .map(|(peak, current)| peak - current)
        .fold(0.0, f32::max)
}

#[test]
fn ac_23_first_visible_tick_has_no_peak_cap_backlog() {
    let mut hidden = VisualEngine::new();
    hidden.set_playing(true);
    let mut continuously_visible = VisualEngine::new();
    continuously_visible.set_playing(true);
    let mut control_arm_largest_gap = 0.0_f32;

    for frame in 0..CLOSED_FRAMES {
        let sample = music_frame(frame);
        hidden.ingest((&sample, FRAME_ELAPSED));
        continuously_visible.ingest((&sample, FRAME_ELAPSED));
        continuously_visible.advance_by(FRAME_ELAPSED);
        control_arm_largest_gap =
            control_arm_largest_gap.max(largest_peak_gap(&continuously_visible));
    }

    assert!(
        control_arm_largest_gap <= CONTROL_ARM_TOLERANCE,
        "control arm exceeded its {CONTROL_ARM_TOLERANCE:.4} tolerance: {control_arm_largest_gap:.4}"
    );

    hidden.advance_by(FRAME_ELAPSED);
    let first_visible_arm_gap = hidden
        .bands_peaks
        .iter()
        .zip(continuously_visible.bands_peaks)
        .map(|(hidden_peak, visible_peak)| (hidden_peak - visible_peak).abs())
        .fold(0.0, f32::max);

    eprintln!(
        "first-visible arm gap {first_visible_arm_gap:.4}; control max peak-current gap {control_arm_largest_gap:.4}"
    );

    assert!(
        first_visible_arm_gap <= CONTROL_ARM_TOLERANCE,
        "peak caps retained a hidden-tab backlog: hidden median peak {:.4}, current median {:.4}, control median peak {:.4}, first-visible arm gap {:.4}, control max peak-current gap {:.4}",
        median(hidden.bands_peaks),
        median(hidden.bands_current),
        median(continuously_visible.bands_peaks),
        first_visible_arm_gap,
        control_arm_largest_gap,
    );
}

#[test]
fn ac_27_paused_scene_keeps_peak_caps_without_fresh_ingest() {
    let mut engine = VisualEngine::new();
    engine.set_has_track(true);
    engine.set_playing(true);
    engine.ingest((
        &SpectrumFrame::from_cava_bars([0.9; SPECTRUM_BAND_COUNT]),
        FRAME_ELAPSED,
    ));
    engine.ingest((
        &SpectrumFrame::from_cava_bars([0.2; SPECTRUM_BAND_COUNT]),
        FRAME_ELAPSED,
    ));
    engine.set_playing(false);
    let paused_caps = engine.bands_peaks;

    engine.advance_by(Duration::from_secs(10));

    assert_eq!(
        engine.bands_peaks, paused_caps,
        "paused peak caps changed without a fresh audio frame"
    );
}

#[test]
fn ac_27_continuous_motion_ceases_without_a_loaded_track() {
    let mut engine = lively_engine();
    assert!(!engine.tick());
    engine.set_playing(false);

    assert!((0..500).any(|_| engine.tick()));
    assert!(
        engine.bands_peaks.iter().all(|peak| *peak == 0.0),
        "an unloaded settled scene retained stale peak caps"
    );
}

#[test]
fn ac_27_idle_breathing_keeps_a_loaded_track_alive_while_stopped() {
    let mut engine = lively_engine();
    engine.set_has_track(true);
    engine.set_playing(false);

    // The live bars release first; the idle wave takes over and never
    // settles, so the tick loop keeps running.
    for _ in 0..200 {
        assert!(!engine.tick());
    }
    let first = engine.display_bands;
    for _ in 0..30 {
        engine.tick();
    }

    assert!(first.iter().any(|bar| *bar > 0.0));
    assert_ne!(first, engine.display_bands);
}

#[test]
fn ac_27_idle_breathing_stays_a_low_resting_wave() {
    let mut engine = VisualEngine::new();
    engine.set_has_track(true);
    for _ in 0..400 {
        engine.tick();
        assert!(
            engine.display_bands.iter().all(|bar| *bar <= IDLE_PEAK),
            "idle wave must stay below the resting ceiling"
        );
    }
    assert!(engine.bands_peaks.iter().all(|peak| *peak == 0.0));
}

#[test]
fn ac_27_playback_takes_over_from_the_idle_wave_immediately() {
    let mut engine = VisualEngine::new();
    engine.set_has_track(true);
    for _ in 0..200 {
        engine.tick();
    }
    engine.set_playing(true);
    let bars = std::array::from_fn(|index| index as f32 / SPECTRUM_BAND_COUNT as f32);
    engine.ingest((
        &SpectrumFrame::from_cava_bars(bars),
        Duration::from_micros(16_667),
    ));

    assert_eq!(engine.display_bands, bars);
}

#[test]
fn ac_27_disabled_animations_show_the_resting_wave_without_motion() {
    let mut engine = VisualEngine::new();
    engine.set_has_track(true);
    engine.snap_to_static();
    let resting = engine.display_bands;

    assert!(resting.iter().any(|bar| *bar > 0.0));
    assert_eq!(resting, engine.display_bands);
}
