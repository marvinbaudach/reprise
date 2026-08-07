use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use super::{browser_url, build, build_for_selection};
use crate::ui::podcasts::podcasts_episode_files::EpisodePaths;

fn episode(id: i64, kind: PodcastKind) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 7,
        guid: format!("episode-{id}"),
        title: format!("Episode {id}"),
        show: "Show".into(),
        show_image_url: None,
        image_url: None,
        kind,
        audio_url: format!("https://example.test/{id}.mp3"),
        page_url: None,
        published_at: None,
        duration_secs: None,
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: 1,
        is_new: false,
        media_category: None,
    }
}

fn collect_actions(model: &gio::MenuModel, actions: &mut Vec<String>) {
    for item in 0..model.n_items() {
        if let Some(action) = model
            .item_attribute_value(item, "action", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>())
        {
            actions.push(action);
        }
        if let Some(section) = model.item_link(item, "section") {
            collect_actions(&section, actions);
        }
    }
}

fn menu_actions(menu: &gio::Menu) -> Vec<String> {
    let mut actions = Vec::new();
    collect_actions(menu.upcast_ref(), &mut actions);
    actions
}

fn open_in_browser_target(model: &gio::MenuModel) -> Option<i64> {
    for item in 0..model.n_items() {
        let action = model
            .item_attribute_value(item, "action", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>());
        if action.as_deref() == Some("podcasts.open-in-browser") {
            return model
                .item_attribute_value(item, "target", None)
                .and_then(|target| target.get::<i64>());
        }
        if let Some(section) = model.item_link(item, "section") {
            if let Some(target) = open_in_browser_target(&section) {
                return Some(target);
            }
        }
    }
    None
}

#[test]
fn src_4b_browser_url_uses_only_the_episode_web_page_for_its_source_kind() {
    let mut youtube = episode(1, PodcastKind::Youtube);
    youtube.audio_url = "https://www.youtube.com/watch?v=video-id".into();
    youtube.page_url = Some("https://ignored.example/episode".into());
    assert_eq!(browser_url(&youtube), Some(youtube.audio_url.as_str()));

    let mut rss_with_page = episode(2, PodcastKind::Rss);
    rss_with_page.page_url = Some("https://podcast.example/episodes/2".into());
    assert_eq!(
        browser_url(&rss_with_page),
        rss_with_page.page_url.as_deref()
    );

    let rss_without_page = episode(3, PodcastKind::Rss);
    assert_eq!(browser_url(&rss_without_page), None);
    assert_ne!(
        browser_url(&rss_without_page),
        Some(rss_without_page.audio_url.as_str()),
        "an RSS enclosure is media, not the episode web page"
    );

    let mut non_web = episode(4, PodcastKind::Rss);
    non_web.page_url = Some("file:///tmp/episode.html".into());
    assert_eq!(browser_url(&non_web), None);
}

#[test]
fn src_4b_open_in_browser_appears_exactly_when_browser_url_exists() {
    let mut youtube = episode(1, PodcastKind::Youtube);
    youtube.audio_url = "https://www.youtube.com/watch?v=video-id".into();
    let rss_without_page = episode(2, PodcastKind::Rss);

    for row in [&youtube, &rss_without_page] {
        let menu = build(row);
        let open_entries = menu_actions(&menu)
            .iter()
            .filter(|action| action.as_str() == "podcasts.open-in-browser")
            .count();
        assert_eq!(
            open_entries,
            usize::from(browser_url(row).is_some()),
            "menu visibility must follow browser_url for {}",
            row.title
        );
        assert_eq!(
            open_in_browser_target(menu.upcast_ref()),
            browser_url(row).map(|_| row.id),
            "the browser action targets the single context row"
        );
    }
}

#[test]
fn src_12b_multi_selection_hides_open_in_browser_instead_of_targeting_one_row() {
    let mut youtube = episode(1, PodcastKind::Youtube);
    youtube.audio_url = "https://www.youtube.com/watch?v=video-id".into();

    let paths = EpisodePaths::from_rows(&[]);
    let menu = build_for_selection(&youtube, &[youtube.id, 99], None, &paths);

    assert!(
        !menu_actions(&menu)
            .iter()
            .any(|action| action == "podcasts.open-in-browser"),
        "a multi-selection menu must contain batch actions only"
    );
    assert_eq!(open_in_browser_target(menu.upcast_ref()), None);
}
