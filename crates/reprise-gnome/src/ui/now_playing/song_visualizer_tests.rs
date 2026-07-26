use super::*;

#[test]
fn ac_20_visual_chrome_is_a_bars_only_canvas() {
    let css = css();
    assert!(css.contains("color: @reprise_player_accent"));
    assert!(css.contains(".reprise-song-visual-canvas"));
    assert!(!css.contains(".reprise-song-visual-modes"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_20_visual_widget_exposes_only_a_labeled_bars_canvas() {
    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();

    assert_eq!(visualizer.root.observe_children().n_items(), 1);
    assert_eq!(visualizer.area.accessible_role(), gtk4::AccessibleRole::Img);
    assert!(gtk4::test_accessible_has_property(
        &visualizer.area,
        gtk4::AccessibleProperty::Label
    ));
}

/// Builds an engine that has just been hammered by a beat-then-sustain:
/// playing, accented, 20 silent frames (settles the envelopes at rest), one
/// full-spectrum impact frame (fires the beat/kick off the silence-to-loud
/// flux jump — a real kick is broadband for an instant), then 9 frames of a
/// realistic bass-heavy, treble-light sustain.
///
/// With the honest-loudness spectrum mapping (`SpectrumAnalyzer::ingest`,
/// `playback.rs`), each band's height reflects its actual level, so the
/// bass-heavy sustain reads back as a genuine spectrum silhouette (loud bass
/// tapering to quiet treble) rather than a flat maxed-out wall. Mirrors
/// `reprise_core::visuals::engine::lively_engine`, which is test-only and not
/// exported across the crate boundary.
fn lively_engine() -> VisualEngine {
    use reprise_core::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};

    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.22, 0.78, 0.74)); // app teal
    let mut analyzer = SpectrumAnalyzer::new();
    for _ in 0..20 {
        engine.ingest(&analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        engine.tick();
    }
    // One full-scale impact frame: fires the beat and seeds every band's
    // auto-gain ceiling near its max.
    engine.ingest(&analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
    engine.tick();
    // Bass-heavy descending sustain with a couple of ripples, not a flat
    // 0 dB slam: db(i) = -6 - 55*(i/255)^0.6 + 8*sin(i*0.20), clamped. Held
    // well above the auto-gain floor throughout, so it reads as a smooth
    // taper rather than a hard on/off cutoff.
    let mut shaped = [0.0_f32; SPECTRUM_ANALYSIS_BAND_COUNT];
    for (i, bin) in shaped.iter_mut().enumerate() {
        let x = i as f32 / (SPECTRUM_ANALYSIS_BAND_COUNT - 1) as f32;
        let db = -6.0 - 55.0 * x.powf(0.6) + 8.0 * (i as f32 * 0.20).sin();
        *bin = db.clamp(-80.0, 0.0);
    }
    for _ in 0..9 {
        engine.ingest(&analyzer.ingest(shaped));
        engine.tick();
    }
    engine
}

#[test]
#[ignore = "visual gallery: renders the Bars scene to REPRISE_VIS_OUT for eyeballing"]
fn render_bars_gallery_ppm() {
    let out = std::env::var("REPRISE_VIS_OUT").unwrap_or_else(|_| "/tmp".to_owned());
    let (w, h) = (548.0_f32, 300.0_f32);

    let engine = lively_engine();
    let scene = engine.scene(w, h);

    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, w as i32, h as i32).unwrap();
    {
        let cr = gtk4::cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(0.078, 0.094, 0.102);
        let _ = cr.paint();
        render::draw_scene(&cr, &scene);
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
#[ignore = "diagnostic: measures the complete scene-build and Cairo-render frame budget"]
fn bars_fullscreen_render_budget_diagnostic() {
    use reprise_core::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};
    use std::time::Instant;

    const FRAMES: usize = 240;
    const FRAME_BUDGET_MS: f64 = 16.0;

    let mut over_budget = Vec::new();
    for (width, height) in [(548, 300), (960, 540), (1920, 1080)] {
        let mut analyzer = SpectrumAnalyzer::new();
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        let surface =
            gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, width, height).unwrap();
        let mut renderer = render::SceneRenderer::default();
        let mut timings = Vec::with_capacity(FRAMES);

        for frame in 0..FRAMES {
            let mut input = [-42.0_f32; SPECTRUM_ANALYSIS_BAND_COUNT];
            if frame % 10 == 0 {
                input[..96].fill(-2.0);
                input[96..].fill(-12.0);
            }
            engine.ingest(&analyzer.ingest(input));

            let started = Instant::now();
            engine.tick();
            let scene_size = render::capped_scene_size(width, height);
            let scene = engine.scene(scene_size.0 as f32, scene_size.1 as f32);
            let cr = gtk4::cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(0.02, 0.025, 0.03);
            let _ = cr.paint();
            renderer.draw(&cr, &scene, width, height);
            surface.flush();
            timings.push(started.elapsed().as_secs_f64() * 1000.0);
        }

        timings.sort_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            let index = ((timings.len() - 1) as f64 * fraction).round() as usize;
            timings[index]
        };
        let p50 = percentile(0.50);
        let p95 = percentile(0.95);
        let p99 = percentile(0.99);
        println!("Bars {width}x{height}: p50={p50:.3} ms p95={p95:.3} ms p99={p99:.3} ms");
        if p95 > FRAME_BUDGET_MS {
            over_budget.push(format!("Bars {width}x{height} p95={p95:.3} ms"));
        }
    }
    assert!(
        over_budget.is_empty(),
        "modes miss the 60 Hz frame budget: {}",
        over_budget.join(", ")
    );
}
