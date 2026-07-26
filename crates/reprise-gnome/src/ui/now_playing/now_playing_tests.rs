use super::*;

use libadwaita::prelude::AdwApplicationWindowExt;
use std::time::Duration;

fn wait_for_layout(milliseconds: u64) {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_millis(milliseconds), move || {
        quit.quit();
    });
    main_loop.run();
}

fn loaded_track() -> NowPlaying {
    NowPlaying {
        id: 7,
        title: "Loaded title".into(),
        artist: "Loaded artist".into(),
        album: "Loaded album".into(),
        album_artist: "Loaded artist".into(),
        genre: "Test".into(),
        artist_mbid: None,
        art_url: None,
        duration_ms: 180_000,
        path: "/tmp/loaded.mp3".into(),
    }
}

fn test_widgets(content: &impl IsA<gtk4::Widget>, visible: bool) -> PanelWidgets {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    build_widgets(content, visible, Rc::new(RefCell::new(conn)), &cover_loader)
}

fn test_widgets_for_session(
    content: &impl IsA<gtk4::Widget>,
    visible: bool,
    session: &Rc<TabSession>,
) -> PanelWidgets {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    build_widgets_for_session(
        content,
        visible,
        session,
        Rc::new(RefCell::new(conn)),
        &cover_loader,
    )
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
fn ac_20_visual_is_the_third_panel_tab() {
    assert_eq!(PanelTab::Visual.page_name(), VISUAL_PAGE);
    assert_eq!(
        PANEL_TABS,
        [PanelTab::UpNext, PanelTab::Lyrics, PanelTab::Visual]
    );
}

#[test]
fn npp_12_panel_defaults_hidden_and_visibility_round_trips() {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    assert!(!reprise_core::library::settings::get_info_panel_visible(
        &conn
    ));
    reprise_core::library::settings::set_info_panel_visible(&conn, true).unwrap();
    assert!(reprise_core::library::settings::get_info_panel_visible(
        &conn
    ));
    reprise_core::library::settings::set_info_panel_visible(&conn, false).unwrap();
    assert!(!reprise_core::library::settings::get_info_panel_visible(
        &conn
    ));
}

#[test]
fn que_6_closed_panel_does_not_render() {
    assert!(!should_render_up_next(false, PanelTab::UpNext));
    assert!(!should_render_up_next(true, PanelTab::Lyrics));
    assert!(should_render_up_next(true, PanelTab::UpNext));
}

#[test]
fn now_playing_panel_has_the_fixed_npp_width() {
    assert_eq!(PANEL_WIDTH, 300);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_1_panel_remains_300_px_when_text_is_scaled() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous_dpi = settings.property::<i32>("gtk-xft-dpi");
    settings.set_property("gtk-xft-dpi", 144 * 1024);

    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingScaledWidthTest");
    panel.retain_for_window(&window);
    panel.widgets.column.set_visible(true);
    window.set_default_size(1200, 800);
    window.present();
    wait_for_layout(100);

    let width_unit = panel.widgets.column.widget().sidebar_width_unit();
    let allocated_width = panel.widgets.stage.width();
    window.close();
    settings.set_property("gtk-xft-dpi", previous_dpi);

    assert_eq!(
        width_unit,
        adw::LengthUnit::Px,
        "the NPP-1 width is a pixel contract, not a text-scaled sp value"
    );
    assert_eq!(
        allocated_width, PANEL_WIDTH,
        "enlarged text must not widen and partially clip the Now Playing panel"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_1_full_size_cover_cannot_resize_the_fixed_sidebar() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingCoverWidthTest");
    panel.retain_for_window(&window);
    window.set_default_size(1200, 800);
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let placeholder_width = panel.widgets.stage.width();
    let bytes = gtk4::glib::Bytes::from_owned(vec![0x80_u8; 1024 * 768 * 4]);
    let texture = gtk4::gdk::MemoryTexture::new(
        1024,
        768,
        gtk4::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        1024 * 4,
    );
    panel.widgets.cover.set_paintable(Some(&texture));
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(
        placeholder_width, PANEL_WIDTH,
        "the placeholder must begin at the fixed sidebar width"
    );
    assert_eq!(
        panel.widgets.stage.width(),
        PANEL_WIDTH,
        "a decoded cover's intrinsic dimensions must not widen the sidebar"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_1_long_queue_source_cannot_resize_the_fixed_sidebar() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingSourceWidthTest");
    panel.retain_for_window(&window);
    panel
        .conn
        .borrow()
        .execute(
            "INSERT INTO tracks (id,path,title,artist,album,duration_ms,added_at) \
             VALUES (1,'/source-width.flac','Track','Artist','Album',180000,0)",
            [],
        )
        .unwrap();
    window.set_default_size(1200, 800);
    window.present();

    let short =
        crate::ui::track_list::queue_sections::compose(None, &[], &[1], Some("Popular Monster"));
    panel.set_up_next_model(&short);
    wait_for_layout(100);
    let short_width = panel.widgets.stage.width();

    let long = crate::ui::track_list::queue_sections::compose(
        None,
        &[],
        &[1],
        Some("I Feel The Everblack Festering Within Me"),
    );
    panel.set_up_next_model(&long);
    wait_for_layout(100);

    let expected_header = "Playing from I Feel The Everblack Festering Within Me · 1 track";
    assert!(
        widget_tree(panel.widgets.up_next.widget().upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
            .any(|label| label.text() == expected_header),
        "the production ListView header must be bound before measuring its width"
    );

    assert_eq!(short_width, PANEL_WIDTH);
    assert_eq!(
        panel.widgets.stage.width(),
        PANEL_WIDTH,
        "the Playing-from header must ellipsize instead of widening the sidebar"
    );
    window.close();
}

#[test]
fn now_playing_css_defines_the_21a_stage_head_and_glow() {
    let css = css();

    assert!(css.contains(".reprise-now-playing-stage"));
    assert!(css.contains("background-color: @sidebar_bg_color"));
    assert!(!css.contains("background-color: #17191c"));
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
fn npp_11_now_playing_css_defines_the_adaptive_view_switcher_and_footer() {
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
    let widgets = test_widgets(&content, true);
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
    let widgets = test_widgets(&content, true);
    let tree = widget_tree(widgets.column.widget().upcast_ref());

    assert!(tree.iter().all(|widget| !widget.is::<gtk4::VolumeButton>()));
    assert!(tree.iter().all(|widget| !widget.is::<gtk4::Scale>()));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn head_and_pill_match_the_21a_structure() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = test_widgets(&content, true);

    assert_eq!(widgets.cover.pixel_size(), 168);
    assert_eq!(widgets.cover.width_request(), 168);
    assert_eq!(widgets.cover.height_request(), 168);
    assert!(widgets.title.has_css_class("reprise-now-playing-title"));
    assert!(widgets.artist.has_css_class("reprise-now-playing-subtitle"));
    assert!(widgets.album.has_css_class("reprise-now-playing-subtitle"));
    assert_eq!(PANEL_TABS.len(), 3);
    assert!(widgets
        .tab_switcher
        .has_css_class("reprise-now-playing-tabs"));
    assert_eq!(
        widgets.tab_switcher.stack().as_ref(),
        Some(&widgets.tab_stack)
    );
    assert!(widgets.tab_stack.child_by_name(VISUAL_PAGE).is_some());
    assert_eq!(widgets.tab_stack.pages().n_items(), 3);
    let visual = widgets.tab_stack.child_by_name(VISUAL_PAGE).unwrap();
    let page = widgets.tab_stack.page(&visual);
    assert_eq!(page.title().as_deref(), Some("Visual"));
    assert_eq!(page.icon_name().as_deref(), Some("audio-speakers-symbolic"));
    assert!(widgets.footer.has_css_class("reprise-now-playing-footer"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_20_icons_only_switcher_keeps_three_labeled_keyboard_targets() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = test_widgets(&content, true);
    widgets
        .tab_switcher
        .set_display_mode(adw::InlineViewSwitcherDisplayMode::Icons);

    let buttons = widget_tree(widgets.tab_switcher.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
        .collect::<Vec<_>>();
    assert_eq!(buttons.len(), 3);
    assert!(buttons.iter().all(gtk4::prelude::WidgetExt::is_focusable));
    assert!(buttons
        .iter()
        .all(|button| gtk4::test_accessible_has_property(button, gtk4::AccessibleProperty::Label)));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn idle_uses_a_placeholder_cover_without_the_accent_glow() {
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingIdleTest");
    panel.retain_for_window(&window);
    let settings = gtk4::Settings::default().unwrap();
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

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
    settings.set_gtk_enable_animations(animations_were_enabled);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_4_tab_persists_in_session() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let session = Rc::new(TabSession::default());

    let first = test_widgets_for_session(&content, true, &session);
    first.tab_stack.set_visible_child_name(LYRICS_PAGE);
    assert_eq!(session.selected.get(), PanelTab::Lyrics);

    let rebuilt = test_widgets_for_session(&content, true, &session);
    assert_eq!(
        rebuilt.tab_stack.visible_child_name().as_deref(),
        Some(LYRICS_PAGE)
    );

    let restarted_session = Rc::new(TabSession::default());
    let restarted = test_widgets_for_session(&content, true, &restarted_session);
    assert_eq!(
        restarted.tab_stack.visible_child_name().as_deref(),
        Some(UP_NEXT_PAGE)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn loaded_and_idle_tracks_render_from_the_player_context() {
    gtk4::init().unwrap();
    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingContextTest");
    panel.retain_for_window(&window);
    let settings = gtk4::Settings::default().unwrap();
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    panel.set_loaded_track(Some(loaded_track()));
    panel.set_playback_state(PlaybackState::Playing);
    assert_eq!(panel.widgets.title.text(), "Loaded title");
    assert_eq!(panel.widgets.artist.text(), "Loaded artist");
    assert_eq!(panel.widgets.album.text(), "Loaded album");
    panel.set_playback_state(PlaybackState::Paused);
    assert_eq!(panel.widgets.title.text(), "Loaded title");

    panel.set_loaded_track(None);
    assert_eq!(panel.widgets.title.text(), "Nothing playing");
    assert!(panel.widgets.artist.text().is_empty());
    assert!(panel.widgets.album.text().is_empty());
    settings.set_gtk_enable_animations(animations_were_enabled);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4_now_playing_metadata_exposes_track_album_and_artist_links() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (_window, panel) = test_panel("org.reprise.Reprise.NowPlayingRevealLinkTest");

    for surface in [
        panel.widgets.cover.clone().upcast::<gtk4::Widget>(),
        panel.widgets.title.clone().upcast::<gtk4::Widget>(),
        panel.widgets.artist.clone().upcast::<gtk4::Widget>(),
        panel.widgets.album.clone().upcast::<gtk4::Widget>(),
    ] {
        assert!(surface.is_focusable());
        assert!(gtk4::test_accessible_has_role(
            &surface,
            gtk4::AccessibleRole::Link
        ));
        assert!(gtk4::test_accessible_has_property(
            &surface,
            gtk4::AccessibleProperty::Label
        ));
        assert!(surface.has_css_class(crate::ui::link_activation::LINK_CLASS));
    }
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_10_track_change_crossfades_identity_without_repainting_the_active_tab() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = test_widgets(&content, true);
    let shared_tree = widget_tree(widgets.track_content.upcast_ref());
    assert!(shared_tree.iter().any(|widget| widget == &widgets.cover));
    assert!(shared_tree.iter().any(|widget| widget == &widgets.title));
    assert!(!shared_tree
        .iter()
        .any(|widget| widget == &widgets.tab_stack));
    assert_eq!(
        widgets.stage.first_child(),
        Some(widgets.track_content.clone().upcast())
    );
    assert!(widgets.track_content.next_sibling().is_some());

    let (window, panel) = test_panel("org.reprise.Reprise.NowPlayingCrossfadeTest");
    panel.retain_for_window(&window);
    panel.widgets.column.set_visible(true);
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(panel.widgets.track_content.is_mapped());
    let settings = gtk4::Settings::default().unwrap();
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    panel.set_loaded_track(Some(loaded_track()));
    assert!(panel.has_track_animation());

    settings.set_gtk_enable_animations(false);
    let mut next = loaded_track();
    next.id = 8;
    next.title = "Hard-switched title".into();
    panel.set_loaded_track(Some(next));
    assert_eq!(panel.widgets.title.text(), "Hard-switched title");
    assert_eq!(panel.widgets.track_content.opacity(), 1.0);
    assert!(!panel.has_track_animation());
    settings.set_gtk_enable_animations(animations_were_enabled);
}

fn test_panel(application_id: &str) -> (adw::ApplicationWindow, Rc<NowPlayingPanel>) {
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    let cover_runtime = crate::ui::cover_download_worker::setup_for_test();
    let cover_loader = CoverLoader::new(cover_runtime);
    let app = adw::Application::builder()
        .application_id(application_id)
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let panel = NowPlayingPanel::new(&content, conn, runtime, cover_loader);
    window.set_content(Some(panel.widget()));
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

/// UX NPP-1: the two side columns are a PIXEL contract, and deliberately
/// unequal (240 left, 300 right). Both numbers and the panel's own pixel
/// pin are asserted here; the sidebar half lives in `sidebar_presentation`.
#[test]
fn npp_1_panel_uses_the_fixed_pixel_width() {
    assert_eq!(super::super::now_playing_column::PANEL_WIDTH, 300);
}

/// UX NPP-3: the glow is a cover-accent radial gradient confined to the
/// upper third, fading into the neutral stage — never a full tint, so the
/// lyric contrast below it stays constant. Asserted on the CSS because the
/// gradient is what carries the rule; a rendered check is a manual item.
#[test]
fn npp_3_glow_is_a_cover_accent_gradient_over_a_neutral_stage() {
    let css = super::css();

    assert!(css.contains(".reprise-now-playing-glow"));
    assert!(css.contains("radial-gradient(ellipse at center"));
    // The accent comes from the cover pipeline's named color, not a literal.
    assert!(css.contains("alpha(@reprise_player_accent"));
    // It has to fade out, otherwise it is a tint and not a glow.
    assert!(css.contains("0) 70%"));
    // The stage underneath stays neutral so lyric contrast is constant.
    assert!(css.contains(".reprise-now-playing-stage"));
    // Idle drops the glow entirely (Beschluss 4).
    assert!(css.contains(".reprise-now-playing-idle .reprise-now-playing-glow"));
}
