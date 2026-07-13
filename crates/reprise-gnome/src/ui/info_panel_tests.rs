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
fn panel_metrics_are_pinned_wide_and_overlay_narrow() {
    assert_eq!(
        panel_metrics(false),
        PanelMetrics {
            width: 340.0,
            pinned: true,
            collapsed: false
        }
    );
    assert_eq!(
        panel_metrics(true),
        PanelMetrics {
            width: 340.0,
            pinned: false,
            collapsed: true
        }
    );
}

#[test]
fn ordinary_desktop_width_uses_a_fixed_information_column() {
    assert!(panel_metrics_for_width(1_000.0).pinned);
    assert!(!panel_metrics_for_width(900.0).pinned);
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
fn widget_exposes_information_sidebar_metrics() {
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = build_widgets(&content, true);
    assert_eq!(widgets.split.sidebar_position(), gtk4::PackType::End);
    assert_eq!(widgets.split.min_sidebar_width(), 340.0);
    assert_eq!(widgets.split.max_sidebar_width(), 340.0);
    assert!(!widgets.split.is_pin_sidebar());
    assert!(widgets.split.is_collapsed());
    assert!(!widgets.header.shows_start_title_buttons());
    assert!(!widgets.header.shows_end_title_buttons());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pinned_panel_owner_survives_and_header_toggle_reopens_it() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let runtime = ArtistNewsRuntime::setup(&conn.borrow());
    let cover_runtime = crate::ui::cover_download_worker::setup(&conn.borrow());
    let cover_loader = CoverLoader::new(cover_runtime);
    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.InfoPanelTest")
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let panel = InfoPanel::new(&content, &window, conn, runtime, cover_loader);
    panel.widgets.split.set_collapsed(false);
    panel.widgets.split.set_pin_sidebar(true);
    panel.retain_for_window(&window);
    let weak = Rc::downgrade(&panel);
    let toggle = panel.toggle_button();

    drop(panel);
    assert!(weak.upgrade().is_some());
    toggle.set_active(false);
    assert!(!weak.upgrade().unwrap().widgets.split.shows_sidebar());
    toggle.set_active(true);
    assert!(weak.upgrade().unwrap().widgets.split.shows_sidebar());
}
