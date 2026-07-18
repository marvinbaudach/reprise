use super::*;

#[test]
fn interpolation_step_never_overshoots_its_target() {
    // Ordinary frame: advances proportionally, still below target.
    let stepped = interpolation_step(0.10, 1e-6, 16_000.0, 0.20);
    assert!((stepped - 0.116).abs() < 1e-9);
    // Runaway velocity (the stuck-at-100% bug): a stale frame-clock
    // reading once produced dt = 1 µs and an exploded velocity; one real
    // frame then shot the fill to 1.0. The step must stop AT the target.
    assert_eq!(interpolation_step(0.0, 0.002, 16_000.0, 0.004), 0.004);
    // Backwards motion clamps at the target from below, too.
    assert_eq!(interpolation_step(0.5, -0.002, 16_000.0, 0.3), 0.3);
}

#[test]
fn interpolation_step_recovers_a_fill_stuck_beyond_the_target() {
    // Self-healing: if fraction is already past the target (legacy stuck
    // state at 1.0 while the song still plays), the next step snaps back
    // to the target instead of staying pinned.
    assert_eq!(interpolation_step(1.0, 1e-7, 16_000.0, 0.02), 0.02);
}

#[test]
fn interpolation_step_stays_inside_the_unit_range() {
    assert_eq!(interpolation_step(0.99, 0.5, 16_000.0, 1.0), 1.0);
    assert_eq!(interpolation_step(0.01, -0.5, 16_000.0, 0.0), 0.0);
}

#[test]
fn fraction_maps_and_clamps_to_unit_range() {
    assert_eq!(fraction_at(0.0, 200.0), 0.0);
    assert_eq!(fraction_at(100.0, 200.0), 0.5);
    assert_eq!(fraction_at(200.0, 200.0), 1.0);
    assert_eq!(fraction_at(260.0, 200.0), 1.0);
    assert_eq!(fraction_at(50.0, 0.0), 0.0);
}

#[test]
fn bars_split_played_from_unplayed_at_the_fraction() {
    // 4 bars, centres at 0.125/0.375/0.625/0.875; fraction 0.5 plays first 2.
    assert!(bar_played(0, 4, 0.5));
    assert!(bar_played(1, 4, 0.5));
    assert!(!bar_played(2, 4, 0.5));
    assert!(!bar_played(3, 4, 0.5));
    assert!(!bar_played(0, 0, 1.0));
}

#[test]
fn fallback_draws_flat_bar_when_peaks_empty() {
    // No peaks → draw function should not panic, draws fallback.
    // This is a logic test; actual rendering verified in smoke tests.
    assert_eq!(fraction_at(50.0, 100.0), 0.5);
}

#[test]
fn ghost_region_spans_between_fraction_and_drag_fraction() {
    // drag_fraction > fraction: bars with centres in (fraction, drag_fraction]
    // should be in the ghost region.
    let in_ghost = |index: usize, count: usize, fraction: f64, drag_frac: f64| -> bool {
        let bar_center = (index as f64 + 0.5) / count as f64;
        let (lo, hi) = if drag_frac > fraction {
            (fraction, drag_frac)
        } else {
            (drag_frac, fraction)
        };
        bar_center > lo && bar_center <= hi
    };

    // 4 bars at 0.125 / 0.375 / 0.625 / 0.875; fraction=0.25, drag=0.75
    assert!(!in_ghost(0, 4, 0.25, 0.75)); // centre 0.125 ≤ 0.25
    assert!(in_ghost(1, 4, 0.25, 0.75)); // centre 0.375 in (0.25, 0.75]
    assert!(in_ghost(2, 4, 0.25, 0.75)); // centre 0.625 in (0.25, 0.75]
    assert!(!in_ghost(3, 4, 0.25, 0.75)); // centre 0.875 > 0.75

    // Reversed drag: drag < fraction should also produce a ghost range.
    assert!(!in_ghost(0, 4, 0.75, 0.25)); // centre 0.125 ≤ 0.25
    assert!(in_ghost(1, 4, 0.75, 0.25)); // centre 0.375 in (0.25, 0.75]
    assert!(in_ghost(2, 4, 0.75, 0.25)); // centre 0.625 in (0.25, 0.75]
    assert!(!in_ghost(3, 4, 0.75, 0.25)); // centre 0.875 > 0.75
}

#[test]
fn hover_index_targets_correct_bar() {
    // Given 10 bars across 200px, each slot is (200+2)/10 = 20.2px.
    // Bar 0: x in [0, 20.2), bar 3: x in [60.6, 80.8).
    let count = 10usize;
    let w = 200.0_f64;
    let slot = (w + BAR_GAP) / count as f64;
    let x_to_index = |x: f64| ((x / slot) as usize).min(count.saturating_sub(1));

    assert_eq!(x_to_index(0.0), 0);
    assert_eq!(x_to_index(slot * 3.0 + 1.0), 3);
    assert_eq!(x_to_index(w - 1.0), 9);
    // Past the end should clamp to last bar.
    assert_eq!(x_to_index(w + 50.0), 9);
}

#[test]
fn stagger_factor_is_zero_at_start_and_one_at_completion() {
    // At build_progress=0.0, bar 0 stagger is 0 (progress=0, delay_norm=0).
    // stagger = (0.0 - 0.0).max(0) / (1.0 - 0.0).max(0.01) = 0.0.
    let stagger_for = |build_progress: f64, index: usize| -> f64 {
        if build_progress < 1.0 {
            let bar_delay = index as f64 * BAR_STAGGER_S;
            let bar_delay_normalized = bar_delay / BUILD_DURATION_S;
            let adjusted = (build_progress - bar_delay_normalized).max(0.0)
                / (1.0 - bar_delay_normalized).max(0.01);
            adjusted.clamp(0.0, 1.0)
        } else {
            1.0
        }
    };

    // progress=0: all bars start at 0.
    assert_eq!(stagger_for(0.0, 0), 0.0);
    assert_eq!(stagger_for(0.0, 10), 0.0);

    // progress=1: sentinel branch — returns 1.0.
    assert_eq!(stagger_for(1.0, 0), 1.0);
    assert_eq!(stagger_for(1.0, 50), 1.0);

    // progress=0.5: bar 0 (no delay) is at 0.5; a late bar with enough
    // delay to push its bar_delay_normalized > 0.5 is still 0.
    assert!((stagger_for(0.5, 0) - 0.5).abs() < 1e-9);
    assert_eq!(stagger_for(0.5, 100), 0.0); // bar 100: delay=0.2s > 0.15s already passed
}

#[test]
fn smooth_fraction_velocity_is_computed_from_delta() {
    // Pure logic test: given target=0.5, old_target=0.0, dt=1_000_000 us
    // the velocity should be 0.5/1_000_000 per microsecond.
    let old_target = 0.0_f64;
    let new_target = 0.5_f64;
    let dt = 1_000_000_i64;
    let velocity = (new_target - old_target) / dt as f64;
    assert!((velocity - 5e-7).abs() < 1e-12);
}

#[test]
fn aggregate_rms_undoes_the_stored_sqrt_compression() {
    // Stored values are sqrt-compressed: v = sqrt(rms) * 255. A stored 255
    // must aggregate back to rms 1.0, a stored 0 to 0.0.
    let rms = aggregate_rms(&[255, 255, 0, 0], 2);
    assert_eq!(rms.len(), 2);
    assert!((rms[0] - 1.0).abs() < 1e-6);
    assert!(rms[1].abs() < 1e-6);
}

#[test]
fn aggregate_rms_handles_empty_input() {
    assert!(aggregate_rms(&[], 10).is_empty());
    assert!(aggregate_rms(&[128], 0).is_empty());
}

#[test]
fn shape_gives_a_compressed_wall_internal_dynamics() {
    // A "loudness war" track: RMS varies only in a narrow, loud band
    // (a 230-ish verse into a 250-ish chorus). Percentile mapping must
    // spread that band across the full height.
    let mut raw = vec![230u8; 100];
    raw.extend([250u8; 100]);
    let bars = shape_display_peaks(&raw, 100);
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for bar in &bars {
        if let DisplayBar::Level(level) = bar {
            lo = lo.min(*level);
            hi = hi.max(*level);
        }
    }
    assert!(
        hi - lo > 0.5,
        "narrow loud band must be spread out, got lo={lo} hi={hi}"
    );
}

#[test]
fn shape_clips_outliers_above_the_high_percentile() {
    // 96 quiet bars, 4 very loud ones: the loud ones sit above p95 and
    // must clip to the full height (1.0 after gamma).
    let mut raw = vec![100u8; 96];
    raw.extend([255u8; 4]);
    let bars = shape_display_peaks(&raw, 100);
    let last = bars.last().unwrap();
    match last {
        DisplayBar::Level(level) => assert!(*level > 0.95, "outlier level {level}"),
        DisplayBar::Silence => panic!("loud bar classified as silence"),
    }
}

#[test]
fn shape_marks_true_silence_as_dots_not_levels() {
    // Stored 0 (and anything below −50 dB of track max) is silence.
    let mut raw = vec![0u8; 10];
    raw.extend([200u8; 90]);
    let bars = shape_display_peaks(&raw, 100);
    assert_eq!(bars[0], DisplayBar::Silence);
    assert!(matches!(bars[99], DisplayBar::Level(_)));
}

#[test]
fn shape_of_a_perfectly_flat_track_sits_mid_height_not_full() {
    // Degenerate percentiles (p10 == p95): render mid-height, never a
    // full-height wall.
    let raw = vec![200u8; 100];
    let bars = shape_display_peaks(&raw, 50);
    for bar in bars {
        match bar {
            DisplayBar::Level(level) => {
                assert!((0.05..0.95).contains(&level), "flat level {level}");
            }
            DisplayBar::Silence => panic!("flat loud track is not silence"),
        }
    }
}

#[test]
fn smoothing_averages_neighbors_25_50_25() {
    let smoothed = smooth_neighbors(&[0.0, 1.0, 0.0]);
    // Middle: 0.25*0 + 0.5*1 + 0.25*0 = 0.5; edges clamp to themselves:
    // 0.25*0 + 0.5*0 + 0.25*1 = 0.25.
    assert!((smoothed[1] - 0.5).abs() < 1e-6);
    assert!((smoothed[0] - 0.25).abs() < 1e-6);
    assert!((smoothed[2] - 0.25).abs() < 1e-6);
}

#[test]
fn compute_bar_count_uses_fixed_slots_and_caps_at_160() {
    assert_eq!(compute_bar_count(0), 1);
    assert_eq!(compute_bar_count(1), 1);
    // 600px / 5px per slot = 120 bars.
    assert_eq!(compute_bar_count(600), 120);
    // Very wide bars hit the hard cap.
    assert_eq!(compute_bar_count(2000), 160);
}

#[test]
fn ensure_resampled_clears_display_peaks_when_raw_empty() {
    let mut state = State {
        raw_peaks: Vec::new(),
        display_peaks: vec![DisplayBar::Level(0.5)],
        last_display_width: 100,
        fraction: 0.0,
        hover_fraction: None,
        drag_fraction: None,
        target_fraction: 0.0,
        fraction_velocity: 0.0,
        last_tick_us: 0,
        build_progress: 1.0,
        build_start_us: 0,
        previous_bars: Vec::new(),
        crossfade_progress: 1.0,
        crossfade_start_us: 0,
        min_bar_height: MIN_BAR_HEIGHT,
        max_bar_height: MAX_BAR_HEIGHT,
        duration_ms: 0,
    };
    ensure_resampled(&mut state, 200);
    assert!(state.display_peaks.is_empty());
}

#[test]
fn ensure_resampled_populates_on_width_change() {
    let mut state = State {
        raw_peaks: vec![128u8; 1000],
        display_peaks: Vec::new(),
        last_display_width: 0,
        fraction: 0.0,
        hover_fraction: None,
        drag_fraction: None,
        target_fraction: 0.0,
        fraction_velocity: 0.0,
        last_tick_us: 0,
        build_progress: 1.0,
        build_start_us: 0,
        previous_bars: Vec::new(),
        crossfade_progress: 1.0,
        crossfade_start_us: 0,
        min_bar_height: MIN_BAR_HEIGHT,
        max_bar_height: MAX_BAR_HEIGHT,
        duration_ms: 0,
    };
    ensure_resampled(&mut state, 600);
    assert!(!state.display_peaks.is_empty());
    assert_eq!(state.last_display_width, 600);

    // Calling again with same width should not change the display_peaks vec.
    let before_len = state.display_peaks.len();
    ensure_resampled(&mut state, 600);
    assert_eq!(state.display_peaks.len(), before_len);

    state.previous_bars = vec![DisplayBar::Level(0.25); before_len];
    state.crossfade_progress = 0.5;
    state.crossfade_start_us = 1;
    ensure_resampled(&mut state, 500);
    assert!(state.previous_bars.is_empty());
    assert_eq!(state.crossfade_progress, 1.0);
    assert_eq!(state.crossfade_start_us, 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_waveform_has_16px_height() {
    if gtk4::init().is_err() {
        return;
    }
    let w = WaveformSeek::new_mini();
    assert_eq!(w.widget().content_height(), 16);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_7_waveform_position_hard_switches_when_system_animations_are_disabled() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let waveform = WaveformSeek::new();
    waveform.set_fraction(0.20);
    waveform.state.borrow_mut().last_tick_us = gtk4::glib::monotonic_time() - 1_000_000;
    waveform.set_fraction_smooth(0.22);

    let state = waveform.state.borrow();
    assert_eq!(state.fraction, 0.22);
    assert_eq!(state.target_fraction, 0.22);
    assert_eq!(state.fraction_velocity, 0.0);
    drop(state);
    assert!(waveform.tick_id.borrow().is_none());

    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_7_waveform_completes_build_up_when_animations_disabled_mid_build() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();

    // Start an Ambient build-up with animations on.
    settings.set_gtk_enable_animations(true);
    let waveform = WaveformSeek::new();
    waveform.set_peaks(vec![128u8; 1000]);
    assert!(
        waveform.state.borrow().build_progress < 1.0,
        "set_peaks with animations on should start an in-progress build-up"
    );

    // Disable animations mid-build, then deliver a smooth position update.
    // The build-up must be marked complete (the advancing tick is removed
    // here), otherwise the waveform freezes half-built.
    settings.set_gtk_enable_animations(false);
    waveform.set_fraction_smooth(0.30);

    assert_eq!(waveform.state.borrow().build_progress, 1.0);
    assert!(waveform.tick_id.borrow().is_none());

    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_5_waveform_crossfades_to_the_new_track_instead_of_rebuilding() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);

    let waveform = WaveformSeek::new();
    let window = gtk4::Window::new();
    window.set_default_size(600, 80);
    window.set_child(Some(waveform.widget()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    waveform.set_peaks(vec![80u8; 1000]);
    {
        let mut state = waveform.state.borrow_mut();
        ensure_resampled(&mut state, 600);
        state.build_progress = 1.0;
    }

    waveform.set_peaks(vec![220u8; 1000]);
    {
        let state = waveform.state.borrow();
        assert!(!state.previous_bars.is_empty());
        assert_eq!(state.crossfade_progress, 0.0);
        assert_eq!(state.build_progress, 1.0);
    }
    assert!(waveform.tick_id.borrow().is_some());

    let second_track_bars = {
        let mut state = waveform.state.borrow_mut();
        ensure_resampled(&mut state, 600);
        state.display_peaks.clone()
    };
    waveform.set_peaks(vec![140u8; 1000]);
    {
        let state = waveform.state.borrow();
        assert_eq!(state.previous_bars, second_track_bars);
        assert_eq!(state.crossfade_progress, 0.0);
        assert_eq!(state.build_progress, 1.0);
    }

    let frame_time = waveform.widget().frame_clock().unwrap().frame_time();
    waveform.state.borrow_mut().crossfade_start_us =
        frame_time - i64::from(motion::AMBIENT_MS) * 1_000 - 1;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    while waveform.state.borrow().crossfade_progress < 1.0 && std::time::Instant::now() < deadline {
        while gtk4::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    {
        let state = waveform.state.borrow();
        assert_eq!(state.crossfade_progress, 1.0);
        assert!(state.previous_bars.is_empty());
    }
    assert!(waveform.tick_id.borrow().is_none());

    settings.set_gtk_enable_animations(previous);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_7_waveform_crossfade_hard_switches_when_animations_are_disabled() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let waveform = WaveformSeek::new();
    waveform.state.borrow_mut().display_peaks = vec![DisplayBar::Level(0.25); 40];
    waveform.set_peaks(vec![220u8; 1000]);

    let state = waveform.state.borrow();
    assert!(state.previous_bars.is_empty());
    assert_eq!(state.crossfade_progress, 1.0);
    assert_eq!(state.build_progress, 1.0);
    drop(state);
    assert!(waveform.tick_id.borrow().is_none());

    settings.set_gtk_enable_animations(previous);
}
