//! Typed track-column layout, persistence format, and GTK column registry.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use thiserror::Error;

use reprise_core::library::settings::{self, COLUMN_WIDTHS_KEY};

use crate::ui::cover_loader::CoverLoader;
use crate::ui::rating::COMPACT_RATING_COLUMN_WIDTH;
use crate::ui::strings;
use crate::ui::track_list::Shared;
use crate::ui::track_list_columns::{
    append_column, append_cover_column, append_rating_column, append_title_column, CellAlignment,
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

/// Whether a column's user-dragged width is worth persisting. Cover is not
/// resizable and Title expands to fill remaining space, so their fixed width is
/// not a meaningful user preference — every other column is stored.
pub(super) fn is_width_persistable(id: ColumnId) -> bool {
    !matches!(id, ColumnId::Cover) && !column_width_policy(id).expand
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
    // Rhythmbox has no Cover/Title columns in this list; a fresh import always
    // leads with our artwork + title columns, then the mapped Rhythmbox tokens.
    let mut order = vec![ColumnId::Cover, ColumnId::Title];
    let mut visible = HashSet::from([ColumnId::Cover, ColumnId::Title]);
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
    if id == target {
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

fn normalize(order: Vec<ColumnId>, visible: HashSet<ColumnId>) -> ColumnLayout {
    // The stored order is honored verbatim; any column not mentioned is
    // appended in the built-in default order so all columns stay reachable.
    // Cover and Title are ordinary columns — no forced position or visibility.
    let mut normalized = Vec::with_capacity(DEFAULT_ORDER.len());
    for id in order.into_iter().chain(DEFAULT_ORDER) {
        if !normalized.contains(&id) {
            normalized.push(id);
        }
    }
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
    /// Suppresses `wire_order_persistence`'s columns-model listener while
    /// `apply` rebuilds the column list programmatically — only genuine user
    /// header drags may persist an order from the model side.
    syncing_order: Rc<Cell<bool>>,
}

impl ColumnRegistry {
    pub fn apply(&self, layout: &ColumnLayout) {
        let layout = normalize(layout.order.clone(), layout.visible.clone());
        // Visibility is a per-column property — updating it never touches the
        // view's column list, so scroll position and selection are preserved.
        for (id, column) in &self.columns {
            column.set_visible(layout.visible.contains(id));
        }
        // Only rebuild the column list when the order genuinely changed;
        // remove/re-append resets the horizontal scroll offset otherwise.
        let desired: Vec<ColumnId> = layout
            .order
            .iter()
            .copied()
            .filter(|id| self.columns.contains_key(id))
            .collect();
        if self.current_order() == desired {
            return;
        }
        self.syncing_order.set(true);
        for column in self.columns.values() {
            self.view.remove_column(column);
        }
        for id in &desired {
            if let Some(column) = self.columns.get(id) {
                self.view.append_column(column);
            }
        }
        self.syncing_order.set(false);
    }

    /// The column ids currently held by the view, in their present order.
    /// Used to skip a destructive rebuild when only visibility changed.
    fn current_order(&self) -> Vec<ColumnId> {
        let model = self.view.columns();
        (0..model.n_items())
            .filter_map(|index| {
                let column = model
                    .item(index)?
                    .downcast::<gtk4::ColumnViewColumn>()
                    .ok()?;
                self.columns
                    .iter()
                    .find(|(_, candidate)| **candidate == column)
                    .map(|(id, _)| *id)
            })
            .collect()
    }

    pub fn column(&self, id: ColumnId) -> Option<&gtk4::ColumnViewColumn> {
        self.columns.get(&id)
    }

    pub fn is_visible(&self, id: ColumnId) -> bool {
        self.columns
            .get(&id)
            .is_some_and(gtk4::ColumnViewColumn::is_visible)
    }

    /// Restores every column to its built-in policy width. The wired
    /// `fixed-width` listeners then persist these defaults on the next tick.
    pub fn reset_widths(&self) {
        for (id, column) in &self.columns {
            apply_column_width_policy(column, *id);
        }
    }
}

/// Coalescing delay for width writes: header drags fire many `fixed-width`
/// notifications, so we persist only after the drag settles.
const WIDTH_SAVE_DEBOUNCE_MS: u64 = 500;

/// Overrides policy default widths with any the user previously stored.
fn restore_stored_widths(
    columns: &HashMap<ColumnId, gtk4::ColumnViewColumn>,
    conn: &rusqlite::Connection,
) {
    let stored = settings::get_setting(conn, COLUMN_WIDTHS_KEY)
        .map_err(|error| tracing::warn!(%error, "could not load stored column widths"))
        .ok()
        .flatten();
    let Some(stored) = stored else {
        return;
    };
    for (id, width) in crate::ui::column_widths::parse_widths(&stored) {
        if is_width_persistable(id) {
            if let Some(column) = columns.get(&id) {
                column.set_fixed_width(width);
            }
        }
    }
}

/// Persists the current widths of all persistable columns (debounced).
fn save_widths_now(shared: &Shared, columns: &[(ColumnId, gtk4::ColumnViewColumn)]) {
    let widths: Vec<(ColumnId, i32)> = columns
        .iter()
        .map(|(id, column)| (*id, column.fixed_width()))
        .collect();
    let serialized = crate::ui::column_widths::serialize_widths(&widths);
    if let Err(error) = settings::set_setting(&shared.conn.borrow(), COLUMN_WIDTHS_KEY, &serialized)
    {
        tracing::warn!(%error, "could not persist column widths");
    }
}

/// Wires a debounced `fixed-width` listener on every persistable column so a
/// header-drag resize is stored ~500 ms after it settles. Must run after the
/// initial policy/restore widths are applied, so setup does not self-trigger.
fn wire_width_persistence(
    shared: &Rc<Shared>,
    columns: &HashMap<ColumnId, gtk4::ColumnViewColumn>,
) {
    let snapshot: Rc<Vec<(ColumnId, gtk4::ColumnViewColumn)>> = Rc::new(
        columns
            .iter()
            .filter(|(id, _)| is_width_persistable(**id))
            .map(|(id, column)| (*id, column.clone()))
            .collect(),
    );
    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    for (_, column) in snapshot.iter() {
        let shared_weak = Rc::downgrade(shared);
        let snapshot = snapshot.clone();
        let debounce = debounce.clone();
        column.connect_fixed_width_notify(move |_| {
            if let Some(pending) = debounce.borrow_mut().take() {
                pending.remove();
            }
            let shared_weak = shared_weak.clone();
            let snapshot = snapshot.clone();
            let debounce_inner = debounce.clone();
            let handle = glib::timeout_add_local_once(
                Duration::from_millis(WIDTH_SAVE_DEBOUNCE_MS),
                move || {
                    *debounce_inner.borrow_mut() = None;
                    if let Some(shared) = shared_weak.upgrade() {
                        save_widths_now(&shared, &snapshot);
                    }
                },
            );
            *debounce.borrow_mut() = Some(handle);
        });
    }
}

/// Persists a header drag-reorder. GTK's built-in column drag (the view is
/// `reorderable`) mutates the view's columns model directly — without ever
/// going through `TrackList::apply_column_layout` — so this listens on that
/// model and stores the resulting order under the same setting the popover/
/// preferences editor writes. `syncing_order` mutes the events fired by
/// `ColumnRegistry::apply`'s own remove/re-append rebuild; a mid-mutation
/// snapshot (fewer columns than registered) is skipped, so only the final
/// post-drop order is ever persisted.
fn wire_order_persistence(
    shared: &Rc<Shared>,
    view: &gtk4::ColumnView,
    columns: &HashMap<ColumnId, gtk4::ColumnViewColumn>,
    syncing_order: &Rc<Cell<bool>>,
) {
    let shared_weak = Rc::downgrade(shared);
    let columns = columns.clone();
    let syncing_order = syncing_order.clone();
    view.columns().connect_items_changed(move |model, _, _, _| {
        if syncing_order.get() {
            return;
        }
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        let order: Vec<ColumnId> = (0..model.n_items())
            .filter_map(|index| {
                let column = model
                    .item(index)?
                    .downcast::<gtk4::ColumnViewColumn>()
                    .ok()?;
                columns
                    .iter()
                    .find(|(_, candidate)| **candidate == column)
                    .map(|(id, _)| *id)
            })
            .collect();
        if order.len() != columns.len() {
            return; // transient mid-mutation state
        }
        let visible: HashSet<ColumnId> = order
            .iter()
            .copied()
            .filter(|id| {
                columns
                    .get(id)
                    .is_some_and(gtk4::ColumnViewColumn::is_visible)
            })
            .collect();
        let serialized = serialize_layout(&normalize(order, visible));
        let saved = settings::set_setting(
            &shared.conn.borrow(),
            reprise_core::library::settings::COLUMN_LAYOUT_KEY,
            &serialized,
        );
        match saved {
            Ok(()) => {
                tracing::debug!(layout = %serialized, "column order persisted after header drag");
            }
            Err(error) => tracing::warn!(%error, "could not persist dragged column order"),
        }
    });
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
    restore_stored_widths(&columns, &shared.conn.borrow());
    wire_width_persistence(shared, &columns);
    // GTK's native header drag-reorder (on by default; made explicit here) —
    // the listener below keeps the persisted layout in sync with it.
    view.set_reorderable(true);
    let syncing_order = Rc::new(Cell::new(false));
    wire_order_persistence(shared, view, &columns, &syncing_order);
    let registry = ColumnRegistry {
        view: view.clone(),
        columns,
        syncing_order,
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
    fn only_resizable_fixed_width_columns_persist_their_width() {
        // Cover is not resizable; Title expands — neither has a meaningful,
        // user-set fixed width to store.
        assert!(!is_width_persistable(ColumnId::Cover));
        assert!(!is_width_persistable(ColumnId::Title));
        for id in [
            ColumnId::Artist,
            ColumnId::Album,
            ColumnId::Genre,
            ColumnId::Year,
            ColumnId::Duration,
            ColumnId::Rating,
            ColumnId::PlayCount,
            ColumnId::TrackNumber,
        ] {
            assert!(is_width_persistable(id), "{id:?} should persist its width");
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

    fn test_registry(ids: &[ColumnId]) -> ColumnRegistry {
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let mut columns = HashMap::new();
        for id in ids.iter().copied() {
            let column = gtk4::ColumnViewColumn::builder().title(id.as_str()).build();
            view.append_column(&column);
            columns.insert(id, column);
        }
        ColumnRegistry {
            view,
            columns,
            syncing_order: Rc::new(Cell::new(false)),
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn visibility_only_apply_does_not_rebuild_the_column_list() {
        use std::cell::Cell;
        if gtk4::init().is_err() {
            return;
        }
        let ids = [
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Artist,
            ColumnId::Album,
        ];
        let registry = test_registry(&ids);
        // Align the view order with the layout order first (this may rebuild).
        let mut visible: HashSet<ColumnId> = ids.iter().copied().collect();
        registry.apply(&ColumnLayout {
            order: ids.to_vec(),
            visible: visible.clone(),
        });

        let rebuilds = Rc::new(Cell::new(0u32));
        let counter = rebuilds.clone();
        registry
            .view
            .columns()
            .connect_items_changed(move |_, _, _, _| counter.set(counter.get() + 1));

        // Hide Artist only — order is unchanged.
        visible.remove(&ColumnId::Artist);
        registry.apply(&ColumnLayout {
            order: ids.to_vec(),
            visible,
        });

        assert_eq!(
            rebuilds.get(),
            0,
            "a visibility-only change must not remove/re-append columns"
        );
        assert!(!registry.column(ColumnId::Artist).unwrap().is_visible());
        assert!(registry.column(ColumnId::Album).unwrap().is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn reordering_apply_rebuilds_the_column_list_once() {
        use std::cell::Cell;
        if gtk4::init().is_err() {
            return;
        }
        let ids = [
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Artist,
            ColumnId::Album,
        ];
        let registry = test_registry(&ids);
        let visible: HashSet<ColumnId> = ids.iter().copied().collect();
        registry.apply(&ColumnLayout {
            order: ids.to_vec(),
            visible: visible.clone(),
        });

        let rebuilds = Rc::new(Cell::new(0u32));
        let counter = rebuilds.clone();
        registry
            .view
            .columns()
            .connect_items_changed(move |_, _, _, _| counter.set(counter.get() + 1));

        // Move Album ahead of Artist — order genuinely changes.
        registry.apply(&ColumnLayout {
            order: vec![
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Album,
                ColumnId::Artist,
            ],
            visible,
        });

        assert!(
            rebuilds.get() > 0,
            "a real reorder must update the column list"
        );
        assert_eq!(
            registry.current_order(),
            vec![
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Album,
                ColumnId::Artist
            ]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn restore_stored_widths_applies_persistable_columns_only() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        settings::set_setting(&conn, COLUMN_WIDTHS_KEY, "artist:333,cover:999").unwrap();
        let registry = test_registry(&[ColumnId::Cover, ColumnId::Artist]);

        restore_stored_widths(&registry.columns, &conn);

        assert_eq!(
            registry.column(ColumnId::Artist).unwrap().fixed_width(),
            333
        );
        // Cover is not persistable, so its stored value is ignored.
        assert_ne!(registry.column(ColumnId::Cover).unwrap().fixed_width(), 999);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn reset_widths_restores_the_policy_default() {
        if gtk4::init().is_err() {
            return;
        }
        let registry = test_registry(&[ColumnId::Artist]);
        registry
            .column(ColumnId::Artist)
            .unwrap()
            .set_fixed_width(500);

        registry.reset_widths();

        assert_eq!(
            registry.column(ColumnId::Artist).unwrap().fixed_width(),
            column_width_policy(ColumnId::Artist).fixed_width
        );
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
    fn parse_layout_respects_order_and_visibility_without_pinning() {
        // Cover and Title are no longer forced to the front or forced visible:
        // a stored layout is honored verbatim (missing columns still append).
        let layout = parse_layout("artist,album;artist,album").unwrap();
        assert_eq!(layout.order[..2], [ColumnId::Artist, ColumnId::Album]);
        assert!(!layout.visible.contains(&ColumnId::Cover));
        assert!(!layout.visible.contains(&ColumnId::Title));
        // Every known column is still present in the normalized order.
        assert!(layout.order.contains(&ColumnId::Cover));
        assert!(layout.order.contains(&ColumnId::Title));
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
    fn cover_and_title_can_be_hidden_like_any_column() {
        let layout = ColumnLayout::default();
        let cover_hidden = set_column_visible(&layout, ColumnId::Cover, false);
        assert!(!cover_hidden.visible.contains(&ColumnId::Cover));
        assert_eq!(cover_hidden.order, layout.order);
        let title_hidden = set_column_visible(&layout, ColumnId::Title, false);
        assert!(!title_hidden.visible.contains(&ColumnId::Title));
        assert_eq!(title_hidden.order, layout.order);
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
    fn cover_and_title_move_freely_while_self_moves_are_noops() {
        let layout = ColumnLayout::default();
        // Self-move stays a no-op.
        assert_eq!(
            move_column(&layout, ColumnId::Artist, ColumnId::Artist),
            layout
        );
        // Cover may be moved after Title (previously forbidden).
        let moved = move_column_after(&layout, ColumnId::Cover, ColumnId::Title);
        let title_index = moved
            .order
            .iter()
            .position(|id| *id == ColumnId::Title)
            .unwrap();
        assert_eq!(moved.order[title_index + 1], ColumnId::Cover);
        // Artist may be moved before Title (Title is no longer an anchor).
        let moved = move_column(&layout, ColumnId::Artist, ColumnId::Title);
        let title_index = moved
            .order
            .iter()
            .position(|id| *id == ColumnId::Title)
            .unwrap();
        assert_eq!(moved.order[title_index - 1], ColumnId::Artist);
    }

    #[test]
    fn rhythmbox_import_is_offered_exactly_when_available() {
        assert!(should_offer_rhythmbox_import(true));
        assert!(!should_offer_rhythmbox_import(false));
    }
}
