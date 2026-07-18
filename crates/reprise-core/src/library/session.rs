//! Versioned, bounded persistence for restorable UI and queue state.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::queries::BrowseFilter;
use crate::queue::{Queue, QueueSnapshot, Repeat};
use crate::up_next::UpNextQueue;

pub const SESSION_KEY: &str = "ui.session.v1";
const VERSION: u8 = 1;
const DEFAULT_WIDTH: i32 = 1200;
const DEFAULT_HEIGHT: i32 = 800;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum SessionSource {
    #[default]
    Library,
    Playlist(i64),
    Smart(i64),
    Queue,
    Missing,
    ImportErrors,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            queue: empty_queue(),
            up_next: UpNextQueue::default(),
            current_up_next: None,
            play_origin: None,
            play_origin_label: None,
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

pub fn load(conn: &Connection) -> SessionState {
    let stored = match crate::library::settings::get_setting(conn, SESSION_KEY) {
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
    normalize(state)
}

pub fn save(conn: &Connection, state: &SessionState) -> Result<(), SessionError> {
    let serialized = serde_json::to_string(&normalize(state.clone()))?;
    crate::library::settings::set_setting(conn, SESSION_KEY, &serialized)?;
    Ok(())
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
    if state.play_origin.is_none() {
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

    fn conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
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
        state.play_origin = Some(SessionSource::Playlist(7));
        state.play_origin_label = Some("Late Night".into());
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
