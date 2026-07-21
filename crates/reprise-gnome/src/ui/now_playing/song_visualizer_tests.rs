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
    // Design-mock chrome: header/bottom scrims and the fullscreen title.
    assert!(css.contains(".reprise-fs-header-scrim"));
    assert!(css.contains(".reprise-fs-bottom-scrim"));
    assert!(css.contains(".reprise-fs-title"));

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
        ["Grid", "Bars", "Flow", "Pulse", "Particles", "Neon"]
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

/// End-to-end smoke test for the Task 9 fullscreen chrome: builds the whole
/// overlay (backdrop, vignette-painted canvas, header, bottom bar with seek
/// and volume scales, transport, mode pills) with a cover texture and
/// player hooks installed, and confirms it constructs and tears down without
/// panicking — the unit tests above only exercise pure formatting/CSS
/// helpers, not the widget tree itself.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_10_fullscreen_chrome_builds_and_tears_down_without_panicking() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.SongVisualizerFullscreenTest")
        .build();
    let window = adw::ApplicationWindow::new(&app);

    let visualizer = SongVisualizer::new();
    visualizer.set_track_meta("Song Title", "Artist · Album");
    visualizer.set_queue_position(2, 9);
    visualizer.set_next_up(Some("Up next: Another Song".to_owned()));
    visualizer.set_position(42_000, 210_000);
    visualizer.set_playback_state(PlaybackState::Playing);
    visualizer.set_player_hooks(PlayerHooks {
        previous: Rc::new(|| {}),
        play_pause: Rc::new(|| {}),
        stop: Rc::new(|| {}),
        next: Rc::new(|| {}),
        seek_to_ms: Rc::new(|_| {}),
        set_volume: Rc::new(|_| {}),
        initial_volume: 0.6,
    });

    visualizer.toggle_fullscreen(&window);
    assert!(visualizer.fullscreen_active.get());
    assert!(visualizer.fullscreen_chrome.borrow().is_some());

    // A live update while the overlay is open should reach the chrome
    // without panicking (this is what `set_position`/`set_playback_state`
    // do on every player tick while the window is up).
    visualizer.set_position(84_000, 210_000);
    visualizer.set_playback_state(PlaybackState::Paused);

    visualizer.close_fullscreen();
    assert!(!visualizer.fullscreen_active.get());
    assert!(visualizer.fullscreen_chrome.borrow().is_none());
}

/// Builds an engine that has just been hammered by a beat-then-drop: playing,
/// accented, 20 silent frames (settles the envelopes at rest), one
/// full-spectrum impact frame (fires the beat/kick off the silence-to-loud
/// flux jump — a real kick is broadband for an instant), then 9 frames of a
/// realistic bass-heavy sustain.
///
/// The sustain is intentionally *not* fed from silence directly: the
/// engine's per-band auto-gain (`SpectrumAnalyzer::ingest`, `playback.rs`)
/// tracks each band's own recent peak and snaps its ratio to `1.0` the
/// instant a band first exceeds that peak, so any spectrum shape held
/// constant over several frames converges to a uniform wall regardless of
/// its per-bin variation — which is exactly what made the Bars mode render
/// as a flat maxed-out slab instead of a spectrum silhouette. Leading with
/// one loud broadband frame seeds every band's auto-gain ceiling near 1.0;
/// the following bass-heavy, treble-light sustain then reads back as
/// genuine relative contrast (bass stays pinned near its ceiling, treble
/// reads well under its still-decaying one), which is what a real
/// kick-into-sustain transient looks like. Mirrors
/// `reprise_core::visuals::engine::lively_engine`, which is test-only and
/// not exported across the crate boundary.
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

/// Fullscreen layout regression: builds the real fullscreen overlay window
/// (`fullscreen::build`) with a cover texture, representative title/artist,
/// and player hooks installed, presents it under Xvfb, and confirms the
/// design mock's corner-anchored bottom bar: a small (84px) cover thumb
/// pinned to the bottom-LEFT corner, a mirror-image block pinned to the
/// bottom-RIGHT corner (the volume controls), and the seek/transport/pills
/// stack centered between them — rather than everything jammed into one
/// centered column (the bug this change fixes). Also dumps a screenshot to
/// `/tmp/fs-layout.png` for human inspection; the structural assertions
/// below are what actually gate the test; the PNG is best-effort evidence.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_10_fullscreen_layout_is_corner_anchored_per_design() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install();

    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.SongVisualizerFullscreenLayoutTest")
        .build();
    let parent = adw::ApplicationWindow::new(&app);

    let visualizer = SongVisualizer::new();
    visualizer.set_track_meta(
        "A Long Enough Title To Prove It Never Overlaps The Cover",
        "Artist · Album",
    );
    visualizer.set_queue_position(2, 9);
    visualizer.set_next_up(Some("Up next: Another Song".to_owned()));
    visualizer.set_position(42_000, 210_000);
    visualizer.set_playback_state(PlaybackState::Playing);
    visualizer.set_player_hooks(PlayerHooks {
        previous: Rc::new(|| {}),
        play_pause: Rc::new(|| {}),
        stop: Rc::new(|| {}),
        next: Rc::new(|| {}),
        seek_to_ms: Rc::new(|_| {}),
        set_volume: Rc::new(|_| {}),
        initial_volume: 0.6,
    });
    visualizer.set_cover(Some(dummy_cover_texture(64)));

    let window = fullscreen::build(&visualizer, &parent);
    window.present();
    pump();

    // 1. The cover thumb is capped to 84px — not the texture's natural size.
    {
        let chrome_ref = visualizer.fullscreen_chrome.borrow();
        let chrome = chrome_ref.as_ref().expect("chrome installed while open");
        assert_eq!(chrome.cover_thumb.pixel_size(), 84);

        // 2. The three bottom blocks (cover, controls, volume) are distinct
        // corner/center anchors, not one shared centered stack.
        let cover_block = chrome
            .cover_thumb
            .parent()
            .expect("cover thumb sits inside a corner block");
        assert_eq!(cover_block.halign(), gtk4::Align::Start);
        assert_eq!(cover_block.valign(), gtk4::Align::End);

        let bottom = cover_block
            .parent()
            .expect("the corner block sits directly on the bottom overlay");
        let siblings = widget_children(&bottom);
        assert_eq!(
            siblings.len(),
            3,
            "expected exactly 3 direct children of the bottom overlay \
             (centered controls + cover corner + volume corner)"
        );

        let anchors: Vec<(gtk4::Align, gtk4::Align)> =
            siblings.iter().map(|w| (w.halign(), w.valign())).collect();
        assert!(
            anchors.contains(&(gtk4::Align::Center, gtk4::Align::End)),
            "missing the centered controls column: {anchors:?}"
        );
        assert!(
            anchors.contains(&(gtk4::Align::Start, gtk4::Align::End)),
            "missing the bottom-left cover corner: {anchors:?}"
        );
        assert!(
            anchors.contains(&(gtk4::Align::End, gtk4::Align::End)),
            "missing the bottom-right volume corner: {anchors:?}"
        );

        // 3. The seek row (part of the centered stack) is NOT inside the
        // cover corner block — i.e. the old single-column jam is gone.
        let seek_ancestor_chain = ancestors(chrome.seek.upcast_ref::<gtk4::Widget>());
        assert!(
            !seek_ancestor_chain.contains(&cover_block),
            "the seek scale must not live inside the cover's corner block"
        );
    }

    // Best-effort screenshot for human inspection — not itself an assertion,
    // since headless GL/software renderers can vary across CI machines.
    if let Some(png) = screenshot_png(&window) {
        let _ = std::fs::write("/tmp/fs-layout.png", png);
    }

    window.close();
}

/// A small solid-color premultiplied-BGRA texture standing in for a real
/// album cover, so `apply_cover`'s texture (not `None`) path runs. Mirrors
/// the byte layout `backdrop_texture` (above, in the parent module) already
/// relies on for `gdk::MemoryTexture`.
fn dummy_cover_texture(edge: i32) -> gtk4::gdk::Texture {
    let stride = (edge * 4) as usize;
    let mut data = vec![0u8; stride * edge as usize];
    for pixel in data.chunks_mut(4) {
        pixel[0] = 40; // B
        pixel[1] = 120; // G
        pixel[2] = 200; // R
        pixel[3] = 255; // A
    }
    let bytes = gtk4::glib::Bytes::from_owned(data);
    gtk4::gdk::MemoryTexture::new(
        edge,
        edge,
        gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &bytes,
        stride,
    )
    .upcast()
}

/// A widget's direct children, oldest-first, via the generic
/// first-child/next-sibling walk every `gtk4::Widget` supports — used here
/// instead of a container-specific getter since `gtk4::Overlay` exposes no
/// "list my overlay children" API.
fn widget_children(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut children = Vec::new();
    let mut next = widget.first_child();
    while let Some(child) = next {
        next = child.next_sibling();
        children.push(child);
    }
    children
}

/// A widget's ancestor chain (parent, grandparent, …), used to confirm one
/// widget does NOT live inside another's subtree.
fn ancestors(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut chain = Vec::new();
    let mut current = widget.parent();
    while let Some(parent) = current {
        current = parent.parent();
        chain.push(parent);
    }
    chain
}

/// Iterates the main context for 300ms wall-clock time, letting the
/// presented window map, size-allocate, and settle before it's inspected or
/// rendered. Mirrors `style::buttons::tests::pump`.
fn pump() {
    let done = Rc::new(Cell::new(false));
    let done_setter = done.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        done_setter.set(true);
    });
    let context = gtk4::glib::MainContext::default();
    while !done.get() {
        context.iteration(true);
    }
}

/// The presented window's rendered pixels as an encoded PNG, or `None` if
/// the window has no GSK renderer (e.g. an unsupported headless backend) —
/// best-effort evidence only, see the caller.
fn screenshot_png(window: &gtk4::Window) -> Option<Vec<u8>> {
    let root = window.child()?;
    let paintable = gtk4::WidgetPaintable::new(Some(&root));
    let snapshot = gtk4::Snapshot::new();
    let width = f64::from(window.width().max(1));
    let height = f64::from(window.height().max(1));
    paintable.snapshot(&snapshot, width, height);
    let node = snapshot.to_node()?;
    let renderer = window.native().and_then(|native| native.renderer())?;
    Some(
        renderer
            .render_texture(&node, None)
            .save_to_png_bytes()
            .to_vec(),
    )
}
