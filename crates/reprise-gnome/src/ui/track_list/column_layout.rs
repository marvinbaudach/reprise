//! Music-table column construction and presentation policy.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::settings::{self, COLUMN_LAYOUT_KEY, COLUMN_WIDTHS_KEY};

use crate::ui::cover_loader::CoverLoader;
use crate::ui::rating::COMPACT_RATING_COLUMN_WIDTH;
use crate::ui::strings;
use crate::ui::table_columns::registry::{ColumnRegistry as GenericColumnRegistry, TableKeys};
use crate::ui::table_columns::width_persistence;
use crate::ui::track_list::Shared;
use crate::ui::track_list::track_list_title_column::append_title_column;
use crate::ui::track_list_columns::{
    CellAlignment, append_column, append_cover_column, append_rating_column,
};
use reprise_core::format::format_duration;
pub use reprise_view::columns::ColumnId;
use reprise_view::columns::{Layout, layout};

pub type ColumnLayout = Layout<ColumnId>;
pub(in crate::ui) type ColumnRegistry = Rc<GenericColumnRegistry<ColumnId>>;

fn cell_alignment(id: ColumnId) -> CellAlignment {
    match id {
        ColumnId::TrackNumber
        | ColumnId::Year
        | ColumnId::Added
        | ColumnId::Duration
        | ColumnId::Rating
        | ColumnId::PlayCount => CellAlignment::Numeric,
        ColumnId::Cover
        | ColumnId::Title
        | ColumnId::Artist
        | ColumnId::Album
        | ColumnId::Genre => CellAlignment::Text,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnWidthPolicy {
    fixed_width: i32,
    expand: bool,
}

fn column_width_policy(id: ColumnId) -> ColumnWidthPolicy {
    let fixed_width = match id {
        ColumnId::Cover => 40,
        // Title uses expand — a low fixed_width keeps the total column
        // minimum under the content-area width when the info panel is
        // open, while expand absorbs remaining space.
        ColumnId::Title => 120,
        ColumnId::TrackNumber => 80,
        ColumnId::Artist => 260,
        ColumnId::Album => 300,
        ColumnId::Genre => 180,
        ColumnId::Year => 90,
        ColumnId::Added => 160,
        ColumnId::Duration => 100,
        ColumnId::Rating => COMPACT_RATING_COLUMN_WIDTH,
        ColumnId::PlayCount => 90,
    };
    ColumnWidthPolicy {
        fixed_width,
        expand: id == ColumnId::Title,
    }
}

/// Whether a column's width can become a persisted user preference. Cover is
/// fixed artwork; every free music column can be resized.
pub(super) fn is_width_persistable(id: ColumnId) -> bool {
    !matches!(id, ColumnId::Cover)
}

#[cfg(test)]
fn is_width_persistable_now(id: ColumnId, column: &gtk4::ColumnViewColumn) -> bool {
    is_width_persistable(id) && !column.expands()
}

fn apply_column_width_policy(column: &gtk4::ColumnViewColumn, id: ColumnId) {
    let policy = column_width_policy(id);
    // ColumnView recycles row widgets while scrolling. A fixed width prevents
    // newly visible cell contents from changing the natural column geometry.
    column.set_fixed_width(policy.fixed_width);
    column.set_expand(policy.expand);
}

pub(in crate::ui) fn column_label(id: ColumnId) -> String {
    let message = match id {
        ColumnId::Cover => strings::COLUMN_COVER,
        ColumnId::Title => strings::COLUMN_TITLE,
        ColumnId::TrackNumber => strings::COLUMN_TRACK_NUMBER,
        ColumnId::Artist => strings::COLUMN_ARTIST,
        ColumnId::Album => strings::COLUMN_ALBUM,
        ColumnId::Genre => strings::COLUMN_GENRE,
        ColumnId::Year => strings::COLUMN_YEAR,
        ColumnId::Added => strings::COLUMN_ADDED,
        ColumnId::Duration => strings::COLUMN_LENGTH,
        ColumnId::Rating => strings::RATING,
        ColumnId::PlayCount => strings::COLUMN_PLAY_COUNT,
    };
    strings::text(message)
}

pub fn serialize_layout(layout: &ColumnLayout) -> String {
    layout::serialize(layout)
}

pub fn parse_layout(value: &str) -> Option<ColumnLayout> {
    layout::parse(value)
}

pub fn load_layout(conn: &Db) -> ColumnLayout {
    let stored = settings::get_setting(conn, COLUMN_LAYOUT_KEY)
        .map_err(|error| tracing::warn!(%error, "could not load stored column layout"))
        .ok()
        .flatten();
    let layout = stored.as_deref().and_then(parse_layout).unwrap_or_default();
    let canonical = serialize_layout(&layout);
    if stored.as_deref() != Some(&canonical) {
        if let Err(error) = settings::set_setting(conn, COLUMN_LAYOUT_KEY, &canonical) {
            tracing::warn!(%error, "could not persist canonical column layout");
        }
    }
    layout
}

pub fn set_column_visible(layout: &ColumnLayout, id: ColumnId, visible: bool) -> ColumnLayout {
    layout::set_visible(layout, id, visible)
}

pub fn move_column(layout: &ColumnLayout, id: ColumnId, target: ColumnId) -> ColumnLayout {
    layout::move_before(layout, id, target)
}

pub fn move_column_after(layout: &ColumnLayout, id: ColumnId, target: ColumnId) -> ColumnLayout {
    layout::move_after(layout, id, target)
}

pub(super) struct BuiltColumns {
    pub(super) registry: ColumnRegistry,
    pub(super) title: gtk4::ColumnViewColumn,
    pub(super) artist: gtk4::ColumnViewColumn,
}

pub(super) fn build_columns(
    view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    cover_loader: &Rc<CoverLoader>,
) -> BuiltColumns {
    let cover = append_cover_column(view, shared, cover_loader);
    // The Title column has its own factory (equaliser + now-playing accent),
    // not the generic text-cell one — see `append_title_column`.
    let title = append_title_column(view, shared);
    let track_number = append_column(
        view,
        shared,
        "track_no",
        &strings::text(strings::COLUMN_TRACK_NUMBER),
        cell_alignment(ColumnId::TrackNumber),
        |t| {
            t.track_no
                .map(|value| value.to_string())
                .unwrap_or_default()
        },
    );
    let artist = append_column(
        view,
        shared,
        "artist",
        &strings::text(strings::COLUMN_ARTIST),
        cell_alignment(ColumnId::Artist),
        |t| t.artist.clone(),
    );
    let album = append_column(
        view,
        shared,
        "album",
        &strings::text(strings::COLUMN_ALBUM),
        cell_alignment(ColumnId::Album),
        |t| t.album.clone(),
    );
    let genre = append_column(
        view,
        shared,
        "genre",
        &strings::text(strings::COLUMN_GENRE),
        cell_alignment(ColumnId::Genre),
        |t| t.genre.clone(),
    );
    let year = append_column(
        view,
        shared,
        "year",
        &strings::text(strings::COLUMN_YEAR),
        cell_alignment(ColumnId::Year),
        |t| t.year.map(|value| value.to_string()).unwrap_or_default(),
    );
    let duration = append_column(
        view,
        shared,
        "duration_ms",
        &strings::text(strings::COLUMN_LENGTH),
        cell_alignment(ColumnId::Duration),
        |t| format_duration(t.duration_ms),
    );
    let added = append_column(
        view,
        shared,
        "added_at",
        &strings::text(strings::COLUMN_ADDED),
        cell_alignment(ColumnId::Added),
        |track| reprise_core::format::format_unix_timestamp(track.added_at),
    );
    let rating = append_rating_column(view, shared);
    let play_count = append_column(
        view,
        shared,
        "play_count",
        &strings::text(strings::COLUMN_PLAY_COUNT),
        cell_alignment(ColumnId::PlayCount),
        |track| track.play_count.to_string(),
    );

    let columns = vec![
        (ColumnId::Cover, cover),
        (ColumnId::Title, title.clone()),
        (ColumnId::TrackNumber, track_number),
        (ColumnId::Artist, artist.clone()),
        (ColumnId::Album, album),
        (ColumnId::Genre, genre),
        (ColumnId::Year, year),
        (ColumnId::Added, added),
        (ColumnId::Duration, duration),
        (ColumnId::Rating, rating),
        (ColumnId::PlayCount, play_count),
    ];
    for (id, column) in &columns {
        apply_column_width_policy(column, *id);
    }
    let registry = GenericColumnRegistry::new(
        view,
        shared.conn.clone(),
        TableKeys {
            layout: COLUMN_LAYOUT_KEY,
            widths: COLUMN_WIDTHS_KEY,
        },
        columns,
    );
    width_persistence::wire(
        &registry,
        column_label,
        |id| {
            debug_assert_eq!(is_width_persistable(id), id != ColumnId::Cover);
            column_width_policy(id).fixed_width
        },
        ColumnId::Title,
    );
    super::column_header_dnd::wire_header_drag(view);
    let layout = registry.layout();
    registry.apply(&layout);
    BuiltColumns {
        registry,
        title,
        artist,
    }
}

#[cfg(test)]
#[path = "column_layout_tests.rs"]
mod tests;
