use super::impact::ImpactState;
use super::*;

fn bands_ramp() -> [f32; SPECTRUM_BAND_COUNT] {
    std::array::from_fn(|index| (index as f32 / SPECTRUM_BAND_COUNT as f32).clamp(0.0, 1.0))
}

fn input_for<'a>(
    bands: &'a [f32; SPECTRUM_BAND_COUNT],
    peaks: &'a [f32; SPECTRUM_BAND_COUNT],
) -> SceneInput<'a> {
    SceneInput { bands, peaks }
}

#[test]
fn ac_10_bars_geometry_is_bounded_with_value_and_peak_marks() {
    let bands = bands_ramp();
    let peaks = bands_ramp();
    let bars = scene(&input_for(&bands, &peaks), 240.0, 220.0);

    // One value bar + one peak-hold tick per band.
    assert_eq!(bars.bars.len(), SPECTRUM_BAND_COUNT * 2);
    assert!(bars.is_finite_and_bounded(240.0, 220.0));
}

#[test]
fn ac_10_louder_spectrum_grows_the_geometry() {
    let quiet_bands = [0.0; SPECTRUM_BAND_COUNT];
    let loud_bands = [1.0; SPECTRUM_BAND_COUNT];
    let quiet = scene(&input_for(&quiet_bands, &quiet_bands), 240.0, 220.0);
    let loud = scene(&input_for(&loud_bands, &loud_bands), 240.0, 220.0);

    assert_eq!(quiet.bars.len(), loud.bars.len());
    assert!(
        loud.bars.iter().map(|bar| bar.length).sum::<f64>()
            > quiet.bars.iter().map(|bar| bar.length).sum::<f64>()
    );
}

#[test]
fn ac_10_visual_chrome_uses_the_shared_cover_accent_and_press_vocabulary() {
    let css = css();
    assert!(css.matches("@reprise_player_accent").count() >= 4);
    assert!(css.matches("color: @reprise_player_accent").count() >= 2);
    assert!(css.contains(".reprise-song-visual-fullscreen-canvas"));
    // Fullscreen chrome fades rather than snapping.
    assert!(css.contains(".reprise-song-visual-chrome-hidden"));
    assert!(css.contains("transition: opacity"));

    let buttons = crate::ui::style::buttons::css();
    assert!(buttons.contains(".reprise-btn-toggle:active"));
    assert!(buttons.contains(".reprise-btn-toggle:focus-visible"));
}

#[test]
fn ac_11_playing_rises_fast_then_pause_eases_down() {
    let mut state = RenderState {
        target: [1.0; SPECTRUM_BAND_COUNT],
        static_profile: [0.2; SPECTRUM_BAND_COUNT],
        level_target: 1.0,
        playback: PlaybackState::Playing,
        ..RenderState::default()
    };

    // Playing never settles, and fast attack lifts bands hard in one step.
    assert!(!advance_state(&mut state));
    let risen = state.current[0];
    assert!(risen > 0.3, "fast attack should lift quickly, got {risen}");

    state.playback = PlaybackState::Paused;
    state.target = state.static_profile;
    state.level_target = 0.0;
    advance_state(&mut state);
    assert!(
        state.current[0] < risen,
        "release should ease back down toward the static profile"
    );
}

#[test]
fn ac_11_stop_then_track_clear_settles_toward_neutral() {
    let mut state = RenderState {
        current: [0.8; SPECTRUM_BAND_COUNT],
        target: [0.2; SPECTRUM_BAND_COUNT],
        static_profile: [0.2; SPECTRUM_BAND_COUNT],
        playback: PlaybackState::Stopped,
        ..RenderState::default()
    };

    clear_static_profile(&mut state, true);
    assert_eq!(state.current, [0.8; SPECTRUM_BAND_COUNT]);
    assert_eq!(state.target, NEUTRAL_PROFILE);

    let mut settled = false;
    for _ in 0..1000 {
        if advance_state(&mut state) {
            settled = true;
            break;
        }
    }
    assert!(settled, "a stopped visualizer must come to rest");
    assert!(state
        .current
        .iter()
        .all(|band| (*band - NEUTRAL_PROFILE[0]).abs() < 0.01));
}

#[test]
fn impact_beat_storm_stays_within_fixed_capacity() {
    let mut impact = ImpactState::new();
    for _ in 0..200 {
        impact.spawn_beat(1.0);
    }
    // Pools are fixed-capacity: a beat storm never grows without bound.
    assert!(impact.shockwaves().count() <= 6);
    assert!(impact.particles().count() <= 56);
    for spark in impact.particles() {
        assert!(spark.dist.is_finite() && spark.life_frac.is_finite());
        assert!((0.0..=1.0).contains(&spark.life_frac));
    }
    for wave in impact.shockwaves() {
        assert!((0.0..=1.0).contains(&wave.progress));
    }
}

#[test]
fn impact_decays_to_rest_after_a_burst() {
    let mut impact = ImpactState::new();
    assert!(impact.is_idle());
    impact.spawn_beat(1.0);
    impact.spawn_drop(0.9);
    assert!(!impact.is_idle());
    for _ in 0..200 {
        impact.advance();
    }
    assert!(impact.is_idle(), "all ornaments must decay to rest");
}

#[test]
fn impact_drop_below_threshold_is_a_noop() {
    let mut impact = ImpactState::new();
    impact.spawn_drop(0.1);
    assert!(impact.is_idle(), "ordinary loudness must not flash");
    assert_eq!(impact.flash(), 0.0);

    impact.spawn_drop(0.95);
    assert!(!impact.is_idle());
    assert!(impact.flash() > 0.0);
}

#[test]
#[ignore = "visual gallery: renders the bars scene to REPRISE_VIS_OUT for eyeballing"]
fn render_bars_gallery_png() {
    let out = std::env::var("REPRISE_VIS_OUT").unwrap_or_else(|_| "/tmp".to_owned());
    let (w, h) = (548.0_f64, 300.0_f64);
    // A lively, bass-heavy frame mid-beat.
    let bands: [f32; SPECTRUM_BAND_COUNT] = std::array::from_fn(|i| {
        let x = i as f32 / SPECTRUM_BAND_COUNT as f32;
        (0.9 * (1.0 - x) + 0.35 * (x * 9.0).sin().abs()).clamp(0.05, 1.0)
    });
    let peaks: [f32; SPECTRUM_BAND_COUNT] = std::array::from_fn(|i| bands[i] * 0.9);
    let accent = (0.22, 0.78, 0.74); // app teal

    let mut state = RenderState {
        current: bands,
        target: bands,
        peaks,
        level: 0.82,
        level_target: 0.82,
        playback: PlaybackState::Playing,
        ..RenderState::default()
    };
    state.impact.spawn_beat(0.9);
    state.impact.spawn_drop(0.7);
    for _ in 0..5 {
        state.impact.advance();
    }

    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, w as i32, h as i32).unwrap();
    {
        let cr = gtk4::cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(0.078, 0.094, 0.102); // dark panel
        let _ = cr.paint();
        let scene = scene(&state.scene_input(), w, h);
        draw_scene(
            &cr,
            &scene,
            &state.impact,
            w,
            h,
            f64::from(state.level),
            accent,
        );
    }
    let (iw, ih) = (w as usize, h as usize);
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut ppm = format!("P6\n{iw} {ih}\n255\n").into_bytes();
    for y in 0..ih {
        for x in 0..iw {
            let o = y * stride + x * 4;
            ppm.extend_from_slice(&[data[o + 2], data[o + 1], data[o]]);
        }
    }
    drop(data);
    let path = format!("{out}/visualizer-bars.ppm");
    std::fs::write(&path, ppm).unwrap();
    println!("wrote {path}");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_10_visual_widget_exposes_a_labeled_canvas() {
    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();

    assert_eq!(visualizer.area.accessible_role(), gtk4::AccessibleRole::Img);
    assert!(gtk4::test_accessible_has_property(
        &visualizer.area,
        gtk4::AccessibleProperty::Label
    ));
}
