use super::clauses::{
    build_track_ids_query_base, build_track_ids_query_browsed, like_pattern, row_to_id,
    TrackSqlOptions,
};
use super::{browse, library, library_views, playlist, queue, smart, BrowseFilter};
use crate::db::Db;
use crate::models::Track;
use crate::up_next::QueueItem;
use crate::view_source::ViewSource;
use rusqlite::types::Value;
use rusqlite::Connection;

static EMPTY_BROWSE: BrowseFilter = BrowseFilter {
    genre: None,
    artist: None,
    album: None,
    year: None,
    rating: None,
    added_since: None,
};

/// The source and refinements that define one track view.
#[derive(Clone, Copy, Debug)]
pub struct TrackViewQuery<'a> {
    pub source: &'a ViewSource,
    pub filter: &'a str,
    pub browse: &'a BrowseFilter,
    pub queue_items: &'a [QueueItem],
    pub exclude_ai: bool,
}

impl<'a> TrackViewQuery<'a> {
    #[must_use]
    pub fn new(source: &'a ViewSource) -> Self {
        Self {
            source,
            filter: "",
            browse: &EMPTY_BROWSE,
            queue_items: &[],
            exclude_ai: false,
        }
    }

    #[must_use]
    pub fn with_filter(mut self, filter: &'a str) -> Self {
        self.filter = filter;
        self
    }

    #[must_use]
    pub fn with_browse(mut self, browse: &'a BrowseFilter) -> Self {
        self.browse = browse;
        self
    }

    #[must_use]
    pub fn with_queue_items(mut self, items: &'a [QueueItem]) -> Self {
        self.queue_items = items;
        self
    }

    #[must_use]
    pub fn with_exclude_ai(mut self, exclude: bool) -> Self {
        self.exclude_ai = exclude;
        self
    }
}

/// The caller-selected ordering for a track query.
#[derive(Clone, Copy, Debug)]
pub struct TrackSort<'a> {
    pub field: &'a str,
    pub dir: &'a str,
}

/// One requested slice of a track view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowWindow {
    pub offset: i64,
    pub limit: i64,
}

/// Whether a window query should project the track AI-provenance column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiColumn {
    #[default]
    Project,
    Skip,
}

/// Returns one bounded window from the described track view.
pub fn query_track_window(
    db: &Db,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
    rows: RowWindow,
    ai: AiColumn,
) -> Result<Vec<Track>, rusqlite::Error> {
    query_track_window_dispatch(db.conn(), view, sort, rows, ai)
}

/// Counts every row in the described track view.
pub fn query_track_count(db: &Db, view: &TrackViewQuery<'_>) -> Result<i64, rusqlite::Error> {
    query_track_count_dispatch(db.conn(), view)
}

/// Returns the playable track ids represented by the described track view.
pub fn query_track_ids(
    db: &Db,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
) -> Result<Vec<i64>, rusqlite::Error> {
    query_track_ids_in(db.conn(), view, sort)
}

pub(crate) fn query_track_ids_in(
    conn: &Connection,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
) -> Result<Vec<i64>, rusqlite::Error> {
    query_track_ids_dispatch(conn, view, sort)
}

fn query_track_window_dispatch(
    conn: &Connection,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
    rows: RowWindow,
    ai: AiColumn,
) -> Result<Vec<Track>, rusqlite::Error> {
    let source = view.source;
    let browse = view.browse;
    match source {
        ViewSource::Library => library::query_track_window_library(conn, view, sort, rows, ai),
        ViewSource::RecentlyAdded => {
            let browse = recently_added_browse(browse);
            let scoped = TrackViewQuery {
                browse: &browse,
                ..*view
            };
            library::query_track_window_library(conn, &scoped, sort, rows, ai)
        }
        ViewSource::Missing => library::query_track_window_missing(conn, view, sort, rows, ai),
        ViewSource::Playlist(id) => {
            playlist::query_track_window_playlist(conn, *id, view, sort, rows, ai)
        }
        ViewSource::Smart(id) => smart::query_track_window_smart(conn, *id, view, sort, rows, ai),
        ViewSource::Queue => queue::query_track_window_queue(conn, view, rows, ai),
        ViewSource::Album {
            album,
            album_artist,
        } => library_views::query_album_track_window(
            conn,
            library_views::AlbumKey {
                album,
                album_artist,
            },
            library_views::TrackRefinement::from(view),
            sort,
            rows,
            ai,
        ),
        ViewSource::Artist(artist) => library_views::query_artist_track_window(
            conn,
            artist,
            library_views::TrackRefinement::from(view),
            sort,
            rows,
            ai,
        ),
        ViewSource::Genre(genre) => {
            let browse = genre_browse(genre, browse);
            let scoped = TrackViewQuery {
                browse: &browse,
                ..*view
            };
            library::query_track_window_library(conn, &scoped, sort, rows, ai)
        }
        ViewSource::ImportErrors
        | ViewSource::MyStats
        | ViewSource::Releases
        | ViewSource::Concerts
        | ViewSource::Podcasts
        | ViewSource::Youtube
        | ViewSource::Radio
        | ViewSource::Conversions => Ok(Vec::new()),
    }
}

fn query_track_count_dispatch(
    conn: &Connection,
    view: &TrackViewQuery<'_>,
) -> Result<i64, rusqlite::Error> {
    let source = view.source;
    let browse = view.browse;
    match source {
        ViewSource::Library => library::query_track_count_library_ai(conn, view),
        ViewSource::RecentlyAdded => {
            let browse = recently_added_browse(browse);
            let scoped = TrackViewQuery {
                browse: &browse,
                ..*view
            };
            library::query_track_count_library_ai(conn, &scoped)
        }
        ViewSource::Missing => library::query_track_count_missing(conn, view),
        ViewSource::Playlist(id) => playlist::query_track_count_playlist(conn, *id, view),
        ViewSource::Smart(id) => smart::query_track_count_smart(conn, *id, view),
        // Stage-3 close-out fix: this used to trust `queue_items.len()`
        // verbatim, on the documented assumption that nothing hard-deletes a
        // `tracks` row. That assumption no longer holds (`remove_missing_tracks`
        // does exactly that) — the queue itself is purged in lockstep by
        // `ui::player_controller::PlayerController::purge_queue_ids` whenever a
        // hard-delete happens through the app's own UI, but counting matched
        // rows here (rather than trusting the caller's `queue_items` slice) is
        // a second, independent guarantee that a `ColumnView` can never be told
        // there are more rows than `query_track_window_queue` will actually
        // render, even if some future caller forgets to purge the queue after a
        // hard-delete.
        ViewSource::Queue => queue::query_track_count_queue(conn, view),
        ViewSource::Album {
            album,
            album_artist,
        } => library_views::query_album_track_count(
            conn,
            library_views::AlbumKey {
                album,
                album_artist,
            },
            library_views::TrackRefinement::from(view),
        ),
        ViewSource::Artist(artist) => library_views::query_artist_track_count(
            conn,
            artist,
            library_views::TrackRefinement::from(view),
        ),
        ViewSource::Genre(genre) => {
            let browse = genre_browse(genre, browse);
            let scoped = TrackViewQuery {
                browse: &browse,
                ..*view
            };
            library::query_track_count_library_ai(conn, &scoped)
        }
        ViewSource::ImportErrors
        | ViewSource::MyStats
        | ViewSource::Releases
        | ViewSource::Concerts
        | ViewSource::Podcasts
        | ViewSource::Youtube
        | ViewSource::Radio
        | ViewSource::Conversions => Ok(0),
    }
}

fn query_track_ids_dispatch(
    conn: &Connection,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
) -> Result<Vec<i64>, rusqlite::Error> {
    let source = view.source;
    let filter = view.filter;
    let browse = view.browse;
    match source {
        ViewSource::Library => query_library_track_ids(conn, view, sort),
        ViewSource::RecentlyAdded => query_track_ids_recently_added(conn, view, sort),
        ViewSource::Missing => {
            let has_filter = !filter.trim().is_empty();
            let sql = build_track_ids_query_base(1, sort, has_filter);
            let mut stmt = conn.prepare(&sql)?;
            let like = like_pattern(filter.trim());
            let rows = if has_filter {
                stmt.query_map(rusqlite::params![like], row_to_id)?
            } else {
                stmt.query_map([], row_to_id)?
            };
            rows.collect()
        }
        ViewSource::Playlist(id) => playlist::query_playable_track_ids_playlist(conn, *id, view),
        ViewSource::Smart(id) => smart::query_track_ids_smart(conn, *id, view, sort),
        ViewSource::Queue => Ok(view
            .queue_items
            .iter()
            .filter_map(|item| item.track_id())
            .collect()),
        ViewSource::Album {
            album,
            album_artist,
        } => library_views::query_album_track_ids_browsed(
            conn,
            library_views::AlbumKey {
                album,
                album_artist,
            },
            library_views::TrackRefinement::from(view),
            sort,
        ),
        ViewSource::Artist(artist) => library_views::query_artist_track_ids(
            conn,
            artist,
            library_views::TrackRefinement::from(view),
            sort,
        ),
        ViewSource::Genre(genre) => {
            let browse = genre_browse(genre, browse);
            let scoped = TrackViewQuery {
                browse: &browse,
                ..*view
            };
            query_library_track_ids(conn, &scoped, sort)
        }
        ViewSource::ImportErrors
        | ViewSource::MyStats
        | ViewSource::Releases
        | ViewSource::Concerts
        | ViewSource::Podcasts
        | ViewSource::Youtube
        | ViewSource::Radio
        | ViewSource::Conversions => Ok(Vec::new()),
    }
}

fn query_library_track_ids(
    conn: &Connection,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
) -> Result<Vec<i64>, rusqlite::Error> {
    let has_filter = !view.filter.trim().is_empty();
    let sql = build_track_ids_query_browsed(
        sort,
        has_filter,
        TrackSqlOptions::from_view(view, AiColumn::Skip),
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params = Vec::new();
    if has_filter {
        params.push(Value::Text(like_pattern(view.filter.trim())));
    }
    let (_, browse_values) = browse::browse_clause(view.browse, params.len() + 1);
    params.extend(browse_values.into_iter().map(Value::Text));
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_id)?;
    rows.collect()
}

pub(super) fn recently_added_browse(browse: &BrowseFilter) -> BrowseFilter {
    const SEVEN_DAYS_SECONDS: i64 = 7 * 24 * 60 * 60;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    BrowseFilter {
        added_since: Some(now.saturating_sub(SEVEN_DAYS_SECONDS).to_string()),
        ..browse.clone()
    }
}

pub(super) fn query_track_ids_recently_added(
    conn: &Connection,
    view: &TrackViewQuery<'_>,
    sort: TrackSort<'_>,
) -> Result<Vec<i64>, rusqlite::Error> {
    let browse = recently_added_browse(view.browse);
    let scoped = TrackViewQuery {
        browse: &browse,
        ..*view
    };
    query_library_track_ids(conn, &scoped, sort)
}

fn genre_browse(genre: &str, browse: &BrowseFilter) -> BrowseFilter {
    let mut scoped = browse.clone();
    scoped.genre = Some(genre.trim().to_owned());
    scoped
}

/// Returns the ids represented by the current visible view. This differs
/// from [`query_track_ids`] only for manual playlists: their missing members
/// remain selectable at their durable positions, while playback continues to
/// seed queues from playable rows only.
pub fn query_visible_track_ids_browsed(
    db: &Db,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    queue_items: &[QueueItem],
) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = db.conn();
    let view = TrackViewQuery::new(source)
        .with_filter(filter)
        .with_browse(browse)
        .with_queue_items(queue_items);
    let sort = TrackSort {
        field: sort_field,
        dir: sort_dir,
    };
    match source {
        ViewSource::Playlist(id) => {
            playlist::query_visible_track_ids_playlist(conn, *id, &view, sort)
        }
        _ => query_track_ids_dispatch(conn, &view, sort),
    }
}
