use super::*;

fn loaded_track() -> NowPlaying {
    NowPlaying {
        id: 7,
        title: "Loaded title".into(),
        artist: "Loaded artist".into(),
        album: "Loaded album".into(),
        art_url: None,
        duration_ms: 180_000,
        path: "/tmp/loaded.mp3".into(),
    }
}

#[test]
fn loaded_track_presentation_is_identical_while_playing_or_paused() {
    let track = loaded_track();
    let playing = panel_presentation(Some(&track), PlaybackState::Playing);
    let paused = panel_presentation(Some(&track), PlaybackState::Paused);

    assert_eq!(playing, paused);
    assert_eq!(playing.title, "Loaded title");
    assert_eq!(playing.subtitle, "Loaded artist · Loaded album");
    assert!(!playing.idle);
}

#[test]
fn no_loaded_track_uses_the_idle_presentation() {
    let presentation = panel_presentation(None, PlaybackState::Stopped);

    assert_eq!(presentation.title, "Nothing playing");
    assert_eq!(presentation.subtitle, "");
    assert!(presentation.idle);
}

#[test]
fn now_playing_panel_visibility_round_trips_through_settings() {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    assert!(reprise_core::library::settings::get_info_panel_visible(
        &conn
    ));
    reprise_core::library::settings::set_info_panel_visible(&conn, false).unwrap();
    assert!(!reprise_core::library::settings::get_info_panel_visible(
        &conn
    ));
}

#[test]
fn now_playing_panel_has_the_fixed_npp_width() {
    assert_eq!(PANEL_WIDTH, 300);
}

#[test]
fn now_playing_css_defines_the_21a_stage_head_and_glow() {
    let css = css();

    assert!(css.contains(".reprise-now-playing-stage"));
    assert!(css.contains("background-color: #17191c"));
    assert!(!css.contains("@sidebar_bg_color"));
    assert!(css.contains(".reprise-now-playing-glow"));
    assert!(css.contains("radial-gradient"));
    assert!(css.contains("alpha(@reprise_player_accent, 0.4)"));
    assert!(css.contains(".reprise-now-playing-idle .reprise-now-playing-glow"));
    assert!(css.contains("border-radius: 12px"));
    assert!(css.contains("font-size: 15px"));
    assert!(css.contains("font-size: 12px"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn now_playing_css_parses_without_gtk_errors() {
    gtk4::init().unwrap();
    let errors = crate::ui::style::css_parse_errors(&css());
    assert!(errors.is_empty(), "GTK reported CSS errors: {errors:?}");
}

#[test]
fn now_playing_css_defines_the_two_segment_pill_and_footer() {
    let css = css();

    assert!(css.contains(".reprise-now-playing-tabs"));
    assert!(css.contains("border-radius: 99px"));
    assert!(css.contains("alpha(#ffffff, 0.06)"));
    assert!(css.contains("alpha(#ffffff, 0.14)"));
    assert!(css.contains(".reprise-now-playing-footer"));
    assert!(css.contains("font-size: 10.5px"));
    assert!(css.contains("alpha(#ffffff, 0.35)"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn panel_has_no_local_header_refresh_or_close_buttons() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = build_widgets(&content, true);
    let tree = widget_tree(widgets.column.widget().upcast_ref());

    assert!(tree.iter().all(|widget| !widget.is::<adw::HeaderBar>()));
    let icon_names = tree
        .iter()
        .filter_map(|widget| widget.clone().downcast::<gtk4::Button>().ok())
        .filter_map(|button| button.icon_name())
        .collect::<Vec<_>>();
    assert!(!icon_names
        .iter()
        .any(|name| { name == "view-refresh-symbolic" || name == "window-close-symbolic" }));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
#[allow(deprecated)]
fn npp_2_no_volume_in_panel() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = build_widgets(&content, true);
    let tree = widget_tree(widgets.column.widget().upcast_ref());

    assert!(tree.iter().all(|widget| !widget.is::<gtk4::VolumeButton>()));
    assert!(tree.iter().all(|widget| !widget.is::<gtk4::Scale>()));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn head_and_pill_match_the_21a_structure() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = build_widgets(&content, true);

    assert_eq!(widgets.cover.pixel_size(), 168);
    assert_eq!(widgets.cover.width_request(), 168);
    assert_eq!(widgets.cover.height_request(), 168);
    assert!(widgets.title.has_css_class("reprise-now-playing-title"));
    assert!(widgets
        .subtitle
        .has_css_class("reprise-now-playing-subtitle"));
    assert_eq!(widgets.tab_buttons.len(), 2);
    assert_eq!(widgets.tab_buttons[0].label().as_deref(), Some("Up Next"));
    assert_eq!(widgets.tab_buttons[1].label().as_deref(), Some("Lyrics"));
    assert!(widgets
        .tab_buttons
        .iter()
        .all(|button| button.has_css_class("reprise-now-playing-tab")));
    assert!(widgets.footer.has_css_class("reprise-now-playing-footer"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn idle_uses_a_placeholder_cover_without_the_accent_glow() {
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingIdleTest");
    panel.retain_for_window(&window);

    assert_eq!(panel.widgets.title.text(), "Nothing playing");
    assert_eq!(
        panel.widgets.cover.icon_name().as_deref(),
        Some("audio-x-generic-symbolic")
    );
    assert!(panel
        .widgets
        .stage
        .has_css_class("reprise-now-playing-idle"));
    panel.set_loaded_track(Some(loaded_track()));
    assert!(!panel
        .widgets
        .stage
        .has_css_class("reprise-now-playing-idle"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn loaded_and_idle_tracks_render_from_the_player_context() {
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingContextTest");
    panel.retain_for_window(&window);

    panel.set_loaded_track(Some(loaded_track()));
    panel.set_playback_state(PlaybackState::Playing);
    assert_eq!(panel.widgets.title.text(), "Loaded title");
    assert_eq!(
        panel.widgets.subtitle.text(),
        "Loaded artist · Loaded album"
    );
    panel.set_playback_state(PlaybackState::Paused);
    assert_eq!(panel.widgets.title.text(), "Loaded title");

    panel.set_loaded_track(None);
    assert_eq!(panel.widgets.title.text(), "Nothing playing");
    assert!(panel.widgets.subtitle.text().is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fixed_panel_owner_survives_and_header_toggle_reopens_it() {
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingOwnerTest");
    panel.retain_for_window(&window);
    let weak = Rc::downgrade(&panel);
    let toggle = panel.toggle_button();

    drop(panel);
    assert!(weak.upgrade().is_some());
    toggle.set_active(false);
    assert!(!weak.upgrade().unwrap().widgets.column.is_visible());
    toggle.set_active(true);
    assert!(weak.upgrade().unwrap().widgets.column.is_visible());
}

fn test_panel(application_id: &str) -> (adw::ApplicationWindow, Rc<NowPlayingPanel>) {
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    let portraits = ArtistPortraitRuntime::setup();
    let cover_runtime = crate::ui::cover_download_worker::setup();
    let cover_loader = CoverLoader::new(cover_runtime);
    let app = adw::Application::builder()
        .application_id(application_id)
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let panel = NowPlayingPanel::new(&content, &window, conn, runtime, &portraits, cover_loader);
    (window, panel)
}

fn widget_tree(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut widgets = vec![root.clone()];
    let mut index = 0;
    while index < widgets.len() {
        let mut child = widgets[index].first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            widgets.push(widget);
        }
        index += 1;
    }
    widgets
}
