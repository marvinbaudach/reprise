//! Typed track-column layout, persistence format, and GTK column registry.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use thiserror::Error;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;
use crate::ui::track_list::Shared;
use crate::ui::track_list_columns::{append_column, append_cover_column, append_rating_column};
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
}

const DEFAULT_ORDER: [ColumnId; 9] = [
    ColumnId::Cover,
    ColumnId::Title,
    ColumnId::Artist,
    ColumnId::Album,
    ColumnId::Year,
    ColumnId::Duration,
    ColumnId::Rating,
    ColumnId::TrackNumber,
    ColumnId::Genre,
];

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
        }
    }

    fn parse(value: &str) -> Option<Self> {
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
            _ => None,
        }
    }
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
            _ => continue,
        };
        if visible.insert(id) {
            order.push(id);
        }
    }
    normalize(order, visible)
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
        0.0,
        false,
        |t| t.title.clone(),
    );
    let track_number = append_column(
        view,
        shared,
        "track_no",
        &strings::text(strings::COLUMN_TRACK_NUMBER),
        1.0,
        true,
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
        0.0,
        false,
        |t| t.artist.clone(),
    );
    let album = append_column(
        view,
        shared,
        "album",
        &strings::text(strings::COLUMN_ALBUM),
        0.0,
        false,
        |t| t.album.clone(),
    );
    let genre = append_column(
        view,
        shared,
        "genre",
        &strings::text(strings::COLUMN_GENRE),
        0.0,
        false,
        |t| t.genre.clone(),
    );
    let year = append_column(
        view,
        shared,
        "year",
        &strings::text(strings::COLUMN_YEAR),
        0.0,
        false,
        |t| t.year.map(|value| value.to_string()).unwrap_or_default(),
    );
    let duration = append_column(
        view,
        shared,
        "duration_ms",
        &strings::text(strings::COLUMN_LENGTH),
        1.0,
        true,
        |t| format_duration(t.duration_ms),
    );
    let rating = append_rating_column(view, shared);

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
    ]);
    let registry = ColumnRegistry {
        view: view.clone(),
        columns,
    };
    let layout = {
        let conn = shared.conn.borrow();
        let stored = reprise_core::library::settings::get_setting(
            &conn,
            reprise_core::library::settings::COLUMN_LAYOUT_KEY,
        )
        .ok()
        .flatten();
        let layout = stored.as_deref().and_then(parse_layout).unwrap_or_default();
        let canonical = serialize_layout(&layout);
        if stored.as_deref() != Some(&canonical) {
            if let Err(error) = reprise_core::library::settings::set_setting(
                &conn,
                reprise_core::library::settings::COLUMN_LAYOUT_KEY,
                &canonical,
            ) {
                tracing::warn!(%error, "could not persist canonical column layout");
            }
        }
        layout
    };
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
        let tokens =
            ["rating", "duration", "album", "artist", "date", "post-time"].map(str::to_string);
        let layout = import_rhythmbox_tokens(&tokens);
        assert_eq!(
            layout.order[..7],
            [
                ColumnId::Cover,
                ColumnId::Title,
                ColumnId::Rating,
                ColumnId::Duration,
                ColumnId::Album,
                ColumnId::Artist,
                ColumnId::Year,
            ]
        );
        assert_eq!(layout.visible.len(), 7);
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
}
