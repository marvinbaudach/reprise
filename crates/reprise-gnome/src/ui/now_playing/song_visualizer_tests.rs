use super::*;

#[test]
fn ac_23_visual_chrome_is_a_bars_only_canvas() {
    let css = css();
    assert!(css.contains("color: @reprise_player_accent"));
    assert!(css.contains(".reprise-song-visual-canvas"));
    assert!(!css.contains(".reprise-song-visual-modes"));
}

#[test]
fn visualizer_has_no_cover_color_input() {
    let visualizer = include_str!("song_visualizer.rs");
    assert!(!visualizer.contains(&["set_", "cover"].concat()));
    assert!(!visualizer.contains(&["downscale_cover", "_rgba"].concat()));
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
    let values = analysis_values(
        BassPressure {
            level_dbfs: -14.2,
            baseline_dbfs: -20.0,
            impact: 0.42,
            aura: 0.0,
            kick: 0.77,
            pressure: 0.61,
        },
        0.44,
    );

    // Whole decibels: a tenth of a dB is neither readable at this refresh rate
    // nor affordable in a 300 px panel, where it truncated "Baseline".
    assert_eq!(values.len(), 6);
    assert_eq!(values[0], "-14 dBFS");
    assert_eq!(values[1], "-20 dBFS");
    // Bass, Baseline, Breakdown, Kick, Pressure, Swell — `impact` drives
    // nothing since the glow became a stage light, so it is not shown.
    assert_eq!(values[2], "0.00");
    assert_eq!(values[3], "0.77");
    assert_eq!(values[4], "0.61");
    assert_eq!(values[5], "0.44");
}

#[test]
fn ac_23_a_silent_analysis_reads_as_a_dash_instead_of_a_bottomed_out_level() {
    let values = analysis_values(
        BassPressure {
            level_dbfs: -140.0,
            baseline_dbfs: -140.0,
            impact: 0.0,
            aura: 0.0,
            kick: 0.0,
            pressure: 0.0,
        },
        0.0,
    );

    assert_eq!(values[0], "—");
    assert_eq!(values[1], "—");
    assert_eq!(values.len(), 6);
    for value in &values[2..] {
        assert_eq!(value, "0.00");
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_23_the_readout_fits_in_the_strip_left_under_the_canvas() {
    // The panel is a fixed 300 px wide and the canvas takes everything above,
    // leaving roughly one strip. A readout taller than that is silently
    // clipped — the live session showed Impact and Breakdown cut off. The
    // strip grew by the 12 px the canvas gave back when Kick and Pressure
    // added a third row; the two numbers move together and neither may be
    // raised on its own.
    const PANEL_WIDTH: i32 = 300;
    const STRIP_HEIGHT: i32 = 64;

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
    crate::ui::style::install_css_string_for_test(&css());

    let readout = AnalysisReadout::new();
    readout.set(
        BassPressure {
            level_dbfs: -41.7,
            baseline_dbfs: -41.1,
            impact: 0.35,
            aura: 0.95,
            kick: 0.0,
            pressure: 0.0,
        },
        0.0,
    );

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

    // The panel owns the envelope and the readout only reports it, so the
    // swell has to be in place before the frame that writes the labels.
    visualizer.set_swell(0.45);
    visualizer.set_spectrum(
        SpectrumFrame::from_cava_bars([0.5; SPECTRUM_BAND_COUNT]).with_bass_pressure(
            BassPressure {
                level_dbfs: -11.4,
                baseline_dbfs: -19.0,
                impact: 0.87,
                aura: 0.31,
                kick: 0.64,
                pressure: 0.72,
            },
        ),
    );

    // Six values, each distinct, so the assertion pins the *order* and not
    // just the presence of some numbers.
    let shown = visualizer.readout.shown_values();
    assert_eq!(shown.len(), 6);
    assert_eq!(shown[0], "-11 dBFS"); // Bass
    assert_eq!(shown[1], "-19 dBFS"); // Baseline
    assert_eq!(shown[2], "0.31"); // Breakdown (aura)
    assert_eq!(shown[3], "0.64"); // Kick
    assert_eq!(shown[4], "0.72"); // Pressure
    assert_eq!(shown[5], "0.45"); // Swell

    // `impact` is produced but no longer displayed: since the glow became a
    // stage light driven by `kick`, nothing reads it, and AC-23 asks this strip
    // to name the analysis the visual actually reacts to.
    assert!(!shown.contains(&"0.87".to_owned()));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_27_loading_a_track_reaches_the_engine_and_rests_the_canvas_alive() {
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

/// Streams every input that the Bars renderer sees from decoded PCM.
///
/// `REPRISE_VIS_OUT` receives `bands.csv` and `pressure.csv`. Set
/// `REPRISE_VIS_WRITE_RGB=1` to also write the rendered Cairo frames as packed
/// RGB bytes to `frames.rgb`; the asset packer deliberately skips that large
/// diagnostic output.
#[test]
#[ignore = "measurement: needs REPRISE_VIS_PCM (raw mono f32 44.1 kHz)"]
fn dump_song_visualizer_stream() {
    use reprise_core::playback::{
        BassPressureDetector, CavaBarProcessor, CavaConfig, SpectrumFrame, SPECTRUM_BAND_COUNT,
    };
    use std::io::Write as _;

    const SAMPLE_RATE: u32 = 44_100;
    const CHUNK_SAMPLES: usize = 1_024;
    const DEFAULT_RENDER_WIDTH: f32 = 663.0;
    const DEFAULT_RENDER_HEIGHT: f32 = 652.0;

    let pcm_path = std::env::var("REPRISE_VIS_PCM").expect("REPRISE_VIS_PCM");
    let output_dir = std::env::var("REPRISE_VIS_OUT").expect("REPRISE_VIS_OUT");
    let render_width = visualizer_measurement_dimension("REPRISE_VIS_W", DEFAULT_RENDER_WIDTH);
    let render_height = visualizer_measurement_dimension("REPRISE_VIS_H", DEFAULT_RENDER_HEIGHT);
    let write_rgb = std::env::var("REPRISE_VIS_WRITE_RGB").as_deref() == Ok("1");

    std::fs::create_dir_all(&output_dir).expect("writable REPRISE_VIS_OUT");
    let bytes = std::fs::read(&pcm_path).expect("readable raw PCM");
    let samples: Vec<f32> = bytes
        .chunks_exact(size_of::<f32>())
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte PCM chunk")))
        .collect();

    let mut cava = CavaBarProcessor::new(CavaConfig::new(SAMPLE_RATE, SPECTRUM_BAND_COUNT))
        .expect("CAVA processor");
    let mut detector = BassPressureDetector::new(SAMPLE_RATE);
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_has_track(true);
    engine.set_accent((0.22, 0.78, 0.74));

    let mut bands_output =
        std::io::BufWriter::new(std::fs::File::create(format!("{output_dir}/bands.csv")).unwrap());
    let mut pressure_output = std::io::BufWriter::new(
        std::fs::File::create(format!("{output_dir}/pressure.csv")).unwrap(),
    );
    let mut rgb_output = write_rgb.then(|| {
        std::io::BufWriter::new(std::fs::File::create(format!("{output_dir}/frames.rgb")).unwrap())
    });
    let mut frame_count = 0_usize;

    for chunk in samples.chunks(CHUNK_SAMPLES) {
        let bands: [f32; SPECTRUM_BAND_COUNT] = cava
            .process(chunk)
            .try_into()
            .expect("the configured bar count");
        let pressure = detector.observe(chunk);
        engine.ingest(&SpectrumFrame::from_cava_bars(bands).with_bass_pressure(pressure));
        engine.tick();

        let band_line: Vec<String> = bands.iter().map(|value| format!("{value:.5}")).collect();
        writeln!(bands_output, "{}", band_line.join(",")).unwrap();
        writeln!(
            pressure_output,
            "{:.5},{:.5},{:.5}",
            pressure.kick, pressure.impact, pressure.aura
        )
        .unwrap();

        if let Some(output) = &mut rgb_output {
            write_visualizer_rgb_frame(output, &engine, render_width, render_height);
        }
        frame_count += 1;
    }

    bands_output.flush().unwrap();
    pressure_output.flush().unwrap();
    if let Some(output) = &mut rgb_output {
        output.flush().unwrap();
    }
    println!(
        "frames={frame_count} size={}x{} fps={:.3}",
        render_width as usize,
        render_height as usize,
        SAMPLE_RATE as f32 / CHUNK_SAMPLES as f32
    );
}

fn visualizer_measurement_dimension(name: &str, fallback: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn write_visualizer_rgb_frame(
    output: &mut impl std::io::Write,
    engine: &VisualEngine,
    width: f32,
    height: f32,
) {
    let scene = engine.scene(width, height);
    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, width as i32, height as i32)
            .unwrap();
    {
        let cr = gtk4::cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(0.078, 0.094, 0.102);
        let _ = cr.paint();
        render::draw_scene(&cr, &scene);
    }

    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut row = Vec::with_capacity(width as usize * 3);
    for y in 0..height as usize {
        row.clear();
        for x in 0..width as usize {
            let offset = y * stride + x * 4;
            row.extend_from_slice(&[data[offset + 2], data[offset + 1], data[offset]]);
        }
        output.write_all(&row).unwrap();
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
