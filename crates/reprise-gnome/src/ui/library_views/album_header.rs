//! Album count header + sort dropdown pill. The dropdown drives a
//! `GtkCustomSorter` exposed via `build_sorter`; the album view wraps
//! this in a `GtkSortListModel`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::queries::AlbumSummary;
use rusqlite::Connection;

use crate::ui::strings;

pub(in crate::ui) const ALBUM_SORT_SETTING_KEY: &str = "album_sort";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum AlbumSortKey {
    RecentlyAdded,
    Title,
    Artist,
    Year,
    MostPlayed,
}

impl AlbumSortKey {
    const ALL: [AlbumSortKey; 5] = [
        Self::RecentlyAdded,
        Self::Title,
        Self::Artist,
        Self::Year,
        Self::MostPlayed,
    ];

    fn setting_value(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "recently_added",
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Year => "year",
            Self::MostPlayed => "most_played",
        }
    }

    fn from_setting(value: &str) -> Self {
        match value {
            "title" => Self::Title,
            "artist" => Self::Artist,
            "year" => Self::Year,
            "most_played" => Self::MostPlayed,
            _ => Self::RecentlyAdded,
        }
    }

    fn label(self) -> String {
        strings::text(match self {
            Self::RecentlyAdded => strings::ALBUM_SORT_RECENTLY_ADDED,
            Self::Title => strings::ALBUM_SORT_TITLE,
            Self::Artist => strings::ALBUM_SORT_ARTIST,
            Self::Year => strings::ALBUM_SORT_YEAR,
            Self::MostPlayed => strings::ALBUM_SORT_MOST_PLAYED,
        })
    }
}

/// Reads the persisted sort key from the settings table, defaulting to
/// `RecentlyAdded`.
pub(in crate::ui) fn current_sort_key(conn: &Connection) -> AlbumSortKey {
    reprise_core::library::settings::get_setting(conn, ALBUM_SORT_SETTING_KEY)
        .ok()
        .flatten()
        .map(|v| AlbumSortKey::from_setting(&v))
        .unwrap_or(AlbumSortKey::RecentlyAdded)
}

/// Persists the selected sort key.
fn save_sort_key(conn: &Connection, key: AlbumSortKey) {
    let _ = reprise_core::library::settings::set_setting(
        conn,
        ALBUM_SORT_SETTING_KEY,
        key.setting_value(),
    );
}

/// Builds a `GtkCustomSorter` that compares `BoxedAnyObject<AlbumSummary>`
/// items according to the given sort key.
pub(in crate::ui) fn build_sorter(sort_key: AlbumSortKey) -> gtk4::CustomSorter {
    gtk4::CustomSorter::new(move |a, b| {
        let a = a
            .downcast_ref::<gtk4::glib::BoxedAnyObject>()
            .unwrap();
        let b = b
            .downcast_ref::<gtk4::glib::BoxedAnyObject>()
            .unwrap();
        let a: std::cell::Ref<AlbumSummary> = a.borrow();
        let b: std::cell::Ref<AlbumSummary> = b.borrow();
        let ordering = match sort_key {
            AlbumSortKey::RecentlyAdded => b.max_added_at.cmp(&a.max_added_at),
            AlbumSortKey::Title => a
                .album
                .to_lowercase()
                .cmp(&b.album.to_lowercase())
                .then_with(|| a.album_artist.to_lowercase().cmp(&b.album_artist.to_lowercase())),
            AlbumSortKey::Artist => a
                .album_artist
                .to_lowercase()
                .cmp(&b.album_artist.to_lowercase())
                .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase())),
            AlbumSortKey::Year => {
                let ya = a.year.unwrap_or(0);
                let yb = b.year.unwrap_or(0);
                yb.cmp(&ya) // newest first
                    .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase()))
            }
            AlbumSortKey::MostPlayed => b
                .total_play_count
                .cmp(&a.total_play_count)
                .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase())),
        };
        ordering.into()
    })
}

/// Builds the header row: "Albums N albums" (left) + sort dropdown pill (right).
/// Returns `(root_box, count_label, dropdown)`.
pub(in crate::ui) fn build_header(
    conn: Rc<RefCell<Connection>>,
    on_sort_changed: impl Fn(AlbumSortKey) + 'static,
) -> (gtk4::Box, gtk4::Label, gtk4::DropDown) {
    // "Albums" heading.
    let heading = gtk4::Label::builder()
        .label(&strings::text(strings::LIBRARY_VIEW_ALBUMS))
        .css_classes(["title-2"])
        .halign(gtk4::Align::Start)
        .build();

    // "N albums" count.
    let count_label = gtk4::Label::builder()
        .css_classes(["dim-label"])
        .halign(gtk4::Align::Start)
        .margin_start(8)
        .build();

    let left = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    left.append(&heading);
    left.append(&count_label);
    left.set_hexpand(true);

    // Sort dropdown.
    let labels: Vec<String> = AlbumSortKey::ALL.iter().map(|k| k.label()).collect();
    let string_list = gtk4::StringList::new(&labels.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let dropdown = gtk4::DropDown::builder()
        .model(&string_list)
        .build();
    dropdown.add_css_class("flat");

    // Set initial selection from settings.
    let initial_key = {
        let conn = conn.borrow();
        current_sort_key(&conn)
    };
    let initial_index = AlbumSortKey::ALL
        .iter()
        .position(|k| *k == initial_key)
        .unwrap_or(0);
    dropdown.set_selected(initial_index as u32);

    // On change: persist + notify.
    let conn_for_change = conn.clone();
    dropdown.connect_selected_notify(move |dd| {
        let index = dd.selected() as usize;
        let key = AlbumSortKey::ALL.get(index).copied().unwrap_or(AlbumSortKey::RecentlyAdded);
        {
            let conn = conn_for_change.borrow();
            save_sort_key(&conn, key);
        }
        on_sort_changed(key);
    });

    let root = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .margin_start(24)
        .margin_end(24)
        .margin_top(16)
        .margin_bottom(8)
        .build();
    root.append(&left);
    root.append(&dropdown);

    (root, count_label, dropdown)
}

/// Updates the count label text, e.g. "148 albums".
pub(in crate::ui) fn update_count(label: &gtk4::Label, count: u32) {
    label.set_text(&strings::text(strings::ALBUM_COUNT_FMT).replace("{}", &count.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_key_round_trips_through_setting_value() {
        for key in AlbumSortKey::ALL {
            assert_eq!(AlbumSortKey::from_setting(key.setting_value()), key);
        }
    }

    #[test]
    fn unknown_setting_defaults_to_recently_added() {
        assert_eq!(
            AlbumSortKey::from_setting("nonsense"),
            AlbumSortKey::RecentlyAdded
        );
    }

    #[test]
    fn current_sort_key_defaults_when_unset() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert_eq!(current_sort_key(&conn), AlbumSortKey::RecentlyAdded);
    }

    #[test]
    fn save_and_read_sort_key() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        save_sort_key(&conn, AlbumSortKey::Year);
        assert_eq!(current_sort_key(&conn), AlbumSortKey::Year);
    }
}
