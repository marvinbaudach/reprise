//! Releases full-view composition boundary.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::db::Db;

pub(super) mod css;
mod releases_cell_surface;
mod releases_column_layout;
mod releases_columns;
mod releases_context_menu;
mod releases_cover_column;
mod releases_empty_state;
mod releases_failure_ui;
mod releases_filter_bar;
mod releases_menu;
mod releases_model;
pub(in crate::ui) mod releases_presentation;
mod releases_selection;
mod releases_view;

pub(in crate::ui) use releases_view::ReleasesView;

#[cfg(test)]
pub(super) fn test_entry(mbid: &str) -> reprise_core::artist_news_history::HistoryEntry {
    reprise_core::artist_news_history::HistoryEntry {
        release_group_mbid: mbid.to_owned(),
        artist_name: "Artist".to_owned(),
        title: "Album".to_owned(),
        release_type: "Album".to_owned(),
        first_release_date: String::new(),
        first_seen: None,
        seen_at: None,
        hidden: false,
        hidden_at: None,
        presence: reprise_core::artist_news::LibraryPresence::Absent,
        announce_url: None,
        track_count: None,
        local_track_count: 0,
    }
}

#[allow(dead_code)]
pub(in crate::ui) fn install(conn: Rc<Db>, database_path: PathBuf) -> ReleasesView {
    ReleasesView::new(conn, database_path)
}
