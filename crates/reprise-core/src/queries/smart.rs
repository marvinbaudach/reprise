//! `ViewSource::Smart(id)` window/count/ids queries — see the module doc's
//! `Smart(id)` section for the rules-to-SQL translation and the "Smart
//! playlist window math" nested-subquery approach. Split out of the former
//! single-file `queries.rs` (Refactoring & Extensibility Task 1) — a pure
//! move, no behavior change.

use crate::library::playlists::{self, SmartPlaylist};
use crate::models::Track;

use super::clauses::{
    ai_projection, filter_clause, like_pattern, order_clause, row_to_id, row_to_track, PRESENT,
};
use super::queue::QUEUE_LIMIT;
use super::MAX_WINDOW_LIMIT;
use rusqlite::Connection;

/// Loads one `SmartPlaylist` row by id via `playlists::list_smart` (a full
/// scan of the tiny `smart_playlists` table, not worth a bespoke single-row
/// query — see that module's DRY note). `None` if the id doesn't exist
/// (e.g. the playlist was deleted between the sidebar listing it and the
/// user clicking it) — every caller here treats that as "nothing to show",
/// never a hard error.
fn load_smart_playlist(
    conn: &Connection,
    id: i64,
) -> Result<Option<SmartPlaylist>, rusqlite::Error> {
    Ok(playlists::list_smart(conn)?
        .into_iter()
        .find(|p| p.id == id))
}

/// Builds the nested-subquery SELECT + bound params for a `Smart(id)`
/// window — see the module doc's `Smart playlist window math` section.
/// Returns the same `SmartRulesError` `smart_rules_to_sql` would (a bad
/// `rules_json`, most likely from direct DB tampering since Task 2's rules
/// editor validates before saving) rather than a `rusqlite::Error`; callers
/// treat that as "no rows" rather than propagating it as a SQL failure.
fn build_smart_window_query(
    smart: &SmartPlaylist,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
    project_ai: bool,
) -> Result<(String, Vec<rusqlite::types::Value>), playlists::SmartRulesError> {
    let has_filter = !filter.trim().is_empty();
    let member_order = order_clause(&smart.sort_field, &smart.sort_dir);
    let view_order = order_clause(sort_field, sort_dir);
    let (rules_frag, mut params) = playlists::smart_rules_to_sql(&smart.rules_json)?;

    let mut next_idx = params.len() as u8 + 1;
    let is_ai = ai_projection(project_ai);
    let mut inner_sql = format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing_since, missing_reason, untagged, file_size, device, inode, \
         {is_ai} AS is_ai \
         FROM tracks WHERE {PRESENT} AND ({rules_frag})"
    );
    if has_filter {
        inner_sql.push_str(&filter_clause(true, next_idx));
        params.push(rusqlite::types::Value::Text(like_pattern(filter.trim())));
        next_idx += 1;
    }
    inner_sql.push_str(&format!(" ORDER BY {member_order}"));
    if let Some(limit_count) = smart.limit_count {
        inner_sql.push_str(&format!(" LIMIT ?{next_idx}"));
        params.push(rusqlite::types::Value::Integer(limit_count));
        next_idx += 1;
    }

    let limit_idx = next_idx;
    let offset_idx = next_idx + 1;
    let sql = format!(
        "SELECT * FROM ({inner_sql}) ORDER BY {view_order} \
         LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
    );
    params.push(rusqlite::types::Value::Integer(limit));
    params.push(rusqlite::types::Value::Integer(offset));

    Ok((sql, params))
}

pub(super) fn query_track_window_smart(
    conn: &mut Connection,
    smart_id: i64,
    view_sort: (&str, &str),
    filter: &str,
    offset: i64,
    limit: i64,
    project_ai: bool,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let Some(smart) = load_smart_playlist(conn, smart_id)? else {
        tracing::warn!(
            smart_id,
            "smart playlist not found for window query; returning empty"
        );
        return Ok(Vec::new());
    };
    if smart.role.as_deref() == Some(playlists::RECENTLY_ADDED_ROLE) {
        let browse = super::recently_added_browse(&super::BrowseFilter::default());
        return super::library::query_track_window_library(
            conn,
            view_sort.0,
            view_sort.1,
            filter,
            offset,
            limit,
            &browse,
            false,
            project_ai,
        );
    }

    let (sql, params) = match build_smart_window_query(
        &smart,
        view_sort.0,
        view_sort.1,
        filter,
        offset,
        limit,
        project_ai,
    ) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(%error, smart_id, "invalid smart playlist rules; returning empty window");
            return Ok(Vec::new());
        }
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_track)?;
    rows.collect()
}

pub(super) fn query_track_count_smart(
    conn: &Connection,
    smart_id: i64,
    filter: &str,
) -> Result<i64, rusqlite::Error> {
    let Some(smart) = load_smart_playlist(conn, smart_id)? else {
        tracing::warn!(
            smart_id,
            "smart playlist not found for count query; returning 0"
        );
        return Ok(0);
    };
    if smart.role.as_deref() == Some(playlists::RECENTLY_ADDED_ROLE) {
        return super::library::query_track_count_library(
            conn,
            filter,
            &super::recently_added_browse(&super::BrowseFilter::default()),
        );
    }
    let has_filter = !filter.trim().is_empty();
    let (rules_frag, mut params) = match playlists::smart_rules_to_sql(&smart.rules_json) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(%error, smart_id, "invalid smart playlist rules; returning 0");
            return Ok(0);
        }
    };
    let next_idx = params.len() as u8 + 1;
    let mut sql = format!("SELECT count(*) FROM tracks WHERE {PRESENT} AND ({rules_frag})");
    if has_filter {
        sql.push_str(&filter_clause(true, next_idx));
        params.push(rusqlite::types::Value::Text(like_pattern(filter.trim())));
    }
    let raw: i64 = conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
        r.get(0)
    })?;
    Ok(match smart.limit_count {
        Some(n) => raw.min(n),
        None => raw,
    })
}

pub(super) fn query_track_ids_smart(
    conn: &Connection,
    smart_id: i64,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let Some(smart) = load_smart_playlist(conn, smart_id)? else {
        tracing::warn!(
            smart_id,
            "smart playlist not found for ids query; returning empty"
        );
        return Ok(Vec::new());
    };
    if smart.role.as_deref() == Some(playlists::RECENTLY_ADDED_ROLE) {
        return super::query_track_ids_recently_added(
            conn,
            sort_field,
            sort_dir,
            filter,
            &super::BrowseFilter::default(),
            false,
        );
    }
    let has_filter = !filter.trim().is_empty();
    let member_order = order_clause(&smart.sort_field, &smart.sort_dir);
    let view_order = order_clause(sort_field, sort_dir);
    let (rules_frag, mut params) = match playlists::smart_rules_to_sql(&smart.rules_json) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(%error, smart_id, "invalid smart playlist rules; returning empty ids");
            return Ok(Vec::new());
        }
    };
    let next_idx = params.len() as u8 + 1;
    let mut inner_sql = format!(
        "SELECT id, title, artist, album, year, track_no, genre, duration_ms, \
         rating, play_count, added_at FROM tracks WHERE {PRESENT} AND ({rules_frag})"
    );
    if has_filter {
        inner_sql.push_str(&filter_clause(true, next_idx));
        params.push(rusqlite::types::Value::Text(like_pattern(filter.trim())));
    }
    // The smart playlist's own limit bounds the queue too (capped by
    // `QUEUE_LIMIT` for defense in depth, same as every other source's ids
    // query); a literal, not a bound parameter — both operands are
    // Rust-side i64s, never caller-supplied text.
    let effective_limit = smart.limit_count.unwrap_or(QUEUE_LIMIT).min(QUEUE_LIMIT);
    inner_sql.push_str(&format!(" ORDER BY {member_order} LIMIT {effective_limit}"));
    let sql = format!("SELECT id FROM ({inner_sql}) ORDER BY {view_order}");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_id)?;
    rows.collect()
}
