use super::{
    query_track_count_dispatch, query_track_ids_dispatch, query_track_window_dispatch, BrowseFilter,
};
use crate::db::Db;
use crate::models::Track;
use crate::up_next::QueueItem;
use crate::view_source::ViewSource;
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
