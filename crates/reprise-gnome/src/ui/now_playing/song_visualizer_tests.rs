use super::*;

const MEMBRANE_PHASE_STEPS: [usize; 9] = [0, 4, 8, 12, 16, 22, 30, 40, 56];

#[test]
fn ac_20_visual_chrome_offers_only_grid_and_bars() {
    let css = css();
    assert!(css.contains("color: @reprise_player_accent"));
    assert!(css.contains(".reprise-song-visual-canvas"));
    assert!(css.contains(".reprise-song-visual-modes"));

    let labels: Vec<String> = VisualMode::ALL
        .iter()
        .map(|&mode| strings::text(mode_label(mode)))
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    assert_eq!(labels, ["Grid", "Bars"]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_20_visual_widget_exposes_a_labeled_canvas_and_mode_row() {
    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();

    assert_eq!(visualizer.root.observe_children().n_items(), 2);
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
fn membrane_phase_gallery_steps_cover_both_dome_and_depth() {
    use reprise_core::playback::SPECTRUM_BAND_COUNT;
    use reprise_core::visuals::Membrane;

    let mut membrane = Membrane::new();
    let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
    membrane.splash(1.0);
    membrane.advance(&silent);

    let mut captured = Vec::new();
    for step in 0..=*MEMBRANE_PHASE_STEPS.last().unwrap() {
        if MEMBRANE_PHASE_STEPS.contains(&step) {
            captured.push(membrane.sample(0.5, 0.5));
        }
        membrane.advance(&silent);
    }
    let peak = captured.iter().copied().fold(0.0_f32, f32::max);
    let trough = captured.iter().copied().fold(0.0_f32, f32::min);
    assert!(peak > 0.35, "gallery must capture the dome, peak {peak}");
    assert!(
        trough < -0.05,
        "gallery must capture the depth phase, trough {trough}"
    );
}

#[test]
#[ignore = "visual gallery: renders both mode scenes to REPRISE_VIS_OUT for eyeballing"]
fn render_mode_gallery_ppm() {
    let out = std::env::var("REPRISE_VIS_OUT").unwrap_or_else(|_| "/tmp".to_owned());
    let (w, h) = (548.0_f32, 300.0_f32);

    for mode in VisualMode::ALL {
        let mut engine = lively_engine();
        engine.set_mode(mode);
        let scene = engine.scene(w, h);

        let mut surface =
            gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, w as i32, h as i32)
                .unwrap();
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
        let path = format!("{out}/visualizer-{}.ppm", mode.id());
        std::fs::write(&path, ppm).unwrap();
        println!("wrote {path}");
    }
}

/// Renders a radial membrane phase sequence (one strong beat off silence,
/// followed by quiet) so the dome, glow, depth trough and settling rings can
/// be inspected. Frames are written as `grid-phase-NN.ppm`.
#[test]
#[ignore = "visual: renders the Grid membrane phases to REPRISE_VIS_OUT"]
fn render_grid_membrane_phase_sequence() {
    use reprise_core::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};
    let out = std::env::var("REPRISE_VIS_OUT").unwrap_or_else(|_| "/tmp".to_owned());
    let (w, h) = (548.0_f32, 300.0_f32);

    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.22, 0.78, 0.74));
    let mut analyzer = SpectrumAnalyzer::new();
    for _ in 0..25 {
        engine.ingest(&analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        engine.tick();
    }
    // One full-scale broadband frame fires a strong central cone impulse.
    engine.ingest(&analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
    engine.tick();

    let mut next = 0usize;
    for step in 0..=*MEMBRANE_PHASE_STEPS.last().unwrap() {
        if MEMBRANE_PHASE_STEPS.get(next) == Some(&step) {
            let scene = engine.scene(w, h);
            let mut surface =
                gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, w as i32, h as i32)
                    .unwrap();
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
            let path = format!("{out}/grid-phase-{step:02}.ppm");
            std::fs::write(&path, ppm).unwrap();
            println!("wrote {path}");
            next += 1;
        }
        // Quiet after the beat: isolate the cloth's underdamped response.
        engine.ingest(&analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        engine.tick();
    }
}
