//! SQL query layer for the track list: one set of windowed/count/id queries
//! shared by every `ViewSource` (Stage 3 Task 3 — "one list, many sources").
//! `query_track_window`/`query_track_count`/`query_track_ids` each `match`
//! on the caller's `ViewSource` and dispatch to a private per-source
//! function; SQL stays the single source of truth for ordering/filtering
//! for every source, exactly as it already was for the library-only case
//! this module supported before this task.
//!
//! ## Per-source shape
//!
//! - **Library**: `missing = 0` — unchanged from before this task.
//! - **Missing**: identical shape to Library, `missing = 1` instead.
//! - **Playlist(id)**: `JOIN playlist_tracks pt ON pt.track_id = tracks.id
//!   WHERE pt.playlist_id = id AND missing = 0`. Duplicates (the same track
//!   added to a playlist twice) surface as separate, position-keyed rows —
//!   a natural consequence of the join, matching Task 2's manual-playlist
//!   semantics. Default order is `pt.position` via a whitelist *sentinel*
//!   sort field, `"playlist_order"` (see `SORT_WHITELIST`) — not a
//!   passthrough of arbitrary text, so the whitelist is never weakened by
//!   this addition. A column header click still works: `track_list.rs`
//!   passes a normal whitelisted field (e.g. `"title"`) instead, and this
//!   module's shared `order_expr_and_dir` treats it exactly like any other
//!   source's sort. `"playlist_order"` only resolves to valid SQL when the
//!   query being built actually joins `playlist_tracks AS pt` — i.e. only
//!   for the `Playlist` source — which holds because `track_list.rs` is the
//!   sole place that decides which sort field accompanies which source.
//! - **Smart(id)**: loads the `SmartPlaylist` row, ANDs `library::playlists::
//!   smart_rules_to_sql`'s WHERE fragment with `missing = 0` and the live
//!   search filter, and orders/limits by the smart playlist's *own*
//!   `sort_field`/`sort_dir`/`limit_count` — not whatever `track_list.rs`'s
//!   current column sort happens to be (a smart playlist's sort is part of
//!   its definition, not the view's). `sort_field`/`sort_dir` still run
//!   through the shared `order_expr_and_dir`, so a hand-edited (DB-tampered)
//!   `smart_playlists.sort_field` silently falls back to title order, same
//!   as every other source (see `smart_playlist_window_falls_back_to_title_
//!   on_tampered_sort_field` below).
//!
//!   ### Smart playlist window math
//!
//!   A smart playlist's own `limit_count` (e.g. "Top 50 rated") must bound
//!   the *whole* view, not just the first window: requesting window
//!   `offset=40, limit=20` against a 50-row-limited smart playlist must
//!   return at most 10 rows (positions 40..49), never rows the smart
//!   playlist doesn't actually contain, even if the underlying `WHERE`
//!   clause matches hundreds of tracks. `build_smart_window_query` gets
//!   this right with a nested subquery rather than Rust-side arithmetic:
//!   the *inner* query applies the rules/filter/order and the smart
//!   playlist's own `LIMIT` first, producing exactly its member set in
//!   order; the *outer* query re-applies the same `ORDER BY` (a subquery's
//!   row order is not guaranteed to survive) and slices out the caller's
//!   window via its own `LIMIT`/`OFFSET`. `query_track_count`'s smart arm
//!   mirrors this with plain arithmetic (`raw_count.min(limit_count)`)
//!   since a count has no rows to slice.
//! - **Queue**: ids are supplied by the caller (`queue_ids: &[i64]`, sourced
//!   from `queue::Queue::ids_in_order` via `ui::player_controller::
//!   queue_ids_snapshot`) in the queue's current play order (reflecting
//!   shuffle, if active). Rather than a SQL `CASE`/temp-table trick to make
//!   `ORDER BY id IN (...)` preserve that order, the window function slices
//!   `ids[offset..offset+limit]` in Rust first, runs one unordered `id IN
//!   (...)` query for just that slice, and reorders the results back to the
//!   slice's order in Rust (an id with no matching row — e.g. deleted since
//!   being queued — is silently skipped, not an error). This is simpler
//!   than teaching SQL about an arbitrary Rust-side order and just as fast
//!   for the bounded (`MAX_WINDOW_LIMIT`) sizes involved. `query_track_ids`
//!   for `Queue` returns `queue_ids` verbatim (it already *is* "every id in
//!   the current view, in order" — the reason `query_track_ids` exists at
//!   all); `query_track_count` is simply `queue_ids.len()`. Both ignore the
//!   live search filter — searching *within* the queue view is left to a
//!   later stage; see the module's `ViewSource::Queue` arms.
//! - **ImportErrors**: Task 8 defines the real (non-`tracks`) row shape and
//!   columns; every query here degrades to an empty window/zero count for
//!   this source in the meantime (see `ViewSource`'s own doc comment).
//!   `query_import_error_count` exposes the one piece of this source this
//!   task builds ahead of time: a bare count of the existing `import_errors`
//!   table, for a future sidebar badge.

use std::collections::HashMap;

use crate::library::playlists::{self, SmartPlaylist};
use crate::models::Track;
use crate::ui::view_source::ViewSource;
use rusqlite::{Connection, OptionalExtension};

/// Global constraint: window queries never return more rows than this in one
/// page, regardless of what the caller requests. SQLite treats a negative
/// `LIMIT` as "unlimited", so this also protects against a bad UI-side page
/// size from turning into a full-table scan. Limits capped.
const MAX_WINDOW_LIMIT: i64 = 500;

/// Hard cap on how many track ids `query_track_ids` will ever return in one
/// call. This is a *separate* constant from `MAX_WINDOW_LIMIT` on purpose:
/// `query_track_ids` powers the queue (Stage 2 Task 4 — "play this whole
/// view"), which legitimately wants every matching id, not one `ColumnView`
/// page. `MAX_WINDOW_LIMIT` (500) is sized for a UI page; a queue is
/// reasonably built from a much larger library, but still must not turn a
/// huge/unfiltered library into an unbounded query. 10,000 tracks is a very
/// large personal library and a small `Vec<i64>` (~80 KB) even at the cap.
/// Callers should compare the returned `Vec`'s length against this constant
/// via `is_queue_capped` and log a warning when it's capped, since the `Vec`
/// alone can't distinguish "capped" from "library has exactly this many
/// tracks".
pub const QUEUE_LIMIT: i64 = 10_000;

#[derive(Debug)]
pub struct LibraryStats {
    pub track_count: i64,
    pub total_duration_ms: i64,
    /// `Some(n)` while a search filter is active (status line shows "N of M
    /// tracks"), `None` when it isn't. See `query_library_stats`.
    pub filtered_count: Option<i64>,
}

/// `"playlist_order"` is a *sentinel* entry, not a real column: it only
/// resolves to valid SQL (`pt.position`) inside a query that actually joins
/// `playlist_tracks AS pt` — see the module doc's `Playlist(id)` section for
/// why that's safe (only `ViewSource::Playlist` queries ever pass it).
const SORT_WHITELIST: [(&str, &str); 7] = [
    ("title", "title COLLATE NOCASE"),
    (
        "artist",
        "artist COLLATE NOCASE, album COLLATE NOCASE, track_no",
    ),
    ("album", "album COLLATE NOCASE, track_no"),
    ("year", "year"),
    ("duration_ms", "duration_ms"),
    ("rating", "rating"),
    ("playlist_order", "pt.position"),
];

/// Shared LIKE-filter clause on `(title, artist, album, genre)`, parameterized
/// by the positional index of the bound `?N` placeholder: callers bind the
/// filter value at whatever placeholder index is free once their own
/// preceding parameters (limit/offset, a playlist id, smart-rules params,
/// …) are accounted for. Every source's window/count queries build their
/// WHERE clause through this one function so the filtered columns and LIKE
/// semantics can never drift apart between a count and the rows it
/// describes (DRY).
fn filter_clause(has_filter: bool, param_index: u8) -> String {
    if has_filter {
        format!(
            " AND (title LIKE ?{param_index} OR artist LIKE ?{param_index} \
             OR album LIKE ?{param_index} OR genre LIKE ?{param_index})"
        )
    } else {
        String::new()
    }
}

/// Resolves `sort_field`/`sort_dir` to a whitelisted `ORDER BY` expression
/// and direction keyword. Shared by every source's window/ids query builder
/// so they can never disagree about what a given sort field/direction
/// means. `sort_field` is only ever used as a lookup key into `SORT_
/// WHITELIST` — never interpolated into SQL directly — so caller input
/// cannot inject arbitrary SQL. Unknown sort fields silently fall back to
/// sorting by title (this is also what makes a DB-tampered smart-playlist
/// `sort_field` degrade safely — see the module doc's `Smart(id)` section).
fn order_expr_and_dir(sort_field: &str, sort_dir: &str) -> (&'static str, &'static str) {
    let order_expr = SORT_WHITELIST
        .iter()
        .find(|(k, _)| *k == sort_field)
        .map_or("title COLLATE NOCASE", |(_, v)| *v);
    let dir = if sort_dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };
    (order_expr, dir)
}

/// Builds the parameterized library/missing SELECT for a track window;
/// `missing_flag` is `0` for the library view, `1` for the missing-files
/// view — a Rust-side literal (`0`/`1`), never caller input, so it's safe to
/// interpolate directly. `sort_field` is only ever used to look up an entry
/// in `SORT_WHITELIST` — it is never interpolated into the SQL string
/// directly, so caller input cannot inject arbitrary SQL. Unknown sort
/// fields silently fall back to sorting by title.
fn build_track_query_base(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 3);
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing, file_size, device, inode \
         FROM tracks WHERE missing = {missing_flag}{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

/// Builds the parameterized SELECT for a library window (`missing = 0`).
/// See `build_track_query_base`'s doc comment for the whitelist guarantee.
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    build_track_query_base(0, sort_field, sort_dir, has_filter)
}

/// Builds the parameterized `SELECT id` for the queue seam
/// (`query_track_ids`, library/missing shape): every id matching
/// `(missing_flag, sort_field, sort_dir, filter)`, capped at `QUEUE_LIMIT` —
/// a literal, not a bound parameter, since it's a fixed Rust-side constant
/// rather than caller input (nothing to inject). Shares `order_expr_and_dir`/
/// `filter_clause` with `build_track_query_base` so the queue's ordering can
/// never drift from the track list's.
fn build_track_ids_query_base(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    format!(
        "SELECT id FROM tracks WHERE missing = {missing_flag}{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT {QUEUE_LIMIT}"
    )
}

/// Builds the parameterized `SELECT id` for the library queue seam
/// (`missing = 0`). See `build_track_ids_query_base`'s doc comment.
pub fn build_track_ids_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    build_track_ids_query_base(0, sort_field, sort_dir, has_filter)
}

/// Builds the parameterized SELECT for a `Playlist(id)` window — see the
/// module doc's `Playlist(id)` section for the join shape, the `"playlist_
/// order"` sentinel, and the duplicates-as-separate-rows behavior.
/// `missing = 0` is applied here too: a track that later vanishes from disk
/// drops out of every playlist's view and resurfaces only in the dedicated
/// `Missing` source, exactly like the library view.
fn build_playlist_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 4);
    format!(
        "SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album, \
         tracks.album_artist, tracks.year, tracks.track_no, tracks.genre, \
         tracks.duration_ms, tracks.bitrate_kbps, tracks.rating, tracks.play_count, \
         tracks.last_played_at, tracks.added_at, tracks.file_mtime, tracks.missing, \
         tracks.file_size, tracks.device, tracks.inode \
         FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?3 AND tracks.missing = 0{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

fn row_to_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        album_artist: r.get(5)?,
        year: r.get(6)?,
        track_no: r.get(7)?,
        genre: r.get(8)?,
        duration_ms: r.get(9)?,
        bitrate_kbps: r.get(10)?,
        rating: r.get(11)?,
        play_count: r.get(12)?,
        last_played_at: r.get(13)?,
        added_at: r.get(14)?,
        file_mtime: r.get(15)?,
        missing: r.get::<_, i64>(16)? != 0,
        file_size: r.get(17)?,
        device: r.get(18)?,
        inode: r.get(19)?,
    })
}

fn row_to_id(r: &rusqlite::Row) -> rusqlite::Result<i64> {
    r.get(0)
}

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
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<(String, Vec<rusqlite::types::Value>), playlists::SmartRulesError> {
    let has_filter = !filter.trim().is_empty();
    let (order_expr, dir) = order_expr_and_dir(&smart.sort_field, &smart.sort_dir);
    let (rules_frag, mut params) = playlists::smart_rules_to_sql(&smart.rules_json)?;

    let mut next_idx = params.len() as u8 + 1;
    let mut inner_sql = format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing, file_size, device, inode \
         FROM tracks WHERE missing = 0 AND ({rules_frag})"
    );
    if has_filter {
        inner_sql.push_str(&filter_clause(true, next_idx));
        params.push(rusqlite::types::Value::Text(format!("%{}%", filter.trim())));
        next_idx += 1;
    }
    inner_sql.push_str(&format!(" ORDER BY {order_expr} {dir}"));
    if let Some(limit_count) = smart.limit_count {
        inner_sql.push_str(&format!(" LIMIT ?{next_idx}"));
        params.push(rusqlite::types::Value::Integer(limit_count));
        next_idx += 1;
    }

    let limit_idx = next_idx;
    let offset_idx = next_idx + 1;
    let sql = format!(
        "SELECT * FROM ({inner_sql}) ORDER BY {order_expr} {dir} LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
    );
    params.push(rusqlite::types::Value::Integer(limit));
    params.push(rusqlite::types::Value::Integer(offset));

    Ok((sql, params))
}

fn query_track_window_library(
    conn: &mut Connection,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_query(sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = format!("%{}%", filter.trim());
    let rows = if has_filter {
        stmt.query_map(rusqlite::params![limit, offset, like], row_to_track)?
    } else {
        stmt.query_map(rusqlite::params![limit, offset], row_to_track)?
    };
    rows.collect()
}

fn query_track_window_missing(
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
    let like = format!("%{}%", filter.trim());
    let rows = if has_filter {
        stmt.query_map(rusqlite::params![limit, offset, like], row_to_track)?
    } else {
        stmt.query_map(rusqlite::params![limit, offset], row_to_track)?
    };
    rows.collect()
}

fn query_track_window_playlist(
    conn: &mut Connection,
    playlist_id: i64,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let sql = build_playlist_track_query(sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = format!("%{}%", filter.trim());
    let rows = if has_filter {
        stmt.query_map(
            rusqlite::params![limit, offset, playlist_id, like],
            row_to_track,
        )?
    } else {
        stmt.query_map(rusqlite::params![limit, offset, playlist_id], row_to_track)?
    };
    rows.collect()
}

fn query_track_window_smart(
    conn: &mut Connection,
    smart_id: i64,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let Some(smart) = load_smart_playlist(conn, smart_id)? else {
        tracing::warn!(
            smart_id,
            "smart playlist not found for window query; returning empty"
        );
        return Ok(Vec::new());
    };

    let (sql, params) = match build_smart_window_query(&smart, filter, offset, limit) {
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

/// Window over an explicit id list, in that list's own order — see the
/// module doc's `Queue` section for why this slices in Rust rather than
/// asking SQL to preserve an arbitrary order.
fn query_track_window_queue(
    conn: &Connection,
    ids: &[i64],
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    if limit == 0 || offset < 0 {
        return Ok(Vec::new());
    }
    let offset = offset as usize;
    if offset >= ids.len() {
        return Ok(Vec::new());
    }
    let end = (offset + limit as usize).min(ids.len());
    let slice = &ids[offset..end];
    if slice.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (1..=slice.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing, file_size, device, inode \
         FROM tracks WHERE id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<Track> = stmt
        .query_map(rusqlite::params_from_iter(slice.iter()), row_to_track)?
        .collect::<Result<_, _>>()?;

    let mut by_id: HashMap<i64, Track> = rows.into_iter().map(|t| (t.id, t)).collect();
    Ok(slice.iter().filter_map(|id| by_id.remove(id)).collect())
}

/// Runs the windowed track query for `source`. `queue_ids` is only read for
/// `ViewSource::Queue` (see the module doc's `Queue` section); every other
/// source ignores it, so callers that never show the Queue source may pass
/// `&[]`. `filter` is always bound as a parameter, never concatenated into
/// the SQL text (except for `Queue`, which doesn't apply `filter` at all —
/// see the module doc).
///
/// `#[allow(clippy::too_many_arguments)]`: every one of these eight is an
/// independently meaningful, already-minimal piece of "which rows, in what
/// order, from where" (source, the three sort/filter values, the window
/// bounds, and the queue's own id list) — bundling any subset into a struct
/// would just move the same eight values one level of indirection away
/// without making any single call site clearer, and every one of this
/// function's several callers (`TrackListModel::track_at`, this module's own
/// tests) already names each argument positionally in a way that reads
/// clearly against this doc comment.
#[allow(clippy::too_many_arguments)]
pub fn query_track_window(
    conn: &mut Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
    queue_ids: &[i64],
) -> Result<Vec<Track>, rusqlite::Error> {
    match source {
        ViewSource::Library => {
            query_track_window_library(conn, sort_field, sort_dir, filter, offset, limit)
        }
        ViewSource::Missing => {
            query_track_window_missing(conn, sort_field, sort_dir, filter, offset, limit)
        }
        ViewSource::Playlist(id) => {
            query_track_window_playlist(conn, *id, sort_field, sort_dir, filter, offset, limit)
        }
        ViewSource::Smart(id) => query_track_window_smart(conn, *id, filter, offset, limit),
        ViewSource::Queue => query_track_window_queue(conn, queue_ids, offset, limit),
        ViewSource::ImportErrors => Ok(Vec::new()),
    }
}

fn query_track_count_library(conn: &Connection, filter: &str) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 0{}",
        filter_clause(has_filter, 1)
    );
    if has_filter {
        let like = format!("%{}%", filter.trim());
        conn.query_row(&sql, rusqlite::params![like], |r| r.get(0))
    } else {
        conn.query_row(&sql, [], |r| r.get(0))
    }
}

fn query_track_count_missing(conn: &Connection, filter: &str) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 1{}",
        filter_clause(has_filter, 1)
    );
    if has_filter {
        let like = format!("%{}%", filter.trim());
        conn.query_row(&sql, rusqlite::params![like], |r| r.get(0))
    } else {
        conn.query_row(&sql, [], |r| r.get(0))
    }
}

fn query_track_count_playlist(
    conn: &Connection,
    playlist_id: i64,
    filter: &str,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT count(*) FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?1 AND tracks.missing = 0{}",
        filter_clause(has_filter, 2)
    );
    if has_filter {
        let like = format!("%{}%", filter.trim());
        conn.query_row(&sql, rusqlite::params![playlist_id, like], |r| r.get(0))
    } else {
        conn.query_row(&sql, rusqlite::params![playlist_id], |r| r.get(0))
    }
}

fn query_track_count_smart(
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
    let has_filter = !filter.trim().is_empty();
    let (rules_frag, mut params) = match playlists::smart_rules_to_sql(&smart.rules_json) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(%error, smart_id, "invalid smart playlist rules; returning 0");
            return Ok(0);
        }
    };
    let next_idx = params.len() as u8 + 1;
    let mut sql = format!("SELECT count(*) FROM tracks WHERE missing = 0 AND ({rules_frag})");
    if has_filter {
        sql.push_str(&filter_clause(true, next_idx));
        params.push(rusqlite::types::Value::Text(format!("%{}%", filter.trim())));
    }
    let raw: i64 = conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
        r.get(0)
    })?;
    Ok(match smart.limit_count {
        Some(n) => raw.min(n),
        None => raw,
    })
}

/// Counts rows matching `(source, filter)` — see the module doc for how
/// each source defines "matching". `queue_ids` is only read for
/// `ViewSource::Queue` (its count is simply `queue_ids.len()`).
pub fn query_track_count(
    conn: &Connection,
    source: &ViewSource,
    filter: &str,
    queue_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    match source {
        ViewSource::Library => query_track_count_library(conn, filter),
        ViewSource::Missing => query_track_count_missing(conn, filter),
        ViewSource::Playlist(id) => query_track_count_playlist(conn, *id, filter),
        ViewSource::Smart(id) => query_track_count_smart(conn, *id, filter),
        ViewSource::Queue => Ok(queue_ids.len() as i64),
        ViewSource::ImportErrors => Ok(0),
    }
}

fn query_track_ids_playlist(
    conn: &Connection,
    playlist_id: i64,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    // Deliberately always `pt.position` order, never the caller's current
    // column sort (see the module doc's `Playlist(id)` section): "play this
    // playlist" always follows playlist order, even if the visible window
    // is temporarily sorted by a clicked column header.
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT tracks.id FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?1 AND tracks.missing = 0{} \
         ORDER BY pt.position ASC LIMIT {QUEUE_LIMIT}",
        filter_clause(has_filter, 2)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = if has_filter {
        let like = format!("%{}%", filter.trim());
        stmt.query_map(rusqlite::params![playlist_id, like], row_to_id)?
    } else {
        stmt.query_map(rusqlite::params![playlist_id], row_to_id)?
    };
    rows.collect()
}

fn query_track_ids_smart(
    conn: &Connection,
    smart_id: i64,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let Some(smart) = load_smart_playlist(conn, smart_id)? else {
        tracing::warn!(
            smart_id,
            "smart playlist not found for ids query; returning empty"
        );
        return Ok(Vec::new());
    };
    let has_filter = !filter.trim().is_empty();
    let (order_expr, dir) = order_expr_and_dir(&smart.sort_field, &smart.sort_dir);
    let (rules_frag, mut params) = match playlists::smart_rules_to_sql(&smart.rules_json) {
        Ok(v) => v,
        Err(error) => {
            tracing::error!(%error, smart_id, "invalid smart playlist rules; returning empty ids");
            return Ok(Vec::new());
        }
    };
    let next_idx = params.len() as u8 + 1;
    let mut sql = format!("SELECT id FROM tracks WHERE missing = 0 AND ({rules_frag})");
    if has_filter {
        sql.push_str(&filter_clause(true, next_idx));
        params.push(rusqlite::types::Value::Text(format!("%{}%", filter.trim())));
    }
    // The smart playlist's own limit bounds the queue too (capped by
    // `QUEUE_LIMIT` for defense in depth, same as every other source's ids
    // query); a literal, not a bound parameter — both operands are
    // Rust-side i64s, never caller-supplied text.
    let effective_limit = smart.limit_count.unwrap_or(QUEUE_LIMIT).min(QUEUE_LIMIT);
    sql.push_str(&format!(
        " ORDER BY {order_expr} {dir} LIMIT {effective_limit}"
    ));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_id)?;
    rows.collect()
}

/// Returns every track id matching `(source, sort_field, sort_dir, filter)`,
/// in the order that source's "play this whole view" queue should use,
/// capped at `QUEUE_LIMIT`. This is the queue seam (Stage 2 Task 4; made
/// source-aware in Stage 3 Task 3): activating a row queues "the whole
/// current view" by resolving it to this id list rather than the
/// `MAX_WINDOW_LIMIT`-capped `query_track_window` (which is sized for one
/// `ColumnView` page, not a playback queue). See the module doc for each
/// source's ordering. The `Vec` alone can't tell the caller whether it was
/// truncated by the cap — compare its length with `is_queue_capped` and log
/// a warning if so.
pub fn query_track_ids(
    conn: &Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    queue_ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    match source {
        ViewSource::Library => {
            let has_filter = !filter.trim().is_empty();
            let sql = build_track_ids_query(sort_field, sort_dir, has_filter);
            let mut stmt = conn.prepare(&sql)?;
            let like = format!("%{}%", filter.trim());
            let rows = if has_filter {
                stmt.query_map(rusqlite::params![like], row_to_id)?
            } else {
                stmt.query_map([], row_to_id)?
            };
            rows.collect()
        }
        ViewSource::Missing => {
            let has_filter = !filter.trim().is_empty();
            let sql = build_track_ids_query_base(1, sort_field, sort_dir, has_filter);
            let mut stmt = conn.prepare(&sql)?;
            let like = format!("%{}%", filter.trim());
            let rows = if has_filter {
                stmt.query_map(rusqlite::params![like], row_to_id)?
            } else {
                stmt.query_map([], row_to_id)?
            };
            rows.collect()
        }
        ViewSource::Playlist(id) => query_track_ids_playlist(conn, *id, filter),
        ViewSource::Smart(id) => query_track_ids_smart(conn, *id, filter),
        ViewSource::Queue => Ok(queue_ids.to_vec()),
        ViewSource::ImportErrors => Ok(Vec::new()),
    }
}

/// Whether a `query_track_ids` result of this length was (probably) capped
/// by `QUEUE_LIMIT`. Treats the exact-boundary case (`len == QUEUE_LIMIT`)
/// as capped: the alternative — a library with *exactly* `QUEUE_LIMIT`
/// matching tracks — is indistinguishable from a truncated one without a
/// second `COUNT(*)` query, and logging one harmless extra warning on that
/// rare exact-fit case is a better tradeoff than silently missing a real
/// truncation.
pub fn is_queue_capped(len: usize) -> bool {
    len as i64 >= QUEUE_LIMIT
}

/// The subset of a track's columns the player bar and queue playback path
/// need: the file to hand `Player::play`, the title/artist to show, and the
/// duration play-tracking's 50%-listened check requires
/// (`library::stats::should_count_play`). Deliberately narrower than the
/// full `Track` (no rating/play_count/etc. — the bar doesn't display those),
/// avoiding the cost of loading and holding the columns nothing here reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSummary {
    pub path: String,
    pub title: String,
    pub artist: String,
    /// Stage 2 Task 6 (MPRIS): feeds `Metadata`'s `xesam:album`. Not used by
    /// the player bar (which only shows title/artist), so it went unused
    /// here until MPRIS needed it.
    pub album: String,
    pub duration_ms: i64,
}

/// Resolves one track id to its `TrackSummary` — the queue's per-track
/// playback step (`play_track_id` in `ui::player_controller`) calls this for
/// every auto-advance/next/previous, and Stage 2 Task 5's skip-on-missing-
/// file logic is documented to reuse it too. `Ok(None)` for an id with no
/// matching row (e.g. deleted between queueing and playback) — never an
/// error; the caller decides how to degrade (skip/stop), matching every
/// other fallible path in this module.
pub fn query_track_summary(
    conn: &Connection,
    id: i64,
) -> Result<Option<TrackSummary>, rusqlite::Error> {
    conn.query_row(
        "SELECT path, title, artist, album, duration_ms FROM tracks WHERE id = ?1",
        rusqlite::params![id],
        |r| {
            Ok(TrackSummary {
                path: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                album: r.get(3)?,
                duration_ms: r.get(4)?,
            })
        },
    )
    .optional()
}

/// Marks track `track_id` as missing (Stage 2 Task 5: a physically deleted
/// file must never crash or dead-end the app — this is the DB-side half of
/// that guarantee). Every windowed/count/id query for `ViewSource::Library`/
/// `Playlist`/`Smart` already filters `missing = 0`, so the row disappears
/// from those views and from a freshly-seeded queue on the very next
/// reload, without deleting the row itself — ratings/play history/etc. are
/// preserved, and the row resurfaces in `ViewSource::Missing` (Stage 3 Task
/// 3) instead of vanishing outright.
pub fn mark_track_missing(conn: &Connection, track_id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE tracks SET missing = 1 WHERE id = ?1",
        rusqlite::params![track_id],
    )?;
    Ok(())
}

/// Aggregates library-wide stats over all non-missing tracks. Powers the
/// status line (`ui::status_bar`). `track_count`/`total_duration_ms` always
/// describe the *whole* library, regardless of `filter` — only `filtered_
/// count` reacts to it, becoming `Some(query_track_count(conn, filter))` when
/// `filter` is non-empty (trimmed) and `None` otherwise, so a status line
/// with no active search reads exactly as it did before `filter` existed.
/// Deliberately library-only, unaffected by `ViewSource` (Stage 3 Task 3):
/// the status line keeps showing library-wide totals regardless of which
/// source the track list is currently displaying — see `ui::status_bar`'s
/// `refresh_for_source_count` for the simpler "{n} tracks" line shown
/// alongside it for non-Library sources.
pub fn query_library_stats(
    conn: &Connection,
    filter: &str,
) -> Result<LibraryStats, rusqlite::Error> {
    let (track_count, total_duration_ms) = conn.query_row(
        "SELECT count(*), coalesce(sum(duration_ms),0) FROM tracks WHERE missing = 0",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let filtered_count = if filter.trim().is_empty() {
        None
    } else {
        Some(query_track_count_library(conn, filter)?)
    };
    Ok(LibraryStats {
        track_count,
        total_duration_ms,
        filtered_count,
    })
}

/// Bare count of rows in `import_errors` (the last scan's import failures) —
/// see the module doc's `ImportErrors` section for why this is the only
/// piece of that source this task builds. Not called from production code
/// yet: it's built ahead of a future sidebar badge (Task 4/8's job to wire
/// up), so it's exercised only by this module's own tests for now.
#[allow(dead_code)]
pub fn query_import_error_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_titled_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn query_builder_whitelists_and_sorts() {
        let q = build_track_query("artist", "asc", false);
        assert!(q.contains("ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, track_no ASC"));
        assert!(q.contains("WHERE missing = 0"));
        assert!(!q.contains("?3")); // no filter placeholder without a filter
    }

    #[test]
    fn query_builder_rejects_unknown_column_with_title_fallback() {
        let q = build_track_query("path; DROP TABLE tracks", "desc", true);
        assert!(q.contains("ORDER BY title COLLATE NOCASE DESC"));
        assert!(q.contains("(title LIKE ?3 OR artist LIKE ?3 OR album LIKE ?3 OR genre LIKE ?3)"));
    }

    #[test]
    fn window_returns_filtered_sorted_tracks() {
        let mut conn = seeded_titled_conn();
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        assert_eq!(rows[0].title, "Alpha");
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Library,
            "title",
            "asc",
            "zu",
            0,
            10,
            &[],
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Zulu");
    }

    #[test]
    fn count_is_zero_for_empty_db() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
            0
        );
    }

    #[test]
    fn count_matches_inserted_rows() {
        let conn = seeded_titled_conn();
        assert_eq!(
            query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
            3
        );
    }

    #[test]
    fn count_applies_filter() {
        let conn = seeded_titled_conn();
        assert_eq!(
            query_track_count(&conn, &ViewSource::Library, "zu", &[]).unwrap(),
            1
        );
    }

    #[test]
    fn count_excludes_missing_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing) \
             VALUES ('/x/a.flac', 'A', '', 0, 1)",
            [],
        )
        .unwrap();
        assert_eq!(
            query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
            0
        );
    }

    #[test]
    fn track_ids_follow_whitelist_sort_order() {
        let conn = seeded_titled_conn();
        let ids = query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap();
        assert_eq!(ids.len(), 3);

        // "Alpha" < "Mid" < "Zulu" by title (COLLATE NOCASE) — assert the
        // exact id order directly against the same ORDER BY expression
        // `SORT_WHITELIST` uses for "title".
        let by_title: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM tracks ORDER BY title COLLATE NOCASE ASC")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(ids, by_title);
    }

    #[test]
    fn track_ids_apply_filter() {
        let conn = seeded_titled_conn();
        let ids = query_track_ids(&conn, &ViewSource::Library, "title", "asc", "zu", &[]).unwrap();
        assert_eq!(ids.len(), 1);

        let expected_id: i64 = conn
            .query_row("SELECT id FROM tracks WHERE title = 'Zulu'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ids[0], expected_id);
    }

    #[test]
    fn track_ids_excludes_missing_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing) \
             VALUES ('/x/a.flac', 'A', '', 0, 1)",
            [],
        )
        .unwrap();
        assert_eq!(
            query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap(),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn track_ids_query_is_capped_at_queue_limit() {
        // Inserting QUEUE_LIMIT+1 rows just to prove the cap would make this
        // test slow and heavy for no extra confidence — the cap is a single
        // hardcoded `LIMIT` in the generated SQL, so asserting it's present
        // with the right value in `build_track_ids_query`'s output is the
        // pragmatic, fast way to pin the behavior. The boundary logic for
        // *detecting* a truncated result (`is_queue_capped`) is exercised
        // directly below instead of via a 10,001-row fixture.
        let sql = build_track_ids_query("title", "asc", false);
        assert!(sql.contains(&format!("LIMIT {QUEUE_LIMIT}")));
    }

    #[test]
    fn is_queue_capped_detects_the_boundary() {
        assert!(!is_queue_capped((QUEUE_LIMIT - 1) as usize));
        assert!(is_queue_capped(QUEUE_LIMIT as usize));
    }

    #[test]
    fn track_summary_found_returns_expected_fields() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, duration_ms, added_at) \
             VALUES ('/x/a.flac', 'A Title', 'An Artist', 'An Album', 123456, 0)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks", [], |r| r.get(0))
            .unwrap();

        let summary = query_track_summary(&conn, id).unwrap().unwrap();
        assert_eq!(summary.path, "/x/a.flac");
        assert_eq!(summary.title, "A Title");
        assert_eq!(summary.artist, "An Artist");
        assert_eq!(summary.album, "An Album");
        assert_eq!(summary.duration_ms, 123456);
    }

    #[test]
    fn track_summary_not_found_returns_none() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert!(query_track_summary(&conn, 999).unwrap().is_none());
    }

    #[test]
    fn mark_track_missing_sets_the_flag() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/x/a.flac', 'A', '', 0)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks", [], |r| r.get(0))
            .unwrap();

        mark_track_missing(&conn, id).unwrap();

        let missing: i64 = conn
            .query_row(
                "SELECT missing FROM tracks WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(missing, 1);
    }

    #[test]
    fn mark_track_missing_excludes_from_count_and_ids() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/x/a.flac', 'A', '', 0)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks", [], |r| r.get(0))
            .unwrap();

        assert_eq!(
            query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
            1
        );
        assert_eq!(
            query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap(),
            vec![id]
        );

        mark_track_missing(&conn, id).unwrap();

        assert_eq!(
            query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
            0
        );
        assert_eq!(
            query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap(),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn library_stats_without_filter_has_none_filtered_count_and_full_totals() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, 1000, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }

        let stats = query_library_stats(&conn, "").unwrap();
        assert_eq!(stats.track_count, 3);
        assert_eq!(stats.total_duration_ms, 3000);
        assert_eq!(stats.filtered_count, None);
    }

    #[test]
    fn library_stats_with_filter_matches_query_track_count_and_keeps_totals_unfiltered() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, 1000, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }

        let stats = query_library_stats(&conn, "zu").unwrap();
        // Totals stay unfiltered even though a filter is active.
        assert_eq!(stats.track_count, 3);
        assert_eq!(stats.total_duration_ms, 3000);
        assert_eq!(
            stats.filtered_count,
            Some(query_track_count(&conn, &ViewSource::Library, "zu", &[]).unwrap())
        );
        assert_eq!(stats.filtered_count, Some(1));
    }

    #[test]
    fn library_stats_missing_rows_excluded_from_both_counts() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, duration_ms, added_at, missing) \
             VALUES ('/x/a.flac', 'A', '', 1000, 0, 1)",
            [],
        )
        .unwrap();

        let unfiltered = query_library_stats(&conn, "").unwrap();
        assert_eq!(unfiltered.track_count, 0);
        assert_eq!(unfiltered.total_duration_ms, 0);
        assert_eq!(unfiltered.filtered_count, None);

        let filtered = query_library_stats(&conn, "A").unwrap();
        assert_eq!(filtered.track_count, 0);
        assert_eq!(filtered.filtered_count, Some(0));
    }

    #[test]
    fn window_limit_is_clamped() {
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for t in ["Alpha", "Beta", "Gamma"] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, '', 0)",
                rusqlite::params![format!("/x/{t}.flac"), t],
            )
            .unwrap();
        }

        // SQLite treats a negative LIMIT as "unlimited"; clamped to 0, a
        // negative caller-supplied limit must return no rows.
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            0,
            -1,
            &[],
        )
        .unwrap();
        assert_eq!(rows.len(), 0);

        // A limit far above MAX_WINDOW_LIMIT is clamped down to the cap,
        // which still comfortably covers this small fixture set, so all
        // rows are returned rather than the query becoming unbounded.
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            0,
            10_000,
            &[],
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
    }

    // -- Missing source -------------------------------------------------

    fn seeded_conn_with_missing() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a, missing) in [("Zulu", "AAA", 1), ("Alpha", "BBB", 0), ("Mid", "CCC", 1)] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at, missing) \
                 VALUES (?1, ?2, ?3, 0, ?4)",
                rusqlite::params![format!("/x/{t}.flac"), t, a, missing],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn missing_window_and_count_only_include_missing_rows() {
        let mut conn = seeded_conn_with_missing();
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Missing,
            "title",
            "asc",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Mid");
        assert_eq!(rows[1].title, "Zulu");

        assert_eq!(
            query_track_count(&conn, &ViewSource::Missing, "", &[]).unwrap(),
            2
        );
    }

    #[test]
    fn missing_ids_are_sorted_like_library() {
        let conn = seeded_conn_with_missing();
        let ids = query_track_ids(&conn, &ViewSource::Missing, "title", "asc", "", &[]).unwrap();
        let by_title: Vec<i64> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM tracks WHERE missing = 1 ORDER BY title COLLATE NOCASE ASC",
                )
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(ids, by_title);
    }

    // -- Playlist source -------------------------------------------------

    fn seeded_conn_with_tracks(count: i64) -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for i in 1..=count {
            conn.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, '', 0)",
                rusqlite::params![i, format!("/x/{i}.flac"), format!("Track {i}")],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn playlist_window_follows_position_order_by_default() {
        let mut conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 2]).unwrap();

        let rows = query_track_window(
            &mut conn,
            &ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn playlist_window_shows_duplicates_as_separate_rows() {
        let mut conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 1]).unwrap();

        let rows = query_track_window(
            &mut conn,
            &ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2, 1]);
        assert_eq!(
            query_track_count(&conn, &ViewSource::Playlist(playlist_id), "", &[]).unwrap(),
            3
        );
    }

    #[test]
    fn playlist_window_honors_an_explicit_column_sort_override() {
        let mut conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 2]).unwrap();

        // A column header click (e.g. "title") temporarily overrides
        // playlist order.
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Playlist(playlist_id),
            "title",
            "asc",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        let titles: Vec<&str> = rows.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["Track 1", "Track 2", "Track 3"]);
    }

    #[test]
    fn playlist_window_excludes_missing_tracks() {
        let mut conn = seeded_conn_with_tracks(3);
        conn.execute("UPDATE tracks SET missing = 1 WHERE id = 2", [])
            .unwrap();
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();

        let rows = query_track_window(
            &mut conn,
            &ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 3]);
        assert_eq!(
            query_track_count(&conn, &ViewSource::Playlist(playlist_id), "", &[]).unwrap(),
            2
        );
    }

    #[test]
    fn playlist_ids_always_follow_position_order_ignoring_sort_param() {
        let mut conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 2]).unwrap();

        // Even asking for "title" order, activation ids stay position order.
        let ids = query_track_ids(
            &conn,
            &ViewSource::Playlist(playlist_id),
            "title",
            "asc",
            "",
            &[],
        )
        .unwrap();
        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn playlist_count_applies_filter() {
        let mut conn = seeded_conn_with_tracks(3);
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();

        assert_eq!(
            query_track_count(&conn, &ViewSource::Playlist(playlist_id), "Track 2", &[]).unwrap(),
            1
        );
    }

    // -- Smart source -------------------------------------------------

    fn insert_smart_playlist(
        conn: &Connection,
        rules_json: &str,
        sort_field: &str,
        sort_dir: &str,
        limit_count: Option<i64>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO smart_playlists (name, rules_json, sort_field, sort_dir, limit_count) \
             VALUES ('S', ?1, ?2, ?3, ?4)",
            rusqlite::params![rules_json, sort_field, sort_dir, limit_count],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn smart_window_applies_rules_and_own_sort() {
        let conn = seeded_conn_with_tracks(5);
        conn.execute("UPDATE tracks SET rating = 4 WHERE id IN (2, 4)", [])
            .unwrap();
        let smart_id = insert_smart_playlist(
            &conn,
            r#"[{"field":"rating","op":">=","value":4}]"#,
            "title",
            "asc",
            None,
        );

        let mut conn = conn;
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Smart(smart_id),
            "ignored",
            "ignored",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![2, 4]);
        assert_eq!(
            query_track_count(&conn, &ViewSource::Smart(smart_id), "", &[]).unwrap(),
            2
        );
    }

    #[test]
    fn smart_window_applies_live_search_filter_too() {
        let conn = seeded_conn_with_tracks(5);
        conn.execute("UPDATE tracks SET rating = 4", []).unwrap();
        let smart_id = insert_smart_playlist(
            &conn,
            r#"[{"field":"rating","op":">=","value":4}]"#,
            "title",
            "asc",
            None,
        );

        let mut conn = conn;
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Smart(smart_id),
            "ignored",
            "ignored",
            "Track 3",
            0,
            10,
            &[],
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Track 3");
    }

    #[test]
    fn smart_window_offset_within_limit_returns_the_edge_case_slice() {
        // Regression for the exact edge case the task calls out: a smart
        // playlist limited to 50 rows, windowed at offset 40/limit 20, must
        // return exactly 10 rows (positions 40..49), never rows beyond the
        // smart playlist's own limit.
        let conn = seeded_conn_with_tracks(100);
        let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(50));

        let mut conn = conn;
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Smart(smart_id),
            "ignored",
            "ignored",
            "",
            40,
            20,
            &[],
        )
        .unwrap();
        assert_eq!(rows.len(), 10);

        // The full 50-row set, in title order, has "Track 41".."Track 49"
        // (alphabetically) at the tail — assert against the same fallback
        // by re-deriving the first 50 titles directly.
        let mut all_titles: Vec<String> = (1..=100).map(|i| format!("Track {i}")).collect();
        all_titles.sort();
        let expected = &all_titles[40..50];
        let got: Vec<String> = rows.iter().map(|t| t.title.clone()).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn smart_count_is_capped_by_limit_count() {
        let conn = seeded_conn_with_tracks(100);
        let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(50));

        assert_eq!(
            query_track_count(&conn, &ViewSource::Smart(smart_id), "", &[]).unwrap(),
            50
        );
    }

    #[test]
    fn smart_ids_are_capped_by_limit_count() {
        let conn = seeded_conn_with_tracks(100);
        let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(50));

        let ids = query_track_ids(
            &conn,
            &ViewSource::Smart(smart_id),
            "ignored",
            "ignored",
            "",
            &[],
        )
        .unwrap();
        assert_eq!(ids.len(), 50);
    }

    #[test]
    fn smart_window_falls_back_to_title_on_tampered_sort_field() {
        // Simulates a hand-edited (DB-tampered) smart_playlists row whose
        // sort_field isn't a whitelisted value — `order_expr_and_dir` must
        // fall back to title order rather than erroring or (worse)
        // interpolating the value into SQL.
        let conn = seeded_conn_with_tracks(3);
        let smart_id =
            insert_smart_playlist(&conn, "[]", "sneaky; DROP TABLE tracks--", "asc", None);

        let mut conn = conn;
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Smart(smart_id),
            "ignored",
            "ignored",
            "",
            0,
            10,
            &[],
        )
        .unwrap();
        let titles: Vec<&str> = rows.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["Track 1", "Track 2", "Track 3"]);
    }

    #[test]
    fn smart_source_not_found_degrades_to_empty() {
        let conn = seeded_conn_with_tracks(3);
        let mut conn = conn;
        assert!(
            query_track_window(&mut conn, &ViewSource::Smart(999), "x", "x", "", 0, 10, &[])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            query_track_count(&conn, &ViewSource::Smart(999), "", &[]).unwrap(),
            0
        );
        assert!(
            query_track_ids(&conn, &ViewSource::Smart(999), "x", "x", "", &[])
                .unwrap()
                .is_empty()
        );
    }

    // -- Queue source -------------------------------------------------

    #[test]
    fn queue_window_follows_the_ids_order_not_id_order() {
        let conn = seeded_conn_with_tracks(3);
        let mut conn = conn;
        let queue_ids = vec![3, 1, 2];
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Queue,
            "ignored",
            "ignored",
            "ignored",
            0,
            10,
            &queue_ids,
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn queue_window_skips_ids_with_no_matching_row() {
        let conn = seeded_conn_with_tracks(3);
        let mut conn = conn;
        let queue_ids = vec![3, 999, 1];
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Queue,
            "ignored",
            "ignored",
            "",
            0,
            10,
            &queue_ids,
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![3, 1]);
    }

    #[test]
    fn queue_window_slices_by_offset_and_limit_then_reorders() {
        let conn = seeded_conn_with_tracks(5);
        let mut conn = conn;
        let queue_ids = vec![5, 4, 3, 2, 1];
        let rows = query_track_window(
            &mut conn,
            &ViewSource::Queue,
            "ignored",
            "ignored",
            "",
            2,
            2,
            &queue_ids,
        )
        .unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![3, 2]);
    }

    #[test]
    fn queue_count_is_the_ids_length_regardless_of_filter() {
        let queue_ids = vec![5, 4, 3];
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            query_track_count(&conn, &ViewSource::Queue, "anything", &queue_ids).unwrap(),
            3
        );
    }

    #[test]
    fn queue_ids_are_returned_verbatim() {
        let queue_ids = vec![5, 4, 3];
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(
            query_track_ids(&conn, &ViewSource::Queue, "x", "x", "", &queue_ids).unwrap(),
            queue_ids
        );
    }

    // -- ImportErrors source -------------------------------------------------

    #[test]
    fn import_errors_source_is_always_empty_for_now() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO import_errors (path, reason, occurred_at) VALUES ('/x/a.flac', 'bad tag', 0)",
            [],
        )
        .unwrap();

        let mut conn = conn;
        assert!(query_track_window(
            &mut conn,
            &ViewSource::ImportErrors,
            "x",
            "x",
            "",
            0,
            10,
            &[]
        )
        .unwrap()
        .is_empty());
        assert_eq!(
            query_track_count(&conn, &ViewSource::ImportErrors, "", &[]).unwrap(),
            0
        );
        assert!(
            query_track_ids(&conn, &ViewSource::ImportErrors, "x", "x", "", &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn query_import_error_count_counts_the_table() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(query_import_error_count(&conn).unwrap(), 0);

        conn.execute(
            "INSERT INTO import_errors (path, reason, occurred_at) VALUES ('/x/a.flac', 'bad tag', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO import_errors (path, reason, occurred_at) VALUES ('/x/b.flac', 'bad tag', 0)",
            [],
        )
        .unwrap();
        assert_eq!(query_import_error_count(&conn).unwrap(), 2);
    }
}
