//! `ViewSource::Library`/`ViewSource::Missing` window and count queries —
//! the two "flat `tracks` table, no join" sources, distinguished only by the
//! `missing` flag. Split out of the former single-file `queries.rs`
//! (Refactoring & Extensibility Task 1) — a pure move, no behavior change.

use crate::models::Track;

use super::clauses::{
    build_track_query_base, build_track_query_browsed, filter_clause, like_pattern, row_to_track,
};
use super::MAX_WINDOW_LIMIT;
use super::{browse::browse_clause, BrowseFilter};
use rusqlite::types::Value;
use rusqlite::Connection;

pub(super) fn query_track_window_library(
    conn: &mut Connection,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
    browse: &BrowseFilter,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_query_browsed(sort_field, sort_dir, has_filter, browse);
    let mut stmt = conn.prepare(&sql)?;
    let mut params = vec![Value::Integer(limit), Value::Integer(offset)];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let (_, browse_values) = browse_clause(browse, params.len() + 1);
    params.extend(browse_values.into_iter().map(Value::Text));
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_track)?;
    rows.collect()
}

pub(super) fn query_track_window_missing(
    conn: &mut Connection,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_query_base(1, sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = like_pattern(filter.trim());
    let rows = if has_filter {
        stmt.query_map(rusqlite::params![limit, offset, like], row_to_track)?
    } else {
        stmt.query_map(rusqlite::params![limit, offset], row_to_track)?
    };
    rows.collect()
}

pub(super) fn query_track_count_library(
    conn: &Connection,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let browse_first_param = if has_filter { 2 } else { 1 };
    let (browse_clause, browse_values) = browse_clause(browse, browse_first_param);
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 0{}{browse_clause}",
        filter_clause(has_filter, 1),
    );
    let mut params = Vec::new();
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    params.extend(browse_values.into_iter().map(Value::Text));
    conn.query_row(&sql, rusqlite::params_from_iter(params), |r| r.get(0))
}

pub(super) fn query_track_count_missing(
    conn: &Connection,
    filter: &str,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 1{}",
        filter_clause(has_filter, 1)
    );
    if has_filter {
        let like = like_pattern(filter.trim());
        conn.query_row(&sql, rusqlite::params![like], |r| r.get(0))
    } else {
        conn.query_row(&sql, [], |r| r.get(0))
    }
}
