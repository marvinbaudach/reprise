use super::*;
use reprise_core::artist_news::{AlbumNews, ArtistNews, NewsKind};

fn release(title: &str, kind: NewsKind) -> AlbumNews {
    AlbumNews {
        release_group_mbid: "11111111-1111-1111-1111-111111111111".into(),
        title: title.into(),
        first_release_date: "2026-10-01".into(),
        primary_type: "Album".into(),
        kind,
    }
}

#[test]
fn information_panel_visibility_round_trips_through_settings() {
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
fn disabled_plugin_always_renders_privacy_card() {
    assert_eq!(render_kind(false, false, None), RenderKind::Disabled);
}

#[test]
fn pending_failure_and_cached_results_have_distinct_render_states() {
    assert_eq!(render_kind(true, true, None), RenderKind::Loading);
    assert_eq!(
        render_kind(true, false, Some(Err("offline"))),
        RenderKind::Error
    );
    let cached = ArtistNews {
        artist: "Artist".into(),
        artist_mbid: "id".into(),
        fetched_at: 1,
        items: vec![release("Album", NewsKind::New)],
        stale: true,
    };
    assert_eq!(
        render_kind(true, false, Some(Ok(&cached))),
        RenderKind::CachedNews(1)
    );
}

#[test]
fn fresh_empty_and_populated_results_render_separately() {
    let empty = ArtistNews {
        artist: "Artist".into(),
        artist_mbid: "id".into(),
        fetched_at: 1,
        items: vec![],
        stale: false,
    };
    let news = ArtistNews {
        items: vec![
            release("Soon", NewsKind::Upcoming),
            release("New", NewsKind::New),
        ],
        ..empty.clone()
    };
    assert_eq!(
        render_kind(true, false, Some(Ok(&empty))),
        RenderKind::NoNews
    );
    assert_eq!(
        render_kind(true, false, Some(Ok(&news))),
        RenderKind::News(2)
    );
}

#[test]
fn release_accessible_name_contains_status_title_type_and_date() {
    assert_eq!(
        release_accessible_name(&release("Future Album", NewsKind::Upcoming)),
        "Upcoming: Future Album, Album, 2026-10-01"
    );
}

#[test]
fn release_group_uri_accepts_only_a_musicbrainz_mbid() {
    assert_eq!(
        release_group_uri("11111111-1111-1111-1111-111111111111").as_deref(),
        Some("https://musicbrainz.org/release-group/11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(release_group_uri("../outside"), None);
}

#[test]
fn provider_errors_have_specific_match_copy_and_generic_network_copy() {
    assert_eq!(
        news_error_text(&NewsError::Unmatched),
        strings::text(strings::NEWS_UNMATCHED)
    );
    assert_eq!(
        news_error_text(&NewsError::Ambiguous),
        strings::text(strings::NEWS_AMBIGUOUS)
    );
    assert_eq!(
        news_error_text(&NewsError::Fetch(
            reprise_core::musicbrainz::FetchError::Timeout
        )),
        strings::text(strings::NEWS_ERROR)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn information_panel_uses_split_view_with_end_sidebar() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = build_widgets(&content, true);
    let split = widgets.column.widget();

    assert_eq!(split.sidebar_position(), gtk4::PackType::End);
    assert!(!split.is_collapsed());
    assert!(split.shows_sidebar());
    let sidebar = widgets.column.sidebar_widget();
    assert_eq!(sidebar.width_request(), PANEL_WIDTH);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn widget_keeps_information_beside_instead_of_over_content() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = build_widgets(&content, true);
    assert!(widgets.column.is_visible());
    assert!(!widgets.header.shows_start_title_buttons());
    assert!(!widgets.header.shows_end_title_buttons());
    let pages = widgets.stack.pages();
    assert_eq!(pages.n_items(), 2);
    let information = pages
        .item(0)
        .unwrap()
        .downcast::<gtk4::StackPage>()
        .unwrap();
    let lyrics = pages
        .item(1)
        .unwrap()
        .downcast::<gtk4::StackPage>()
        .unwrap();
    assert_eq!(information.name().as_deref(), Some("information"));
    assert_eq!(lyrics.name().as_deref(), Some("lyrics"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyrics_context_is_independent_and_survives_panel_close() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    let portraits = crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
    let cover_runtime = crate::ui::cover_download_worker::setup();
    let cover_loader = CoverLoader::new(cover_runtime);
    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.LyricsPanelTest")
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let panel = InfoPanel::new(&content, &window, conn, runtime, &portraits, cover_loader);

    panel.set_context(PanelContext::Multiple(2));
    let information_title = panel.widgets.title.text();
    let lyrics = panel.lyrics_view();
    panel.widgets.stack.set_visible_child_name("lyrics");
    lyrics.show_loading("Playback title", "Playback artist");
    assert_eq!(panel.widgets.title.text(), information_title);
    assert_eq!(lyrics.visible_state_name().as_deref(), Some("loading"));
    assert!(panel.widgets.progress.is_visible());
    assert!(!panel.widgets.refresh.is_sensitive());

    let retries = Rc::new(Cell::new(0));
    let retries_called = retries.clone();
    lyrics.set_on_retry(move || retries_called.set(retries_called.get() + 1));
    lyrics.show_error(&reprise_core::lyrics::LyricsError::Temporary);
    assert!(!panel.widgets.progress.is_visible());
    assert!(panel.widgets.refresh.is_sensitive());
    panel.widgets.refresh.emit_clicked();
    assert_eq!(retries.get(), 1);

    lyrics.show_result(&reprise_core::lyrics::LyricsBody::Plain(
        "synthetic panel text".into(),
    ));
    panel.widgets.close.emit_clicked();
    assert!(!panel.widgets.column.is_visible());
    assert_eq!(lyrics.line_labels().len(), 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn multiple_selection_uses_a_finished_empty_state_without_refresh() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    reprise_core::modules::set_enabled(
        &conn.borrow(),
        &reprise_core::modules::ARTIST_NEWS_MODULE,
        true,
    )
    .unwrap();
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    let portraits = crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
    let cover_runtime = crate::ui::cover_download_worker::setup();
    let cover_loader = CoverLoader::new(cover_runtime);
    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.InfoPanelMultipleTest")
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let panel = InfoPanel::new(&content, &window, conn, runtime, &portraits, cover_loader);

    panel.set_context(PanelContext::Multiple(4));

    assert!(!panel.widgets.refresh.is_visible());
    assert!(!panel.widgets.local.is_visible());
    assert!(panel.widgets.body.vexpands());
    assert_eq!(
        panel.widgets.header.centering_policy(),
        adw::CenteringPolicy::Strict
    );
    let status = panel
        .widgets
        .local
        .next_sibling()
        .unwrap()
        .downcast::<adw::StatusPage>()
        .unwrap();
    assert_eq!(status.title(), "4 tracks selected");
    assert_eq!(
        status.description().as_deref(),
        Some("Artist News is paused while multiple tracks are selected.")
    );
    assert!(status.vexpands());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fixed_panel_owner_survives_and_header_toggle_reopens_it() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    let portraits = crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
    let cover_runtime = crate::ui::cover_download_worker::setup();
    let cover_loader = CoverLoader::new(cover_runtime);
    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.InfoPanelTest")
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let panel = InfoPanel::new(&content, &window, conn, runtime, &portraits, cover_loader);
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
