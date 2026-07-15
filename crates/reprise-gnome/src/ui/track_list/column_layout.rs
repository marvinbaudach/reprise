//! Typed track-column layout, persistence format, and GTK column registry.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use thiserror::Error;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::rating::COMPACT_RATING_COLUMN_WIDTH;
use crate::ui::strings;
use crate::ui::track_list::Shared;
use crate::ui::track_list_columns::{
    append_column, append_cover_column, append_rating_column, CellAlignment,
};
use reprise_core::format::format_duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId {
    Cover,
    Title,
    TrackNumber,
    Artist,
    Album,
    Genre,
    Year,
    Duration,
    Rating,
    PlayCount,
}

const DEFAULT_ORDER: [ColumnId; 10] = [
    ColumnId::Cover,
    ColumnId::Title,
    ColumnId::Artist,
    ColumnId::Album,
    ColumnId::Year,
    ColumnId::Duration,
    ColumnId::Rating,
    ColumnId::PlayCount,
    ColumnId::TrackNumber,
    ColumnId::Genre,
];

fn cell_alignment(id: ColumnId) -> CellAlignment {
    match id {
        ColumnId::TrackNumber
        | ColumnId::Year
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
        ColumnId::Duration => 100,
        ColumnId::Rating => COMPACT_RATING_COLUMN_WIDTH,
        ColumnId::PlayCount => 90,
    };
    ColumnWidthPolicy {
        fixed_width,
        expand: id == ColumnId::Title,
    }
}

fn apply_column_width_policy(column: &gtk4::ColumnViewColumn, id: ColumnId) {
    let policy = column_width_policy(id);
    // ColumnView recycles row widgets while scrolling. A fixed width prevents
    // newly visible cell contents from changing the natural column geometry.
    column.set_fixed_width(policy.fixed_width);
    column.set_expand(policy.expand);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnLayout {
    pub order: Vec<ColumnId>,
    pub visible: HashSet<ColumnId>,
}

impl Default for ColumnLayout {
    fn default() -> Self {
        Self {
            order: DEFAULT_ORDER.to_vec(),
            visible: HashSet::from([
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Artist,
                ColumnId::Album,
                ColumnId::Year,
                ColumnId::Duration,
                ColumnId::Rating,
            ]),
        }
    }
}

impl ColumnId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Title => "title",
            Self::TrackNumber => "track-number",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Genre => "genre",
            Self::Year => "year",
            Self::Duration => "duration",
            Self::Rating => "rating",
            Self::PlayCount => "play-count",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "cover" => Some(Self::Cover),
            "title" => Some(Self::Title),
            "track-number" => Some(Self::TrackNumber),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "genre" => Some(Self::Genre),
            "year" => Some(Self::Year),
            "duration" => Some(Self::Duration),
            "rating" => Some(Self::Rating),
            "play-count" => Some(Self::PlayCount),
            _ => None,
        }
    }

    pub fn from_sort_field(field: &str) -> Option<Self> {
        match field {
            "title" => Some(Self::Title),
            "track_no" => Some(Self::TrackNumber),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "genre" => Some(Self::Genre),
            "year" => Some(Self::Year),
            "duration_ms" => Some(Self::Duration),
            "rating" => Some(Self::Rating),
            "play_count" => Some(Self::PlayCount),
            _ => None,
        }
    }
}

pub(super) fn column_label(id: ColumnId) -> String {
    let message = match id {
        ColumnId::Cover => strings::COLUMN_COVER,
        ColumnId::Title => strings::COLUMN_TITLE,
        ColumnId::TrackNumber => strings::COLUMN_TRACK_NUMBER,
        ColumnId::Artist => strings::COLUMN_ARTIST,
        ColumnId::Album => strings::COLUMN_ALBUM,
        ColumnId::Genre => strings::COLUMN_GENRE,
        ColumnId::Year => strings::COLUMN_YEAR,
        ColumnId::Duration => strings::COLUMN_LENGTH,
        ColumnId::Rating => strings::RATING,
        ColumnId::PlayCount => strings::COLUMN_PLAY_COUNT,
    };
    strings::text(message)
}

pub fn serialize_layout(layout: &ColumnLayout) -> String {
    let layout = normalize(layout.order.clone(), layout.visible.clone());
    let order = layout
        .order
        .iter()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let visible = layout
        .order
        .iter()
        .filter(|id| layout.visible.contains(id))
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!("{order};{visible}")
}

pub fn parse_layout(value: &str) -> Option<ColumnLayout> {
    let (order, visible) = value.split_once(';')?;
    let order = parse_ids(order)?;
    let visible = parse_ids(visible)?.into_iter().collect();
    Some(normalize(order, visible))
}

pub fn load_layout(conn: &rusqlite::Connection) -> ColumnLayout {
    let stored = reprise_core::library::settings::get_setting(
        conn,
        reprise_core::library::settings::COLUMN_LAYOUT_KEY,
    )
    .map_err(|error| tracing::warn!(%error, "could not load stored column layout"))
    .ok()
    .flatten();
    let layout = stored.as_deref().and_then(parse_layout).unwrap_or_default();
    let canonical = serialize_layout(&layout);
    if stored.as_deref() != Some(&canonical) {
        if let Err(error) = reprise_core::library::settings::set_setting(
            conn,
            reprise_core::library::settings::COLUMN_LAYOUT_KEY,
            &canonical,
        ) {
            tracing::warn!(%error, "could not persist canonical column layout");
        }
    }
    layout
}

pub fn import_rhythmbox_tokens(tokens: &[String]) -> ColumnLayout {
    let mut order = Vec::new();
    let mut visible = HashSet::new();
    for token in tokens {
        let id = match token.as_str() {
            "track-number" => ColumnId::TrackNumber,
            "artist" => ColumnId::Artist,
            "album" => ColumnId::Album,
            "genre" => ColumnId::Genre,
            "duration" => ColumnId::Duration,
            "date" => ColumnId::Year,
            "rating" => ColumnId::Rating,
            "play-count" => ColumnId::PlayCount,
            _ => continue,
        };
        if visible.insert(id) {
            order.push(id);
        }
    }
    normalize(order, visible)
}

pub fn set_column_visible(layout: &ColumnLayout, id: ColumnId, visible: bool) -> ColumnLayout {
    if matches!(id, ColumnId::Cover | ColumnId::Title) {
        return layout.clone();
    }
    let mut next = layout.clone();
    if visible {
        next.visible.insert(id);
    } else {
        next.visible.remove(&id);
    }
    normalize(next.order, next.visible)
}

pub fn move_column(layout: &ColumnLayout, id: ColumnId, target: ColumnId) -> ColumnLayout {
    move_column_relative(layout, id, target, false)
}

pub fn move_column_after(layout: &ColumnLayout, id: ColumnId, target: ColumnId) -> ColumnLayout {
    move_column_relative(layout, id, target, true)
}

fn move_column_relative(
    layout: &ColumnLayout,
    id: ColumnId,
    target: ColumnId,
    after: bool,
) -> ColumnLayout {
    if id == target
        || matches!(id, ColumnId::Cover | ColumnId::Title)
        || matches!(target, ColumnId::Cover | ColumnId::Title)
    {
        return layout.clone();
    }
    let mut order = layout.order.clone();
    let Some(source_index) = order.iter().position(|candidate| *candidate == id) else {
        return layout.clone();
    };
    order.remove(source_index);
    let Some(target_index) = order.iter().position(|candidate| *candidate == target) else {
        return layout.clone();
    };
    order.insert(target_index + usize::from(after), id);
    normalize(order, layout.visible.clone())
}

const RHYTHMBOX_SCHEMA: &str = "org.gnome.rhythmbox.sources";
const RHYTHMBOX_VISIBLE_COLUMNS_KEY: &str = "visible-columns";

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("the system GSettings schema source is unavailable")]
    SchemaSourceUnavailable,
    #[error("Rhythmbox GSettings schema is not installed")]
    SchemaMissing,
    #[error("Rhythmbox visible-columns setting is unavailable")]
    KeyMissing,
}

pub fn read_rhythmbox_visible_columns() -> Result<Vec<String>, ImportError> {
    let source =
        gio::SettingsSchemaSource::default().ok_or(ImportError::SchemaSourceUnavailable)?;
    let schema = source
        .lookup(RHYTHMBOX_SCHEMA, true)
        .ok_or(ImportError::SchemaMissing)?;
    if !schema.has_key(RHYTHMBOX_VISIBLE_COLUMNS_KEY) {
        return Err(ImportError::KeyMissing);
    }
    let settings = gio::Settings::new_full(&schema, gio::SettingsBackend::NONE, None);
    Ok(settings
        .strv(RHYTHMBOX_VISIBLE_COLUMNS_KEY)
        .iter()
        .map(ToString::to_string)
        .collect())
}

pub fn should_offer_rhythmbox_import(available: bool) -> bool {
    available
}

fn parse_ids(value: &str) -> Option<Vec<ColumnId>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let mut seen = HashSet::new();
    value
        .split(',')
        .map(|token| {
            let id = ColumnId::parse(token)?;
            seen.insert(id).then_some(id)
        })
        .collect()
}

fn normalize(mut order: Vec<ColumnId>, mut visible: HashSet<ColumnId>) -> ColumnLayout {
    order.retain(|id| !matches!(id, ColumnId::Cover | ColumnId::Title));
    let mut normalized = vec![ColumnId::Cover, ColumnId::Title];
    for id in order.into_iter().chain(DEFAULT_ORDER) {
        if !normalized.contains(&id) {
            normalized.push(id);
        }
    }
    visible.insert(ColumnId::Cover);
    visible.insert(ColumnId::Title);
    ColumnLayout {
        order: normalized,
        visible,
    }
}

pub struct BuiltColumns {
    pub registry: ColumnRegistry,
    pub title: gtk4::ColumnViewColumn,
    pub artist: gtk4::ColumnViewColumn,
}

pub struct ColumnRegistry {
    view: gtk4::ColumnView,
    columns: HashMap<ColumnId, gtk4::ColumnViewColumn>,
}

impl ColumnRegistry {
    pub fn apply(&self, layout: &ColumnLayout) {
        let layout = normalize(layout.order.clone(), layout.visible.clone());
        for column in self.columns.values() {
            self.view.remove_column(column);
        }
        for id in &layout.order {
            if let Some(column) = self.columns.get(id) {
                column.set_visible(layout.visible.contains(id));
                self.view.append_column(column);
            }
        }
    }

    pub fn column(&self, id: ColumnId) -> Option<&gtk4::ColumnViewColumn> {
        self.columns.get(&id)
    }

    pub fn is_visible(&self, id: ColumnId) -> bool {
        self.columns
            .get(&id)
            .is_some_and(gtk4::ColumnViewColumn::is_visible)
    }

    pub fn set_header_menu(&self, menu: &gio::Menu) {
        for column in self.columns.values() {
            column.set_header_menu(Some(menu));
        }
    }
}

pub(super) fn build_columns(
    view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    cover_loader: &Rc<CoverLoader>,
) -> BuiltColumns {
    let cover = append_cover_column(view, shared, cover_loader);
    let title = append_column(
        view,
        shared,
        "title",
        &strings::text(strings::COLUMN_TITLE),
        cell_alignment(ColumnId::Title),
        |t| t.title.clone(),
    );
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
    let rating = append_rating_column(view, shared);
    let play_count = append_column(
        view,
        shared,
        "play_count",
        &strings::text(strings::COLUMN_PLAY_COUNT),
        cell_alignment(ColumnId::PlayCount),
        |track| track.play_count.to_string(),
    );

    let columns = HashMap::from([
        (ColumnId::Cover, cover),
        (ColumnId::Title, title.clone()),
        (ColumnId::TrackNumber, track_number),
        (ColumnId::Artist, artist.clone()),
        (ColumnId::Album, album),
        (ColumnId::Genre, genre),
        (ColumnId::Year, year),
        (ColumnId::Duration, duration),
        (ColumnId::Rating, rating),
        (ColumnId::PlayCount, play_count),
    ]);
    for (id, column) in &columns {
        apply_column_width_policy(column, *id);
    }
    let registry = ColumnRegistry {
        view: view.clone(),
        columns,
    };
    let layout = load_layout(&shared.conn.borrow());
    registry.apply(&layout);
    BuiltColumns {
        registry,
        title,
        artist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_metadata_columns_are_classified_for_centering() {
        for id in [
            ColumnId::TrackNumber,
            ColumnId::Year,
            ColumnId::Duration,
            ColumnId::Rating,
            ColumnId::PlayCount,
        ] {
            assert_eq!(cell_alignment(id), CellAlignment::Numeric);
        }
        for id in [
            ColumnId::Title,
            ColumnId::Artist,
            ColumnId::Album,
            ColumnId::Genre,
        ] {
            assert_eq!(cell_alignment(id), CellAlignment::Text);
        }
    }

    #[test]
    fn rating_column_uses_the_compact_width() {
        assert_eq!(
            column_width_policy(ColumnId::Rating).fixed_width,
            crate::ui::rating::COMPACT_RATING_COLUMN_WIDTH
        );
    }

    #[test]
    fn every_track_column_has_stable_width_and_only_title_expands() {
        for id in DEFAULT_ORDER {
            let policy = column_width_policy(id);
            if id == ColumnId::Title {
                // Title uses expand with a low fixed_width so it absorbs
                // remaining space and shrinks when the info panel is open.
                assert!(policy.fixed_width > 0 && policy.fixed_width < 200);
                assert!(policy.expand);
            } else {
                assert!(policy.fixed_width > 0, "missing fixed width for {id:?}");
                assert!(!policy.expand);
            }
        }
    }

    #[test]
    fn play_count_is_available_but_hidden_by_default() {
        let layout = ColumnLayout::default();
        let rating = layout
            .order
            .iter()
            .position(|id| *id == ColumnId::Rating)
            .unwrap();
        assert_eq!(layout.order[rating + 1], ColumnId::PlayCount);
        assert!(!layout.visible.contains(&ColumnId::PlayCount));
        assert_eq!(ColumnId::PlayCount.as_str(), "play-count");
        assert_eq!(
            ColumnId::from_sort_field("play_count"),
            Some(ColumnId::PlayCount)
        );
    }

    #[test]
    fn legacy_layout_gains_a_hidden_play_count_column() {
        let layout = parse_layout("cover,title,artist;cover,title,artist").unwrap();
        assert!(layout.order.contains(&ColumnId::PlayCount));
        assert!(!layout.visible.contains(&ColumnId::PlayCount));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn width_policy_is_applied_to_gtk_columns() {
        gtk4::init().unwrap();
        for id in DEFAULT_ORDER {
            let column = gtk4::ColumnViewColumn::builder().build();
            apply_column_width_policy(&column, id);
            let policy = column_width_policy(id);
            assert_eq!(column.fixed_width(), policy.fixed_width);
            assert_eq!(column.expands(), policy.expand);
        }
    }

    #[test]
    fn layout_round_trips_canonically() {
        let layout = ColumnLayout::default();
        assert_eq!(parse_layout(&serialize_layout(&layout)), Some(layout));
    }

    #[test]
    fn duplicate_or_unknown_ids_are_rejected() {
        assert!(parse_layout("cover,title,title;cover,title").is_none());
        assert!(parse_layout("cover,title,banana;cover,title").is_none());
        assert!(parse_layout("cover,title;cover,banana").is_none());
    }

    #[test]
    fn cover_and_title_are_forced_visible_and_first() {
        let layout = parse_layout("artist,album;artist,album").unwrap();
        assert_eq!(layout.order[..2], [ColumnId::Cover, ColumnId::Title]);
        assert!(layout.visible.contains(&ColumnId::Cover));
        assert!(layout.visible.contains(&ColumnId::Title));
    }

    #[test]
    fn corrupted_layout_can_fall_back_to_default() {
        let loaded = parse_layout("not a layout").unwrap_or_default();
        assert_eq!(loaded, ColumnLayout::default());
    }

    #[test]
    fn rhythmbox_mapping_preserves_supported_order_and_ignores_unknown() {
        let tokens = [
            "rating",
            "play-count",
            "duration",
            "album",
            "artist",
            "date",
            "post-time",
        ]
        .map(str::to_string);
        let layout = import_rhythmbox_tokens(&tokens);
        assert_eq!(
            layout.order[..8],
            [
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Rating,
                ColumnId::PlayCount,
                ColumnId::Duration,
                ColumnId::Album,
                ColumnId::Artist,
                ColumnId::Year,
            ]
        );
        assert_eq!(layout.visible.len(), 8);
    }

    #[test]
    fn rhythmbox_mapping_stably_deduplicates_tokens() {
        let tokens = ["artist", "album", "artist", "genre"].map(str::to_string);
        let layout = import_rhythmbox_tokens(&tokens);
        assert_eq!(
            layout.order[..5],
            [
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Artist,
                ColumnId::Album,
                ColumnId::Genre,
            ]
        );
    }

    #[test]
    fn rhythmbox_empty_list_still_keeps_cover_and_title() {
        let layout = import_rhythmbox_tokens(&[]);
        assert_eq!(layout.order[..2], [ColumnId::Cover, ColumnId::Title]);
        assert_eq!(
            layout.visible,
            HashSet::from([ColumnId::Cover, ColumnId::Title])
        );
    }

    #[test]
    fn optional_visibility_changes_without_changing_order() {
        let layout = ColumnLayout::default();
        let hidden = set_column_visible(&layout, ColumnId::Artist, false);
        assert_eq!(hidden.order, layout.order);
        assert!(!hidden.visible.contains(&ColumnId::Artist));
        let shown = set_column_visible(&hidden, ColumnId::TrackNumber, true);
        assert_eq!(shown.order, layout.order);
        assert!(shown.visible.contains(&ColumnId::TrackNumber));
    }

    #[test]
    fn fixed_columns_cannot_be_hidden() {
        let layout = ColumnLayout::default();
        assert_eq!(set_column_visible(&layout, ColumnId::Cover, false), layout);
        assert_eq!(set_column_visible(&layout, ColumnId::Title, false), layout);
    }

    #[test]
    fn movable_column_is_inserted_before_the_target() {
        let layout = ColumnLayout::default();
        let moved = move_column(&layout, ColumnId::Rating, ColumnId::Artist);
        assert_eq!(
            moved.order[..5],
            [
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Rating,
                ColumnId::Artist,
                ColumnId::Album,
            ]
        );
        assert_eq!(moved.visible, layout.visible);
    }

    #[test]
    fn movable_column_can_be_inserted_after_the_target() {
        let layout = ColumnLayout::default();
        let moved = move_column_after(&layout, ColumnId::Artist, ColumnId::Rating);
        let rating = moved
            .order
            .iter()
            .position(|id| *id == ColumnId::Rating)
            .unwrap();
        assert_eq!(moved.order[rating + 1], ColumnId::Artist);
        assert_eq!(moved.visible, layout.visible);
    }

    #[test]
    fn fixed_target_source_and_self_moves_are_noops() {
        let layout = ColumnLayout::default();
        assert_eq!(
            move_column(&layout, ColumnId::Cover, ColumnId::Artist),
            layout
        );
        assert_eq!(
            move_column(&layout, ColumnId::Artist, ColumnId::Title),
            layout
        );
        assert_eq!(
            move_column(&layout, ColumnId::Artist, ColumnId::Artist),
            layout
        );
    }

    #[test]
    fn rhythmbox_import_is_offered_exactly_when_available() {
        assert!(should_offer_rhythmbox_import(true));
        assert!(!should_offer_rhythmbox_import(false));
    }
}
