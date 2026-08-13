//! Display coverage for targeted Podcast artwork refresh.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};

use super::*;

fn view_with_one_episode() -> (Rc<PodcastsView>, i64) {
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, false)
        .unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed".into(),
            title: "Show".into(),
            author: None,
            image_url: Some("https://images.test/show.png".into()),
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let episode_id = store::upsert_episode(
        &conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".into(),
            title: "Episode".into(),
            image_url: Some("https://images.test/episode.png".into()),
            audio_url: "https://example.test/episode.mp3".into(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        2,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    let runtime = PodcastsRuntime::setup(&conn);
    let conn = Rc::new(conn);
    let view = PodcastsView::install(
        conn,
        runtime,
        PodcastsCallbacks::default(),
        PodcastKind::Rss,
    );
    view.expanded_sources.borrow_mut().insert(subscription_id);
    view.render();
    (view, episode_id)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_6_a_mapped_podcast_view_rebinds_artwork_without_rebuilding_rows() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (view, episode_id) = view_with_one_episode();
    let row_before = view.selection_widgets.borrow()[&episode_id].row.clone();

    view.refresh_visible_artwork();
    assert_eq!(view.selection_widgets.borrow()[&episode_id].row, row_before);

    let window = gtk4::Window::new();
    window.set_default_size(968, 800);
    window.set_child(Some(view.root()));
    window.present();
    crate::ui::source_context_surface::settle_layout();
    let images_before = descendants_with_class(view.root(), "reprise-source-image");
    assert!(!images_before.is_empty());
    reprise_core::modules::set_enabled(&view.conn, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
    crate::ui::podcasts::source_image::recompute_gate(&view.conn);

    view.refresh_visible_artwork();
    crate::ui::source_context_surface::settle_layout();

    let row_after = view.selection_widgets.borrow()[&episode_id].row.clone();
    let images_after = descendants_with_class(view.root(), "reprise-source-image");
    assert_eq!(
        row_after, row_before,
        "artwork refresh rebuilt the episode row"
    );
    assert_eq!(images_after.len(), images_before.len());
    assert!(images_before.iter().zip(&images_after).all(|(a, b)| a != b));
}

fn descendants_with_class(widget: &gtk4::Widget, class: &str) -> Vec<gtk4::Widget> {
    let mut found = widget
        .has_css_class(class)
        .then(|| widget.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let mut child = widget.first_child();
    while let Some(current) = child {
        found.extend(descendants_with_class(&current, class));
        child = current.next_sibling();
    }
    found
}
