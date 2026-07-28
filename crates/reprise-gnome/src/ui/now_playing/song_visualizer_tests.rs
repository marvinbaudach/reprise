use super::*;

#[test]
fn ac_23_visual_chrome_is_a_bars_only_canvas() {
    let css = css();
    assert!(css.contains("color: @reprise_player_accent"));
    assert!(css.contains(".reprise-song-visual-canvas"));
    assert!(!css.contains(".reprise-song-visual-modes"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_23_visual_widget_exposes_only_a_labeled_bars_canvas() {
    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();

    // The canvas, and under it the analysis readout — nothing else.
    assert_eq!(visualizer.root.observe_children().n_items(), 2);
    assert_eq!(visualizer.area.accessible_role(), gtk4::AccessibleRole::Img);
    assert!(gtk4::test_accessible_has_property(
        &visualizer.area,
        gtk4::AccessibleProperty::Label
    ));
}

#[test]
fn ac_23_the_analysis_readout_reports_the_values_the_glow_uses() {
    let values = analysis_values(BassPressure {
        level_dbfs: -14.2,
        baseline_dbfs: -20.0,
        impact: 0.42,
        aura: 0.0,
    });

    // Whole decibels: a tenth of a dB is neither readable at this refresh rate
    // nor affordable in a 300 px panel, where it truncated "Baseline".
    assert_eq!(values.len(), 4);
    assert_eq!(values[0], "-14 dBFS");
    assert_eq!(values[1], "-20 dBFS");
    assert_eq!(values[2], "0.42");
    assert_eq!(values[3], "0.00");
}

#[test]
fn ac_23_a_silent_analysis_reads_as_a_dash_instead_of_a_bottomed_out_level() {
    let values = analysis_values(BassPressure {
        level_dbfs: -140.0,
        baseline_dbfs: -140.0,
        impact: 0.0,
        aura: 0.0,
    });

    assert_eq!(values[0], "—");
    assert_eq!(values[1], "—");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_23_the_readout_fits_in_the_strip_left_under_the_canvas() {
    // The panel is a fixed 300 px wide and the canvas takes everything above,
    // leaving roughly one strip. A readout taller than that is silently
    // clipped — the live session showed Impact and Breakdown cut off.
    const PANEL_WIDTH: i32 = 300;
    const STRIP_HEIGHT: i32 = 52;

    gtk4::init().unwrap();
    let readout = AnalysisReadout::new();

    let (_, natural, _, _) = readout
        .root
        .measure(gtk4::Orientation::Vertical, PANEL_WIDTH);

    assert!(
        natural <= STRIP_HEIGHT,
        "the readout needs {natural} px but only ~{STRIP_HEIGHT} px are free under the canvas"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_23_the_readout_names_stay_readable_at_the_panel_width() {
    // Caption and value share one line, so the captions are the first thing
    // to be ellipsized when they don't fit — the live session showed "BA…"
    // and "BASELI…". What counts is not the 300 px panel but what is left
    // inside `.reprise-song-visuals` and its 18 px side margins, minus some
    // slack: at the bare 264 the live panel still truncated "Baseline".
    // Measured with the real stylesheet, not the default font.
    const PANEL_WIDTH: i32 = 300 - 2 * 18 - 16;

    gtk4::init().unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&css());
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let readout = AnalysisReadout::new();
    readout.set(BassPressure {
        level_dbfs: -41.7,
        baseline_dbfs: -41.1,
        impact: 0.35,
        aura: 0.95,
    });

    let (_, natural, _, _) = readout.root.measure(gtk4::Orientation::Horizontal, -1);

    assert!(
        natural <= PANEL_WIDTH,
        "the readout wants {natural} px, so its captions get truncated in the {PANEL_WIDTH} px panel"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_23_the_readout_follows_the_measurement_the_player_delivers() {
    use reprise_core::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();
    visualizer.set_playback_state(PlaybackState::Playing);

    visualizer.set_spectrum(
        SpectrumFrame::from_cava_bars([0.5; SPECTRUM_BAND_COUNT]).with_bass_pressure(
            BassPressure {
                level_dbfs: -11.4,
                baseline_dbfs: -19.0,
                impact: 0.87,
                aura: 0.31,
            },
        ),
    );

    let shown = visualizer.readout.shown_values();
    assert_eq!(shown[0], "-11 dBFS");
    assert_eq!(shown[2], "0.87");
    assert_eq!(shown[3], "0.31");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_11_loading_a_track_reaches_the_engine_and_rests_the_canvas_alive() {
    gtk4::init().unwrap();
    let visualizer = SongVisualizer::new();
    let bar_shapes = |visualizer: &SongVisualizer| {
        visualizer
            .engine
            .borrow()
            .scene(548.0, 300.0)
            .shapes
            .into_iter()
            .filter(|shape| matches!(shape.geom, reprise_core::visuals::Geom::Rect { .. }))
            .count()
    };

    visualizer.set_playback_state(PlaybackState::Stopped);
    visualizer.set_has_track(false);
    for _ in 0..120 {
        visualizer.engine.borrow_mut().tick();
    }
    assert_eq!(bar_shapes(&visualizer), 0, "an empty player draws nothing");

    visualizer.set_has_track(true);
    for _ in 0..120 {
        visualizer.engine.borrow_mut().tick();
    }
    assert!(
        bar_shapes(&visualizer) > 0,
        "a loaded but resting track keeps a low wave on the canvas"
    );
}

/// Builds a representative already-smoothed CAVA silhouette.
fn lively_engine() -> VisualEngine {
    use reprise_core::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.22, 0.78, 0.74));
    let shaped = std::array::from_fn(|index| {
        let x = index as f32 / (SPECTRUM_BAND_COUNT - 1) as f32;
        (0.9 - 0.55 * x.powf(0.6) + 0.08 * (index as f32 * 0.45).sin()).clamp(0.0, 1.0)
    });
    engine.ingest(&SpectrumFrame::from_cava_bars(shaped));
    engine
}

/// Engine at rest: a loaded track, no playback, idle wave settled in.
fn resting_engine(ticks: usize) -> VisualEngine {
    let mut engine = VisualEngine::new();
    engine.set_accent((0.22, 0.78, 0.74));
    engine.set_has_track(true);
    for _ in 0..ticks {
        engine.tick();
    }
    engine
}

/// Replays a real decoded track through the full chain — CAVA bars plus the
/// bass-pressure detector — and writes the scene at named moments, to confirm
/// on screen what the calibration says in numbers.
#[test]
#[ignore = "visual verification: needs REPRISE_VIS_PCM (raw mono f32 44.1 kHz)"]
fn render_bass_pressure_moments_ppm() {
    use reprise_core::playback::{
        BassPressureDetector, CavaBarProcessor, CavaConfig, SpectrumFrame, SPECTRUM_BAND_COUNT,
    };

    const RATE: u32 = 44_100;
    const CHUNK: usize = 1_024;
    let (w, h) = (548.0_f32, 300.0_f32);

    let Ok(pcm_path) = std::env::var("REPRISE_VIS_PCM") else {
        println!("set REPRISE_VIS_PCM to a raw mono f32 44.1 kHz file");
        return;
    };
    let out = std::env::var("REPRISE_VIS_OUT").unwrap_or_else(|_| "/tmp".to_owned());
    let moments: Vec<(String, f32)> = std::env::var("REPRISE_VIS_MOMENTS")
        .unwrap_or_else(|_| "intro:8.0,rhythmic:42.5,drop:16.5".to_owned())
        .split(',')
        .filter_map(|entry| {
            let (name, seconds) = entry.split_once(':')?;
            Some((name.to_owned(), seconds.parse().ok()?))
        })
        .collect();

    let bytes = std::fs::read(&pcm_path).expect("readable raw PCM");
    let samples: Vec<f32> = bytes
        .chunks_exact(size_of::<f32>())
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte PCM chunk")))
        .collect();

    let mut cava =
        CavaBarProcessor::new(CavaConfig::new(RATE, SPECTRUM_BAND_COUNT)).expect("CAVA processor");
    let mut detector = BassPressureDetector::new(RATE);
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.22, 0.78, 0.74));

    for (index, chunk) in samples.chunks(CHUNK).enumerate() {
        let bands: [f32; SPECTRUM_BAND_COUNT] = cava
            .process(chunk)
            .try_into()
            .expect("the configured bar count");
        let pressure = detector.observe(chunk);
        engine.ingest(&SpectrumFrame::from_cava_bars(bands).with_bass_pressure(pressure));
        engine.tick();

        let seconds = (index * CHUNK) as f32 / RATE as f32;
        let Some((name, _)) = moments
            .iter()
            .find(|(_, at)| (seconds - at).abs() < CHUNK as f32 / RATE as f32 / 2.0)
        else {
            continue;
        };

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
        let path = format!("{out}/pressure-{name}.ppm");
        write_ppm(&mut surface, w as usize, h as usize, &path);
        println!(
            "{path}: t={seconds:.1}s bass={:.1} dBFS baseline={:.1} dBFS impact={:.2} aura={:.2}",
            pressure.level_dbfs, pressure.baseline_dbfs, pressure.impact, pressure.aura
        );
    }
}

fn write_ppm(surface: &mut gtk4::cairo::ImageSurface, width: usize, height: usize, path: &str) {
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    for y in 0..height {
        for x in 0..width {
            let offset = y * stride + x * 4;
            ppm.extend_from_slice(&[data[offset + 2], data[offset + 1], data[offset]]);
        }
    }
    drop(data);
    std::fs::write(path, ppm).unwrap();
}

#[test]
#[ignore = "visual gallery: renders the Bars scene to REPRISE_VIS_OUT for eyeballing"]
fn render_bars_gallery_ppm() {
    let out = std::env::var("REPRISE_VIS_OUT").unwrap_or_else(|_| "/tmp".to_owned());
    let (w, h) = (548.0_f32, 300.0_f32);

    for (name, scene) in [
        ("visualizer-idle", resting_engine(120).scene(w, h)),
        ("visualizer-idle-late", resting_engine(300).scene(w, h)),
        ("visualizer-bars", lively_engine().scene(w, h)),
    ] {
        let mut surface =
            gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, w as i32, h as i32)
                .unwrap();
        {
            let cr = gtk4::cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(0.078, 0.094, 0.102);
            let _ = cr.paint();
            render::draw_scene(&cr, &scene);
        }
        let path = format!("{out}/{name}.ppm");
        write_ppm(&mut surface, w as usize, h as usize, &path);
        println!("wrote {path}");
    }
}

#[test]
#[ignore = "diagnostic: measures the complete scene-build and Cairo-render frame budget"]
fn bars_fullscreen_render_budget_diagnostic() {
    use reprise_core::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};
    use std::time::Instant;

    const FRAMES: usize = 240;
    const FRAME_BUDGET_MS: f64 = 16.0;

    let mut over_budget = Vec::new();
    for (width, height) in [(548, 300), (960, 540), (1920, 1080)] {
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        let surface =
            gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, width, height).unwrap();
        let mut renderer = render::SceneRenderer::default();
        let mut timings = Vec::with_capacity(FRAMES);

        for frame in 0..FRAMES {
            let mut input = [0.45_f32; SPECTRUM_BAND_COUNT];
            if frame % 10 == 0 {
                input[..24].fill(0.98);
                input[24..].fill(0.78);
            }
            engine.ingest(&SpectrumFrame::from_cava_bars(input));

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
