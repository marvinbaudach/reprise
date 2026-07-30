//! Library-wide status-bar aggregates.

use rusqlite::Connection;

use crate::db::Db;

use super::browse::BrowseFilter;
use super::clauses::PRESENT;
use super::library;

#[derive(Debug)]
pub struct LibraryStats {
    pub track_count: i64,
    pub total_duration_ms: i64,
    /// `Some(n)` while a search filter is active (status line shows "N of M
    /// tracks"), `None` when it isn't. See [`query_library_stats`].
    pub filtered_count: Option<i64>,
}

/// Aggregates library-wide stats over all non-missing tracks. Powers the
/// status line (`ui::status_bar`). `track_count`/`total_duration_ms` always
/// describe the *whole* library, regardless of `filter` — only `filtered_
/// count` reacts to it, becoming `Some(query_track_count(conn, filter))` when
/// `filter` is non-empty (trimmed) and `None` otherwise, so a status line
/// with no active search reads exactly as it did before `filter` existed.
/// Deliberately library-only, unaffected by `ViewSource` (Stage 3 Task 3):
/// the status line only ever shows library-wide totals; for non-Library
/// sources `ui::status_bar` hides the line outright — there the filter
/// row is the one count on screen.
pub fn query_library_stats(db: &Db, filter: &str) -> Result<LibraryStats, rusqlite::Error> {
    let conn = db.conn();
    query_library_stats_browsed_conn(conn, filter, &BrowseFilter::default())
}

pub fn query_library_stats_browsed(
    db: &Db,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<LibraryStats, rusqlite::Error> {
    let conn = db.conn();
    query_library_stats_browsed_conn(conn, filter, browse)
}

fn query_library_stats_browsed_conn(
    conn: &Connection,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<LibraryStats, rusqlite::Error> {
    let (track_count, total_duration_ms) = conn.query_row(
        &format!("SELECT count(*), coalesce(sum(duration_ms),0) FROM tracks WHERE {PRESENT}"),
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let filtered_count = if filter.trim().is_empty() && browse.is_empty() {
        None
    } else {
        Some(library::query_track_count_library(conn, filter, browse)?)
    };
    Ok(LibraryStats {
        track_count,
        total_duration_ms,
        filtered_count,
    })
}
