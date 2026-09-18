use super::*;
use crate::playback::STEADY_GLOW;
use crate::visuals::color;

#[test]
fn bars_builds_a_finite_sane_nonempty_scene() {
    let scene = lively_engine().scene(548.0, 300.0);
    assert!(scene.shapes.len() > 1);
    assert!(scene.is_finite_and_sane(548.0, 300.0));
}

#[test]
fn ac_23_ingest_uses_cava_values_without_a_second_bar_envelope() {
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    let bars = std::array::from_fn(|index| index as f32 / SPECTRUM_BAND_COUNT as f32);

    engine.ingest((
        &SpectrumFrame::from_cava_bars(bars),
        Duration::from_micros(16_667),
    ));

    assert_eq!(engine.bands_current, bars);
}

const WIDTH: f32 = 548.0;
const HEIGHT: f32 = 300.0;

/// One engine holding `bars` on screen and `pressure` in its glow layer.
fn engine_with(bars: [f32; SPECTRUM_BAND_COUNT], pressure: BassPressure) -> VisualEngine {
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.ingest((
        &SpectrumFrame::from_cava_bars(bars).with_bass_pressure(pressure),
        Duration::from_micros(16_667),
    ));
    engine
}

fn paused_live_engine() -> VisualEngine {
    let bars = std::array::from_fn(|index| 0.2 + index as f32 * 0.7 / 63.0);
    let mut engine = engine_with(bars, BassPressure::silent());
    engine.set_has_track(true);
    engine.set_playing(false);
    engine
}

#[test]
fn ac_27_paused_live_bands_keep_moving_inside_the_resting_range() {
    let mut engine = paused_live_engine();
    let live = engine.bands_current;
    assert_eq!(engine.display_bands, live, "pause introduced a jump");
    engine.tick();
    assert!(
        engine
            .display_bands
            .iter()
            .zip(live)
            .all(|(paused, live)| (paused - live).abs() < 0.01),
        "the resting shape did not fade in softly"
    );
    for _ in 1..120 {
        engine.tick();
    }

    let mut previous = engine.display_bands;
    let mut changed_samples = 0;
    for _ in 0..8 {
        for _ in 0..30 {
            engine.tick();
        }
        let current = engine.display_bands;
        assert!(
            current.iter().all(|band| (0.04..=0.38).contains(band)),
            "paused live bands left the resting range: {current:?}"
        );
        changed_samples += usize::from(current != previous);
        previous = current;
    }

    assert_eq!(changed_samples, 8, "the paused live scene stopped moving");
}

#[test]
fn ac_27_paused_live_bands_cover_a_clearly_visible_range() {
    let mut engine = paused_live_engine();
    for _ in 0..120 {
        engine.tick();
    }
    let checked_bands = [0, 8, 17, 31, 40, 63];
    let mut minima = [f32::INFINITY; 6];
    let mut maxima = [f32::NEG_INFINITY; 6];

    for _ in 0..IDLE_PERIOD_TICKS as usize {
        engine.tick();
        for (sample, band) in checked_bands.into_iter().enumerate() {
            minima[sample] = minima[sample].min(engine.display_bands[band]);
            maxima[sample] = maxima[sample].max(engine.display_bands[band]);
        }
    }

    for ((band, minimum), maximum) in checked_bands.into_iter().zip(minima).zip(maxima) {
        let span = maximum - minimum;
        assert!(
            span > 0.14,
            "paused band {band} moved through only {span} ({minimum}..={maximum})"
        );
    }
}

#[test]
fn ac_27_paused_live_bands_return_near_their_start_instead_of_drifting() {
    let mut engine = paused_live_engine();
    for _ in 0..120 {
        engine.tick();
    }
    let starting = engine.display_bands[17];
    let mut was_near = true;
    let mut returns = 0;

    for _ in 0..(IDLE_PERIOD_TICKS as usize * 3) {
        engine.tick();
        let is_near = (engine.display_bands[17] - starting).abs() < 0.0015;
        if is_near && !was_near {
            returns += 1;
        }
        was_near = is_near;
    }

    assert!(
        returns >= 5,
        "paused band drifted instead of returning, saw {returns} returns"
    );
}

#[test]
fn ac_27_paused_live_bands_move_out_of_phase() {
    let mut engine = paused_live_engine();
    for _ in 0..120 {
        engine.tick();
    }
    let before = engine.display_bands;

    engine.tick();

    let low_delta = engine.display_bands[8] - before[8];
    let high_delta = engine.display_bands[40] - before[40];
    assert!(
        low_delta.abs() > 0.0001 && high_delta.abs() > 0.0001,
        "chosen bands did not move clearly: {low_delta}, {high_delta}"
    );
    assert!(
        low_delta.signum() != high_delta.signum(),
        "paused bands moved in sync: {low_delta}, {high_delta}"
    );
}

#[test]
fn ac_27_resumed_live_bands_take_over_before_another_ingest() {
    let mut engine = paused_live_engine();
    let live = engine.bands_current;
    for _ in 0..180 {
        engine.tick();
    }
    assert_ne!(engine.display_bands, live);

    engine.set_playing(true);

    assert_eq!(engine.display_bands, live);
}

#[test]
fn current_bands_reports_the_displayed_bars_not_the_raw_ones() {
    // Regression: `current_bands()` used to return `bands_current` — the
    // raw last-ingested frame — while the screen draws `display_bands`,
    // which the AC-27 paused-live blend has already pulled away from it.
    // A fresh sibling engine adopting `current_bands()` at that point
    // reintroduced energy the viewer had already watched decay away, a
    // visible "pop". `paused_live_engine` plus enough ticks is the same
    // setup the AC-27 tests above use to produce that divergence.
    let mut engine = paused_live_engine();
    let raw = engine.bands_current;
    for _ in 0..120 {
        engine.tick();
    }
    assert_ne!(
        engine.display_bands, raw,
        "test setup did not actually diverge display_bands from bands_current"
    );
    assert_eq!(
        engine.current_bands(),
        &engine.display_bands,
        "current_bands must report what is on screen, not the raw ingested bands"
    );
}

/// A stage light: the hit throws it to full, then it falls.
#[test]
fn ac_23_a_bass_hit_throws_the_glow_to_full_and_then_it_falls() {
    let mut engine = VisualEngine::new();
    engine.set_has_track(true);
    engine.set_playing(true);

    // A full kick arrives. The attack is immediate — no easing, no ramp.
    engine.ingest((
        &frame_with(BassPressure {
            kick: 1.0,
            ..pressure(0.0, 0.0)
        }),
        Duration::from_micros(16_667),
    ));
    assert!(
        engine.glow >= 1.0 - f32::EPSILON,
        "the hit did not reach full: {}",
        engine.glow
    );

    // Silence afterwards: it falls, and it falls all the way.
    for _ in 0..2 {
        engine.tick();
    }
    let after_two = engine.glow;
    assert!(after_two < 1.0, "the light latched on: {after_two}");
    for _ in 0..60 {
        engine.tick();
    }
    assert_eq!(engine.glow, 0.0, "the light never went out");
}

/// The reason this changed at all: `impact` cannot reach full on a
/// limited master — measured over three real tracks it tops out at 0.85 —
/// so the glow must not be sourced from it.
#[test]
fn ac_23_the_glow_reads_the_kick_and_not_the_impact() {
    let mut engine = VisualEngine::new();
    engine.set_has_track(true);
    engine.set_playing(true);

    engine.ingest((
        &frame_with(BassPressure {
            kick: 0.0,
            ..pressure(1.0, 1.0)
        }),
        Duration::from_micros(16_667),
    ));
    assert_eq!(
        engine.glow, 0.0,
        "a maxed-out impact must not light the stage on its own"
    );

    engine.ingest((
        &frame_with(BassPressure {
            kick: 0.8,
            ..pressure(0.0, 0.0)
        }),
        Duration::from_micros(16_667),
    ));
    assert!((engine.glow - 0.8).abs() < 1e-6, "got {}", engine.glow);
}

fn frame_with(reading: BassPressure) -> SpectrumFrame {
    SpectrumFrame::from_cava_bars([0.0; SPECTRUM_BAND_COUNT]).with_bass_pressure(reading)
}

/// A reading whose *attack* is `kick` — what the stage light runs on.
fn hit(kick: f32, aura: f32) -> BassPressure {
    BassPressure {
        kick,
        ..pressure(0.0, aura)
    }
}

fn pressure(impact: f32, aura: f32) -> BassPressure {
    BassPressure {
        level_dbfs: -14.0,
        baseline_dbfs: -20.0,
        impact,
        aura,
        kick: 0.0,
        pressure: 0.0,
    }
}

/// Alphas of the broad glows that sit low behind the columns.
fn broad_glow_alphas(engine: &VisualEngine) -> Vec<f32> {
    engine
        .scene(WIDTH, HEIGHT)
        .shapes
        .into_iter()
        .filter_map(|shape| match (shape.geom, shape.fill) {
            (Geom::RadialGlow { cy, r, .. }, Fill::Solid(fill))
                if cy > HEIGHT * 0.6 && r > WIDTH * 0.2 =>
            {
                Some(fill.a)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn ac_23_loud_cava_bass_bands_alone_never_ignite_the_glow() {
    // The exact failure this replaced: CAVA's auto-sensitivity lifts the
    // low bands during a quiet sung passage until they read like a drop.
    let mut bars = [0.0; SPECTRUM_BAND_COUNT];
    bars[..12].fill(0.95);

    let engine = engine_with(bars, pressure(0.0, 0.0));

    assert!(broad_glow_alphas(&engine).is_empty());
}

#[test]
fn ac_23_the_measured_kick_ignites_the_broad_glows() {
    // Bars stay empty; only the attack reading drives the stage light.
    let engine = engine_with([0.0; SPECTRUM_BAND_COUNT], hit(1.0, 0.0));

    assert_eq!(broad_glow_alphas(&engine).len(), 2);
}

#[test]
fn ac_23_a_rhythmic_kick_glows_softer_than_a_full_drop() {
    let kick = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(STEADY_GLOW, 0.0));
    let drop = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 0.0));

    let kick_alpha = broad_glow_alphas(&kick).iter().sum::<f32>();
    let drop_alpha = broad_glow_alphas(&drop).iter().sum::<f32>();

    assert!(kick_alpha > 0.0, "a rhythmic kick still glows softly");
    assert!(
        kick_alpha < drop_alpha * 0.5,
        "a kick must stay clearly below a full drop, got {kick_alpha:.3} vs {drop_alpha:.3}"
    );
}

#[test]
fn ac_23_only_a_sustained_breakdown_adds_the_inner_auras() {
    let kicking = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 0.0));
    let breakdown = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 1.0));

    assert_eq!(broad_glow_alphas(&kicking).len(), 2);
    assert_eq!(broad_glow_alphas(&breakdown).len(), 4);
}

#[test]
fn ac_23_the_glow_leaves_with_the_track_when_playback_stops() {
    let mut engine = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 1.0));
    engine.set_playing(false);

    for _ in 0..200 {
        engine.tick();
    }

    assert!(broad_glow_alphas(&engine).is_empty());
}

#[test]
fn accent2_is_always_hue_shifted_from_the_effective_accent() {
    let mut engine = VisualEngine::new();
    engine.set_accent((0.8, 0.2, 0.2));
    let ctx_hue = color::rgb_hue(engine.accent2());
    let want = (color::rgb_hue((0.8, 0.2, 0.2)) + 42.0) % 360.0;
    let delta = (ctx_hue - want).abs().min(360.0 - (ctx_hue - want).abs());
    assert!(delta < 3.0);
}

#[test]
fn a_tinted_scene_paints_the_given_accent_and_leaves_the_engine_s_own_alone() {
    let mut engine = lively_engine();
    engine.set_accent((0.8, 0.2, 0.2));
    let own = engine.scene(548.0, 300.0);
    let tinted = engine.scene_with_accent(548.0, 300.0, (0.1, 0.3, 0.9));

    let Fill::Solid(own_glow) = own.shapes[0].fill;
    let Fill::Solid(tinted_glow) = tinted.shapes[0].fill;
    assert_eq!((own_glow.r, own_glow.g, own_glow.b), (0.8, 0.2, 0.2));
    assert_eq!(
        (tinted_glow.r, tinted_glow.g, tinted_glow.b),
        (0.1, 0.3, 0.9)
    );
    assert_eq!(own_glow.a, tinted_glow.a);
    assert_eq!(own.shapes.len(), tinted.shapes.len());
    assert_eq!(engine.accent, (0.8, 0.2, 0.2));
}

#[test]
fn test_ctx_borrows_the_exact_cava_bars() {
    let engine = lively_engine();
    let ctx = test_ctx(&engine, 548.0, 300.0);
    assert_eq!(ctx.bars, &engine.bands_current);
    assert_eq!((ctx.width, ctx.height), (548.0, 300.0));
}
