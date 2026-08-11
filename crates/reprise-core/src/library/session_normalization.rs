//! Validation and compatibility normalization for persisted session state.

use rusqlite::Connection;

use crate::browser::{BrowserPlace, LibraryScope, TrackCollection};
use crate::queue::{Queue, QueueSnapshot, Repeat};
use crate::up_next::QueueItem;
use crate::view_source::ViewSource;

use super::{SessionEpisodeOrigin, SessionSource, SessionState, VERSION};

pub(super) fn resolve_persisted_places(conn: &Connection, mut state: SessionState) -> SessionState {
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
        None => matches!(
            place,
            BrowserPlace::ImportErrors
                | BrowserPlace::MyStats
                | BrowserPlace::Releases
                | BrowserPlace::Concerts
                | BrowserPlace::Podcasts
                | BrowserPlace::Youtube
                | BrowserPlace::Radio
        ),
    }
}

fn row_exists(conn: &Connection, table: &str, id: i64) -> bool {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
    conn.query_row(&sql, [id], |row| row.get(0))
        .unwrap_or(false)
}

pub(super) fn empty_queue() -> QueueSnapshot {
    QueueSnapshot {
        ids: Vec::new(),
        order: Vec::new(),
        position: None,
        repeat: Repeat::Off,
        shuffled: false,
    }
}

pub(super) fn normalize(mut state: SessionState) -> SessionState {
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
    let persisted_episodes = state
        .up_next
        .ids()
        .iter()
        .copied()
        .filter(|item| matches!(item, QueueItem::Episode(_)))
        .collect::<Vec<_>>();
    state.up_next.remove_ids(&persisted_episodes);
    state.up_next.truncate(queue_limit);
    if matches!(state.current_up_next, Some(QueueItem::Episode(_))) {
        state.current_up_next = None;
    }
    if let Some(active) = state.active_episode.as_mut() {
        if active.episode_id <= 0 {
            state.active_episode = None;
        } else if active.origin == SessionEpisodeOrigin::ManualQueue {
            active.neighbour_episode_ids.clear();
        } else {
            active.neighbour_episode_ids.retain(|id| *id > 0);
            active.neighbour_episode_ids.truncate(queue_limit);
        }
    }
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
