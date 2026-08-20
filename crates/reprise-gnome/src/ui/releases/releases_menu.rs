//! Pure selection summary and menu model for Releases rows.

use gtk4::gio;
use reprise_core::artist_news_history::HistoryEntry;

use crate::ui::strings;

pub(super) const ACTION_GROUP: &str = "releases";
pub(super) const ACTION_HIDE: &str = "hide";
pub(super) const ACTION_RESTORE: &str = "restore";
pub(super) const ACTION_GO_TO_ARTIST: &str = "go-to-artist";
pub(super) const ACTION_GO_TO_ALBUM: &str = "go-to-album";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MenuSelection {
    pub count: usize,
    pub all_hidden: bool,
    pub single_artist: Option<String>,
    pub single_is_local: bool,
}

pub(super) fn summarize(entries: &[HistoryEntry]) -> MenuSelection {
    let single = (entries.len() == 1).then(|| &entries[0]);
    MenuSelection {
        count: entries.len(),
        all_hidden: !entries.is_empty() && entries.iter().all(|entry| entry.hidden),
        single_artist: single.map(|entry| entry.artist_name.clone()),
        single_is_local: single.is_some_and(|entry| entry.local_track_count > 0),
    }
}

pub(super) fn build(selection: &MenuSelection) -> gio::Menu {
    let menu = gio::Menu::new();
    if selection.count == 0 {
        return menu;
    }

    let primary = gio::Menu::new();
    if selection.all_hidden {
        primary.append(
            Some(&strings::show_releases_again_label(selection.count)),
            Some(&format!("{ACTION_GROUP}.{ACTION_RESTORE}")),
        );
    } else {
        primary.append(
            Some(&strings::hide_releases_label(selection.count)),
            Some(&format!("{ACTION_GROUP}.{ACTION_HIDE}")),
        );
    }
    menu.append_section(None, &primary);

    // CTX-4: navigation needs an unambiguous target, so it belongs to a
    // single row only. A hidden row keeps it -- hiding is about the releases
    // list, not about the library the row points into.
    if selection.single_artist.is_some() {
        let navigation = gio::Menu::new();
        navigation.append(
            Some(&strings::text(strings::RELEASES_GO_TO_ARTIST)),
            Some(&format!("{ACTION_GROUP}.{ACTION_GO_TO_ARTIST}")),
        );
        if selection.single_is_local {
            navigation.append(
                Some(&strings::text(strings::RELEASES_GO_TO_ALBUM)),
                Some(&format!("{ACTION_GROUP}.{ACTION_GO_TO_ALBUM}")),
            );
        }
        menu.append_section(None, &navigation);
    }
    menu
}

#[cfg(test)]
mod tests {
    use gtk4::gio::prelude::*;
    use gtk4::glib;

    use super::*;

    fn entry(mbid: &str, hidden: bool, local_track_count: i64) -> HistoryEntry {
        let mut entry = crate::ui::releases::test_entry(mbid);
        entry.hidden = hidden;
        entry.local_track_count = local_track_count;
        entry
    }

    fn labels(menu: &gio::Menu) -> Vec<String> {
        let mut found = Vec::new();
        for section in 0..menu.n_items() {
            let Some(items) = menu.item_link(section, gio::MENU_LINK_SECTION) else {
                continue;
            };
            for index in 0..items.n_items() {
                if let Some(label) = items
                    .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
                    .and_then(|value| value.get::<String>())
                {
                    found.push(label);
                }
            }
        }
        found
    }

    #[test]
    fn ctx_6_one_visible_release_offers_hide_without_a_count() {
        let menu = build(&summarize(&[entry("one", false, 0)]));
        assert_eq!(labels(&menu).first().map(String::as_str), Some("Hide"));
    }

    #[test]
    fn ctx_6_a_multi_selection_carries_the_count() {
        let selection = summarize(&[entry("one", false, 0), entry("two", false, 0)]);
        assert_eq!(
            labels(&build(&selection)).first().map(String::as_str),
            Some("Hide 2 releases")
        );
    }

    #[test]
    fn a_hidden_selection_offers_restore_instead_of_hide() {
        let menu = build(&summarize(&[entry("one", true, 0)]));
        assert_eq!(
            labels(&menu).first().map(String::as_str),
            Some("Show again")
        );
    }

    #[test]
    fn ctx_4_navigation_needs_exactly_one_row() {
        let single = build(&summarize(&[entry("one", false, 0)]));
        assert!(labels(&single).iter().any(|label| label == "Go to artist"));

        let many = build(&summarize(&[
            entry("one", false, 0),
            entry("two", false, 0),
        ]));
        assert!(
            !labels(&many).iter().any(|label| label == "Go to artist"),
            "a multi-selection has no unambiguous artist to navigate to"
        );
    }

    #[test]
    fn go_to_album_appears_only_when_the_library_actually_holds_tracks() {
        let absent = build(&summarize(&[entry("one", false, 0)]));
        assert!(!labels(&absent).iter().any(|label| label == "Go to album"));

        let present = build(&summarize(&[entry("one", false, 3)]));
        assert!(labels(&present).iter().any(|label| label == "Go to album"));
    }
}
