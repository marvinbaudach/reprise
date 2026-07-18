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
