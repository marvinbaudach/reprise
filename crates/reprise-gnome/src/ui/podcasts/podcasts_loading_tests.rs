//! Display coverage for the Podcasts first-model loading page.

use gtk4::prelude::*;
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};

use super::*;

/// `FB-13`: requesting fresh podcast data replaces the previously rendered
/// rows immediately. The loading row owns the stack until `refresh()` delivers
/// the first replacement model, and its allocation is centred in that stack.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_13_podcasts_show_a_loading_row_until_the_model_arrives() {
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
