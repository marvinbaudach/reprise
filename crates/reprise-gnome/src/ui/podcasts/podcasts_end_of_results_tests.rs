use gtk4::prelude::*;
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};

use super::*;

fn view_with_episodes(kind: PodcastKind, titles: &[&str]) -> Rc<PodcastsView> {
    let conn = crate::test_db::open().unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind,
            feed_url: format!("https://example.test/{kind:?}"),
            title: "A show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    for (index, title) in titles.iter().enumerate() {
        store::upsert_episode(
            &conn,
            subscription_id,
            &ParsedEpisode {
                guid: format!("episode-{index}"),
                title: (*title).to_owned(),
                image_url: None,
                audio_url: format!("https://example.test/{index}.mp3"),
                page_url: None,
                published_at: Some(index as i64 + 1),
                duration_secs: None,
            },
            2,
        )
        .unwrap()
        .unwrap();
    }
    let runtime = PodcastsRuntime::setup(&conn);
    PodcastsView::install(Rc::new(conn), runtime, PodcastsCallbacks::default(), kind)
}

fn descendant_with_class<T: IsA<gtk4::Widget> + Clone + 'static>(
    widget: &gtk4::Widget,
    class: &str,
) -> Option<T> {
    if widget.has_css_class(class) {
        if let Ok(found) = widget.clone().downcast::<T>() {
            return Some(found);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = descendant_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn assert_filtered_end_line(kind: PodcastKind, noun: &str) {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let view = view_with_episodes(kind, &["Afd dispatch", "Different title"]);
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();

    view.set_search_query("afd");
    crate::ui::source_context_surface::settle_layout();

    let line = descendant_with_class::<gtk4::Label>(
        view.root(),
        crate::ui::end_of_results::LINE_CSS_CLASS,
    )
    .expect("the list has an end-of-results line");
    assert_eq!(
        line.text(),
        format!("End of results — 1 {noun} hidden by search “afd”")
    );
    assert!(line.is_visible());

    let recovery = descendant_with_class::<gtk4::Button>(
        view.root(),
        crate::ui::end_of_results::RECOVERY_CSS_CLASS,
    )
    .expect("the list has an end-of-results recovery pill");
    let recovery_label = format!("Show all 2 {noun}s");
    assert_eq!(recovery.label().as_deref(), Some(recovery_label.as_str()));
    recovery.emit_clicked();
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(view.filter_bar.filter().query, "");
    assert!(!line.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_3a_podcasts_end_line_counts_episodes_and_recovers_with_clear_all() {
    assert_filtered_end_line(PodcastKind::Rss, "episode");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_3a_youtube_end_line_counts_videos_and_recovers_with_clear_all() {
    assert_filtered_end_line(PodcastKind::Youtube, "video");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_3a_end_line_stays_away_when_a_search_hides_nothing() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let view = view_with_episodes(PodcastKind::Rss, &["Afd one", "Afd two"]);
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();

    view.set_search_query("afd");
    crate::ui::source_context_surface::settle_layout();

    let line = descendant_with_class::<gtk4::Label>(
        view.root(),
        crate::ui::end_of_results::LINE_CSS_CLASS,
    )
    .expect("the list owns the shared end-of-results line");
    assert!(!line.is_visible());
}
