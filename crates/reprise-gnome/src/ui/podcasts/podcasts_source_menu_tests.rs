//! Source-menu contract tests kept separate from the near-limit menu module.

use gtk4::prelude::*;

use super::*;

fn source(kind: PodcastKind) -> SourceGroup {
    SourceGroup {
        subscription_id: 7,
        title: "Source".into(),
        author: None,
        image_url: None,
        kind,
        sync_to_phone: false,
        episodes: Vec::new(),
    }
}

fn contains_action(menu: &gio::MenuModel, action: &str) -> bool {
    (0..menu.n_items()).any(|index| {
        menu.item_attribute_value(index, gio::MENU_ATTRIBUTE_ACTION, None)
            .and_then(|value| value.get::<String>())
            .is_some_and(|candidate| candidate == action)
            || menu
                .item_link(index, gio::MENU_LINK_SECTION)
                .is_some_and(|section| contains_action(&section, action))
    })
}

/// `POD-10`: the channel page remains reachable from the existing source
/// menu, while RSS sources do not claim to have a channel page.
#[test]
fn pod_10_the_source_menu_opens_the_channel_page() {
    assert!(contains_action(
        build_source(&source(PodcastKind::Youtube)).upcast_ref(),
        "podcasts.open-channel"
    ));
    assert!(!contains_action(
        build_source(&source(PodcastKind::Rss)).upcast_ref(),
        "podcasts.open-channel"
    ));
}
