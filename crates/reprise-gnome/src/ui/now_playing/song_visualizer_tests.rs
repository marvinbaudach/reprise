use super::*;

#[test]
fn ac_10_visual_chrome_uses_the_shared_cover_accent_and_press_vocabulary() {
    let css = css();
    assert!(css.matches("@reprise_player_accent").count() >= 4);
    assert!(css.matches("color: @reprise_player_accent").count() >= 2);
    assert!(css.contains(".reprise-song-visual-fullscreen-canvas"));
    assert!(css.contains(".reprise-song-visual-modes"));
    // Fullscreen chrome fades rather than snapping.
    assert!(css.contains(".reprise-song-visual-chrome-hidden"));
    assert!(css.contains("transition: opacity"));

    let buttons = crate::ui::style::buttons::css();
    assert!(buttons.contains(".reprise-btn-toggle:active"));
    assert!(buttons.contains(".reprise-btn-toggle:focus-visible"));
}

#[test]
fn mode_labels_match_visual_mode_order() {
    let labels: Vec<String> = VisualMode::ALL
        .iter()
        .map(|&mode| strings::text(mode_label(mode)))
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    assert_eq!(
        labels,
        [
            "Grid",
            "Bars",
            "Rings",
            "Flow",
            "Pulse",
            "Particles",
            "Neon",
            "Tunnel",
        ]
    );
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

/// Builds an engine that has just been hammered by a beat-then-slam: playing,
/// accented, 20 silent frames (settles the envelopes at rest) followed by 10
/// full-scale frames (fires a beat, a kick, and pins the bands high). Mirrors
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
    for _ in 0..10 {
        engine.ingest(&analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        engine.tick();
    }
    engine
}

#[test]
#[ignore = "visual gallery: renders every mode's scene to REPRISE_VIS_OUT for eyeballing"]
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
            cr.set_source_rgb(0.078, 0.094, 0.102); // dark panel
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
