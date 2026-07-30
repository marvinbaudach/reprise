//! Versioned, bounded persistence for restorable UI and queue state.

use rusqlite::Connection;

use crate::db::Db;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::queries::BrowseFilter;
use crate::queue::{Queue, QueueSnapshot, Repeat};
use crate::up_next::UpNextQueue;
use crate::{browser::BrowserPlace, view_source::ViewSource};

pub const SESSION_KEY: &str = "ui.session.v1";
const VERSION: u8 = 1;
const DEFAULT_WIDTH: i32 = 1200;
const DEFAULT_HEIGHT: i32 = 800;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum SessionSource {
    #[default]
    Library,
    RecentlyAdded,
    Playlist(i64),
    Smart(i64),
    Queue,
    Missing,
    ImportErrors,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub version: u8,
    pub window_width: i32,
    pub window_height: i32,
    pub maximized: bool,
    pub source: SessionSource,
    pub search: String,
    pub browse: BrowseFilter,
    pub sort_field: String,
    pub sort_dir: String,
    /// The one browser destination restored at startup. Back/Forward stacks
    /// are deliberately not serialized.
    #[serde(default, deserialize_with = "deserialize_optional_browser_place")]
    pub browser_place: Option<BrowserPlace>,
    /// The remembered unscoped Music place, including its local refinements.
    #[serde(default, deserialize_with = "deserialize_optional_browser_place")]
    pub library_root: Option<BrowserPlace>,
    pub queue: QueueSnapshot,
    #[serde(default, deserialize_with = "deserialize_up_next")]
    pub up_next: UpNextQueue,
    #[serde(default, deserialize_with = "deserialize_current_up_next")]
    pub current_up_next: Option<i64>,
    /// Where the current playback snapshot was started from (QUE-1's
    /// "Up Next · from <source>" and NAV-9's jump target). `None` for
    /// sessions saved before the field existed or when nothing was played.
    #[serde(default, deserialize_with = "deserialize_play_origin")]
    pub play_origin: Option<SessionSource>,
    /// Display label resolved at play time (playlist/album/artist name, or
    /// the localized "Music") — stored so the Queue view's section title
    /// survives a restart without re-resolving names that may have changed.
    #[serde(default, deserialize_with = "deserialize_play_origin_label")]
    pub play_origin_label: Option<String>,
    /// Complete immutable playback origin for scoped Album/Artist reveals.
    #[serde(default, deserialize_with = "deserialize_optional_browser_place")]
    pub play_origin_place: Option<BrowserPlace>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            version: VERSION,
            window_width: DEFAULT_WIDTH,
            window_height: DEFAULT_HEIGHT,
            maximized: false,
            source: SessionSource::Library,
            search: String::new(),
            browse: BrowseFilter::default(),
            sort_field: "artist".into(),
            sort_dir: "asc".into(),
            browser_place: Some(BrowserPlace::from(ViewSource::Library)),
            library_root: Some(BrowserPlace::from(ViewSource::Library)),
            queue: empty_queue(),
            up_next: UpNextQueue::default(),
            current_up_next: None,
            play_origin: None,
            play_origin_label: None,
            play_origin_place: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("session serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub fn load(db: &Db) -> SessionState {
    let conn = db.conn();
    let stored = match crate::library::settings::get_setting_in(conn, SESSION_KEY) {
        Ok(Some(stored)) => stored,
        Ok(None) => return SessionState::default(),
        Err(error) => {
            tracing::warn!(%error, "could not read UI session; using defaults");
            return SessionState::default();
        }
    };
    let state: SessionState = match serde_json::from_str(&stored) {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(%error, "invalid UI session JSON; using defaults");
            return SessionState::default();
        }
    };
    if state.version != VERSION {
        tracing::warn!(
            version = state.version,
            "unknown UI session version; using defaults"
        );
        return SessionState::default();
    }
    resolve_persisted_places(conn, normalize(state))
}

pub fn save(db: &Db, state: &SessionState) -> Result<(), SessionError> {
    let conn = db.conn();
    let serialized =
        serde_json::to_string(&resolve_persisted_places(conn, normalize(state.clone())))?;
    crate::library::settings::set_setting_in(conn, SESSION_KEY, &serialized)?;
    Ok(())
}

fn resolve_persisted_places(conn: &Connection, mut state: SessionState) -> SessionState {
    let root = state
        .library_root
        .clone()
        .filter(BrowserPlace::is_library_root)
        .unwrap_or_else(|| BrowserPlace::from(ViewSource::Library));
    if !state
        .browser_place
        .as_ref()
        .is_some_and(|place| place_is_resolvable(conn, place))
    {
        state.browser_place = Some(root.clone());
    }
    if !state
        .play_origin_place
        .as_ref()
        .is_some_and(|place| place_is_resolvable(conn, place))
        && state.play_origin_place.is_some()
    {
        state.play_origin_place = Some(root);
    }
    state
}

fn place_is_resolvable(conn: &Connection, place: &BrowserPlace) -> bool {
    use crate::browser::{LibraryScope, TrackCollection};

    match place.collection() {
        Some(TrackCollection::Playlist(id)) => row_exists(conn, "playlists", *id),
        Some(TrackCollection::Smart(id)) => row_exists(conn, "smart_playlists", *id),
        Some(TrackCollection::Library(LibraryScope::Album(key))) => {
            !key.album.trim().is_empty() && !key.album_artist.trim().is_empty()
        }
        Some(TrackCollection::Library(LibraryScope::Artist(key))) => !key.artist.trim().is_empty(),
        Some(TrackCollection::Library(LibraryScope::Genre(genre))) => !genre.trim().is_empty(),
        Some(
            TrackCollection::Library(LibraryScope::All | LibraryScope::RecentlyAdded)
            | TrackCollection::Queue
            | TrackCollection::Missing,
        ) => true,
        None => matches!(place, BrowserPlace::ImportErrors | BrowserPlace::MyStats),
    }
}

fn row_exists(conn: &Connection, table: &str, id: i64) -> bool {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
    conn.query_row(&sql, [id], |row| row.get(0))
        .unwrap_or(false)
}

fn empty_queue() -> QueueSnapshot {
    QueueSnapshot {
        ids: Vec::new(),
        order: Vec::new(),
        position: None,
        repeat: Repeat::Off,
        shuffled: false,
    }
}

fn normalize(mut state: SessionState) -> SessionState {
    state.version = VERSION;
    state.window_width = state.window_width.clamp(600, 8192);
    state.window_height = state.window_height.clamp(400, 8192);
    truncate_utf8(&mut state.search, 1024);
    if matches!(state.source, SessionSource::Playlist(id) | SessionSource::Smart(id) if id <= 0) {
        state.source = SessionSource::Library;
    }
    const SORT_FIELDS: [&str; 8] = [
        "title",
        "track_no",
        "artist",
        "album",
        "genre",
        "year",
        "duration_ms",
        "rating",
    ];
    if !SORT_FIELDS.contains(&state.sort_field.as_str()) {
        state.sort_field = "title".into();
        state.sort_dir = "asc".into();
    } else if state.sort_dir != "desc" {
        state.sort_dir = "asc".into();
    }
    if matches!(
        state.play_origin,
        Some(SessionSource::Playlist(id) | SessionSource::Smart(id)) if id <= 0
    ) {
        state.play_origin = None;
    }
    let legacy_place = legacy_browser_place(&state);
    if state.browser_place.is_none() {
        state.browser_place = Some(legacy_place.clone());
    }
    if !state
        .library_root
        .as_ref()
        .is_some_and(BrowserPlace::is_library_root)
    {
        state.library_root = Some(if legacy_place.is_library_root() {
            legacy_place
        } else {
            BrowserPlace::from(ViewSource::Library)
        });
    }
    if state.play_origin_place.is_none() {
        state.play_origin_place = state.play_origin.as_ref().map(session_source_place);
    }
    if state.play_origin.is_none() && state.play_origin_place.is_none() {
        state.play_origin_label = None;
    } else if let Some(label) = state.play_origin_label.as_mut() {
        truncate_utf8(label, 256);
    }
    let queue_limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
    state.up_next.truncate(queue_limit);
    if state.queue.ids.len() > queue_limit {
        state.queue = empty_queue();
    } else {
        let mut queue = Queue::new();
        if let Err(error) = queue.restore_snapshot(state.queue.clone()) {
            tracing::warn!(%error, "invalid queue in UI session; dropping queue state");
            state.queue = empty_queue();
        }
    }
    state
}

fn legacy_browser_place(state: &SessionState) -> BrowserPlace {
    let mut place = session_source_place(&state.source);
    if let Some(track_state) = place.track_state_mut() {
        track_state.search = state.search.clone();
        track_state.browse = state.browse.clone();
        track_state.sort = crate::browser::TrackSort::new(
            &state.sort_field,
            if state.sort_dir == "desc" {
                crate::browser::SortDirection::Descending
            } else {
                crate::browser::SortDirection::Ascending
            },
        );
    }
    place
}

fn session_source_place(source: &SessionSource) -> BrowserPlace {
    BrowserPlace::from(match source {
        SessionSource::Library => ViewSource::Library,
        SessionSource::RecentlyAdded => ViewSource::RecentlyAdded,
        SessionSource::Playlist(id) => ViewSource::Playlist(*id),
        SessionSource::Smart(id) => ViewSource::Smart(*id),
        SessionSource::Queue => ViewSource::Queue,
        SessionSource::Missing => ViewSource::Missing,
        SessionSource::ImportErrors => ViewSource::ImportErrors,
    })
}

fn deserialize_up_next<'de, D>(deserializer: D) -> Result<UpNextQueue, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn deserialize_current_up_next<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Tolerates a removed `BrowserPlace` enum variant in old session JSON: an
/// unrecognized value degrades to `None` for that field instead of failing
/// the whole `SessionState` deserialization (which would otherwise discard
/// the complete session — geometry and queue included — over one stale
/// nav place). `normalize`/`resolve_persisted_places` then fall back to the
/// remembered library root, same as any other unresolvable place.
fn deserialize_optional_browser_place<'de, D>(
    deserializer: D,
) -> Result<Option<BrowserPlace>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn deserialize_play_origin<'de, D>(deserializer: D) -> Result<Option<SessionSource>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn deserialize_play_origin_label<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn full_state() -> SessionState {
        SessionState {
            version: VERSION,
            window_width: 1440,
            window_height: 900,
            maximized: true,
            source: SessionSource::Playlist(7),
            search: "live".into(),
            browse: BrowseFilter {
                genre: Some("Rock".into()),
                artist: Some("A".into()),
                album: Some("Stage".into()),
                ..BrowseFilter::default()
            },
            sort_field: "rating".into(),
            sort_dir: "desc".into(),
            browser_place: Some(BrowserPlace::from(ViewSource::Library)),
            library_root: Some(BrowserPlace::from(ViewSource::Library)),
            queue: QueueSnapshot {
                ids: vec![10, 20],
                order: vec![1, 0],
                position: Some(1),
                repeat: Repeat::All,
                shuffled: true,
            },
            up_next: UpNextQueue::default(),
            current_up_next: None,
            play_origin: None,
            play_origin_label: None,
            play_origin_place: None,
        }
    }

    #[test]
    fn session_round_trips_all_fields() {
        let conn = conn();
        let state = full_state();
        save(&conn, &state).unwrap();
        assert_eq!(load(&conn), state);
    }

    #[test]
    fn browse_5_session_round_trips_current_root_and_structured_play_origin() {
        let conn = conn();
        let mut state = full_state();
        let mut current = crate::browser::BrowserPlace::fresh_album("Blue", "Joni Mitchell");
        current.track_state_mut().unwrap().search = "river".into();
        let mut root = crate::browser::BrowserPlace::from(crate::view_source::ViewSource::Library);
        root.track_state_mut().unwrap().selected_ids = vec![7];
        state.browser_place = Some(current.clone());
        state.library_root = Some(root.clone());
        state.play_origin_place = Some(current.clone());

        save(&conn, &state).unwrap();
        let restored = load(&conn);

        assert_eq!(restored.browser_place, Some(current.clone()));
        assert_eq!(restored.library_root, Some(root));
        assert_eq!(restored.play_origin_place, Some(current));
    }

    #[test]
    fn fil_1c_genre_scope_round_trips_as_the_current_browser_place() {
        let conn = conn();
        let mut state = full_state();
        let genre = BrowserPlace::from(ViewSource::Genre("Metalcore".into()));
        state.browser_place = Some(genre.clone());

        save(&conn, &state).unwrap();

        assert_eq!(load(&conn).browser_place, Some(genre));
    }

    #[test]
    fn browse_5_unresolvable_places_fall_back_to_the_remembered_library_root() {
        let conn = conn();
        let mut state = full_state();
        let mut root = BrowserPlace::from(ViewSource::Library);
        root.track_state_mut().unwrap().search = "remembered".into();
        state.library_root = Some(root.clone());
        state.browser_place = Some(BrowserPlace::from(ViewSource::Playlist(9999)));
        state.play_origin = Some(SessionSource::Playlist(9999));
        state.play_origin_place = Some(BrowserPlace::from(ViewSource::Playlist(9999)));
        state.play_origin_label = Some("Deleted".into());

        save(&conn, &state).unwrap();
        let restored = load(&conn);

        assert_eq!(restored.browser_place, Some(root.clone()));
        assert_eq!(restored.play_origin_place, Some(root));
    }

    #[test]
    fn corrupt_or_unknown_version_falls_back_to_default() {
        let conn = conn();
        crate::library::settings::set_setting(&conn, SESSION_KEY, "not-json").unwrap();
        assert_eq!(load(&conn), SessionState::default());
        let mut value = serde_json::to_value(full_state()).unwrap();
        value["version"] = serde_json::json!(99);
        crate::library::settings::set_setting(&conn, SESSION_KEY, &value.to_string()).unwrap();
        assert_eq!(load(&conn), SessionState::default());
    }

    #[test]
    fn session_with_removed_place_variant_falls_back_to_library_root() {
        let conn = conn();
        let mut state = full_state();
        state.play_origin_place = Some(BrowserPlace::from(ViewSource::Library));
        save(&conn, &state).unwrap();

        // Frozen legacy shape: `BrowserPlace` has no serde tag attribute, so
        // a unit variant serializes as a bare string (verified against the
        // still-live enum before it was removed). A session saved before
        // `BrowserPlace::NewReleases` was removed therefore looks exactly
        // like this on disk.
        for removed_place in [
            serde_json::json!("NewReleases"),
            serde_json::json!({"Device": {"serial": "pixel-8"}}),
        ] {
            let mut value = serde_json::to_value(&state).unwrap();
            value["browser_place"] = removed_place.clone();
            value["library_root"] = removed_place.clone();
            value["play_origin_place"] = removed_place;
            crate::library::settings::set_setting(&conn, SESSION_KEY, &value.to_string()).unwrap();

            let restored = load(&conn);

            // The unknown variant must degrade to `None` per field instead of
            // failing the whole `SessionState` deserialization — otherwise the
            // entire session (geometry, queue) is discarded, not just the place.
            assert_eq!(
                restored.browser_place,
                Some(BrowserPlace::from(ViewSource::Library))
            );
            assert!(restored
                .library_root
                .as_ref()
                .is_some_and(BrowserPlace::is_library_root));
            assert_eq!(restored.play_origin_place, None);
            assert_eq!(restored.window_width, state.window_width);
            assert_eq!(restored.window_height, state.window_height);
            assert_eq!(restored.maximized, state.maximized);
            assert_eq!(restored.queue, state.queue);
        }
    }

    #[test]
    fn geometry_search_source_and_sort_are_normalized() {
        let conn = conn();
        let mut state = full_state();
        state.window_width = 1;
        state.window_height = 99_999;
        state.search = "é".repeat(800);
        state.source = SessionSource::Playlist(0);
        state.sort_field = "drop table".into();
        state.sort_dir = "sideways".into();
        save(&conn, &state).unwrap();
        let loaded = load(&conn);
        assert_eq!(loaded.window_width, 600);
        assert_eq!(loaded.window_height, 8192);
        assert!(loaded.search.len() <= 1024);
        assert_eq!(loaded.source, SessionSource::Library);
        assert_eq!(loaded.sort_field, "title");
        assert_eq!(loaded.sort_dir, "asc");
    }

    #[test]
    fn oversized_queue_defaults_safely() {
        let conn = conn();
        let mut state = full_state();
        let len = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap() + 1;
        state.queue.ids = (0..len).map(|id| id as i64).collect();
        state.queue.order = (0..len).collect();
        state.queue.position = Some(0);
        save(&conn, &state).unwrap();
        assert_eq!(load(&conn).queue, empty_queue());
    }

    #[test]
    fn legacy_json_without_play_origin_loads_none() {
        let mut value = serde_json::to_value(full_state()).unwrap();
        value.as_object_mut().unwrap().remove("play_origin");
        value.as_object_mut().unwrap().remove("play_origin_label");
        let state: SessionState = serde_json::from_value(value).unwrap();
        assert_eq!(state.play_origin, None);
        assert_eq!(state.play_origin_label, None);
    }

    #[test]
    fn play_origin_round_trips_and_corrupt_degrades_to_none() {
        let conn = conn();
        let mut state = full_state();
        let playlist_id = crate::library::playlists::create(&conn, "Late Night").unwrap();
        state.play_origin = Some(SessionSource::Playlist(playlist_id));
        state.play_origin_label = Some("Late Night".into());
        state.play_origin_place = Some(BrowserPlace::from(ViewSource::Playlist(playlist_id)));
        save(&conn, &state).unwrap();
        assert_eq!(load(&conn), state);

        let mut value = serde_json::to_value(&state).unwrap();
        value["play_origin"] = serde_json::json!({ "kind": "nonsense" });
        value["play_origin_label"] = serde_json::json!(42);
        let degraded: SessionState = serde_json::from_value(value).unwrap();
        assert_eq!(degraded.play_origin, None);
        assert_eq!(degraded.play_origin_label, None);
    }

    #[test]
    fn play_origin_with_invalid_playlist_id_is_dropped_by_normalize() {
        let conn = conn();
        let mut state = full_state();
        state.play_origin = Some(SessionSource::Smart(0));
        state.play_origin_label = Some("Broken".into());
        save(&conn, &state).unwrap();
        let loaded = load(&conn);
        assert_eq!(loaded.play_origin, None);
        assert_eq!(loaded.play_origin_label, None);
    }

    #[test]
    fn legacy_json_without_up_next_fields_loads_empty_pending_state() {
        let mut value = serde_json::to_value(full_state()).unwrap();
        value.as_object_mut().unwrap().remove("up_next");
        value.as_object_mut().unwrap().remove("current_up_next");
        let state: SessionState = serde_json::from_value(value).unwrap();
        assert!(state.up_next.is_empty());
        assert_eq!(state.current_up_next, None);
    }

    #[test]
    fn corrupt_up_next_fields_degrade_without_dropping_the_session() {
        let mut value = serde_json::to_value(full_state()).unwrap();
        value["up_next"] = serde_json::json!({ "not": "a queue" });
        value["current_up_next"] = serde_json::json!("not an id");
        let state: SessionState = serde_json::from_value(value).unwrap();
        assert!(state.up_next.is_empty());
        assert_eq!(state.current_up_next, None);
        assert_eq!(state.source, SessionSource::Playlist(7));
    }

    #[test]
    fn up_next_fields_round_trip_and_pending_state_is_bounded() {
        let conn = conn();
        let mut state = full_state();
        state.up_next.append(&[30, 40]);
        state.current_up_next = Some(20);
        save(&conn, &state).unwrap();
        assert_eq!(load(&conn), state);

        let len = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap() + 1;
        state.up_next = crate::up_next::UpNextQueue::default();
        state.up_next.append(&(0..len as i64).collect::<Vec<_>>());
        save(&conn, &state).unwrap();
        assert_eq!(load(&conn).up_next.len(), len - 1);
    }
}
