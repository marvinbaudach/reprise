//! Display coverage for the Podcasts first-model loading page.

use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};

use super::*;

/// `FB-14`: a view waiting on its model shows the loading row and never the
/// previously rendered rows. The loading row owns the stack until `refresh()`
/// delivers the model, and its allocation is centred in that stack.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_14_podcasts_show_a_loading_row_until_the_model_arrives() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        &conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".to_owned(),
            title: "Episode".to_owned(),
            image_url: None,
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        2,
    )
    .unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    let view = PodcastsView::install(
        Rc::new(conn),
        runtime,
        PodcastsCallbacks::default(),
        PodcastKind::Rss,
    );
    let window = gtk4::Window::new();
    window.set_default_size(968, 800);
    window.set_child(Some(view.root()));
    window.present();

    view.begin_model_wait();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(view.stack.visible_child_name().as_deref(), Some("loading"));
    let bounds = view
        .loading_row
        .compute_bounds(&view.stack)
        .expect("loading row is allocated inside the podcasts stack");
    let stack_width = view.stack.width() as f32;
    assert!(
        (bounds.center().x() - stack_width / 2.0).abs() <= 1.0,
        "loading row must be horizontally centred in the stack"
    );

    view.refresh();
    assert_eq!(view.stack.visible_child_name().as_deref(), Some("list"));
    window.close();
}

/// A view that already holds rows keeps them on screen while a refresh runs.
/// The tab-open refresh follows `refresh()` immediately, so blanking the list
/// here would hide a fully loaded model for the whole network round trip.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_refresh_keeps_the_cached_rows_on_screen() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::online_sources::set_enabled(&conn, true).unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        &conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".to_owned(),
            title: "Episode".to_owned(),
            image_url: None,
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        2,
    )
    .unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    let view = PodcastsView::install(
        Rc::new(conn),
        runtime,
        PodcastsCallbacks::default(),
        PodcastKind::Rss,
    );
    let window = gtk4::Window::new();
    window.set_default_size(968, 800);
    window.set_child(Some(view.root()));
    window.present();

    view.refresh();
    assert_eq!(view.stack.visible_child_name().as_deref(), Some("list"));

    // No main-loop iteration here: the worker answers the unreachable feed
    // within milliseconds, and the bug window is the time before it does.
    assert!(
        view.request_refresh(true),
        "the refresh must reach the worker"
    );

    assert_eq!(
        view.stack.visible_child_name().as_deref(),
        Some("list"),
        "cached rows stay on screen while the refresh runs"
    );
    assert!(view.footer.is_visible());
    assert!(view.refresh_spinner.is_spinning());
    window.close();
}

/// A view without rows has nothing to keep: the loading row owns the stack
/// while the refresh runs, rather than an empty state the fetch may contradict.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_refresh_without_rows_shows_the_loading_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::online_sources::set_enabled(&conn, true).unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    let view = PodcastsView::install(
        Rc::new(conn),
        runtime,
        PodcastsCallbacks::default(),
        PodcastKind::Rss,
    );
    let window = gtk4::Window::new();
    window.set_default_size(968, 800);
    window.set_child(Some(view.root()));
    window.present();

    view.refresh();
    assert!(view.rows.borrow().is_empty());

    assert!(
        view.request_refresh(true),
        "the refresh must reach the worker"
    );

    assert_eq!(view.stack.visible_child_name().as_deref(), Some("loading"));
    window.close();
}
