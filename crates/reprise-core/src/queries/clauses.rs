//! Shared SQL fragment builders and row-mapping functions used by every
//! `ViewSource`'s query module: the sort whitelist, the LIKE-filter clause,
//! the parameterized library/missing query builders, and the `rusqlite::Row`
//! -> `Track`/`id` mappers. Split out of the former single-file `queries.rs`
//! (Refactoring & Extensibility Task 1) — a pure move, no behavior change.

use crate::library::playlists;
use crate::models::Track;

use super::queue::QUEUE_LIMIT;
use super::{browse::browse_clause, BrowseFilter};

/// `"playlist_order"` is a *sentinel* entry, not a real column: it only
/// resolves to valid SQL (`pt.position`) inside a query that actually joins
/// `playlist_tracks AS pt` — see the module doc's `Playlist(id)` section for
/// why that's safe (only `ViewSource::Playlist` queries ever pass it).
const SORT_WHITELIST: [(&str, &str); 9] = [
    ("title", "title COLLATE NOCASE"),
    (
        "artist",
        "artist COLLATE NOCASE, album COLLATE NOCASE, track_no",
    ),
    ("album", "album COLLATE NOCASE, track_no"),
    ("track_no", "track_no"),
    ("genre", "genre COLLATE NOCASE, artist COLLATE NOCASE"),
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
pub(super) fn filter_clause(has_filter: bool, param_index: u8) -> String {
    if has_filter {
        format!(
            " AND (title LIKE ?{param_index} ESCAPE '\\' OR artist LIKE ?{param_index} ESCAPE '\\' \
             OR album LIKE ?{param_index} ESCAPE '\\' OR genre LIKE ?{param_index} ESCAPE '\\')"
        )
    } else {
        String::new()
    }
}

/// Builds the bound `%…%` LIKE pattern for a trimmed filter value — always
/// through `library::playlists::escape_like` (Stage-3 close-out finding:
/// this used to be a bare `format!("%{}%", filter.trim())` at every call
/// site below, so a literal `%`/`_` typed into the search box acted as a
/// live wildcard instead of matching itself, inconsistent with the smart-
/// rule `contains` operator's own escaping). Every `filter_clause` LIKE
/// site in this module builds its bound value through this one function so
/// the two can never drift apart again.
pub(super) fn like_pattern(filter_trimmed: &str) -> String {
    format!("%{}%", playlists::escape_like(filter_trimmed))
}

/// Resolves `sort_field`/`sort_dir` to a whitelisted `ORDER BY` expression
/// and direction keyword. Shared by every source's window/ids query builder
/// so they can never disagree about what a given sort field/direction
/// means. `sort_field` is only ever used as a lookup key into `SORT_
/// WHITELIST` — never interpolated into SQL directly — so caller input
/// cannot inject arbitrary SQL. Unknown sort fields silently fall back to
/// sorting by title (this is also what makes a DB-tampered smart-playlist
/// `sort_field` degrade safely — see the module doc's `Smart(id)` section).
pub(super) fn order_expr_and_dir(sort_field: &str, sort_dir: &str) -> (&'static str, &'static str) {
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
pub(super) fn build_track_query_base(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
) -> String {
    build_track_query_base_browsed(
        missing_flag,
        sort_field,
        sort_dir,
        has_filter,
        &BrowseFilter::default(),
    )
}

pub(super) fn build_track_query_base_browsed(
    missing_flag: u8,
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 3);
    let browse_first_param = if has_filter { 4 } else { 3 };
    let (browse_clause, _) = browse_clause(browse, browse_first_param);
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing, file_size, device, inode \
         FROM tracks WHERE missing = {missing_flag}{filter_clause}{browse_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

/// Builds the parameterized SELECT for a library window (`missing = 0`).
/// See `build_track_query_base`'s doc comment for the whitelist guarantee.
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    build_track_query_base(0, sort_field, sort_dir, has_filter)
}

pub(super) fn build_track_query_browsed(
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
) -> String {
    build_track_query_base_browsed(0, sort_field, sort_dir, has_filter, browse)
}

/// Builds the parameterized `SELECT id` for the queue seam
/// (`query_track_ids`, library/missing shape): every id matching
/// `(missing_flag, sort_field, sort_dir, filter)`, capped at `QUEUE_LIMIT` —
/// a literal, not a bound parameter, since it's a fixed Rust-side constant
/// rather than caller input (nothing to inject). Shares `order_expr_and_dir`/
/// `filter_clause` with `build_track_query_base` so the queue's ordering can
/// never drift from the track list's.
pub(super) fn build_track_ids_query_base(
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

pub(super) fn build_track_ids_query_browsed(
    sort_field: &str,
    sort_dir: &str,
    has_filter: bool,
    browse: &BrowseFilter,
) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    let browse_first_param = if has_filter { 2 } else { 1 };
    let (browse_clause, _) = browse_clause(browse, browse_first_param);
    format!(
        "SELECT id FROM tracks WHERE missing = 0{filter_clause}{browse_clause} \
         ORDER BY {order_expr} {dir} LIMIT {QUEUE_LIMIT}"
    )
}

pub(super) fn row_to_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
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
        playlist_position: None,
    })
}

/// Same 20-column shape as `row_to_track`, plus a trailing `pt.position`
/// column (index 20) — used only by `query_track_window_playlist`, the one
/// query that actually joins `playlist_tracks AS pt`. See `Track::
/// playlist_position`'s doc comment for why this is the sole populating
/// call site.
pub(super) fn row_to_playlist_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    let mut track = row_to_track(r)?;
    track.playlist_position = Some(r.get(20)?);
    Ok(track)
}

pub(super) fn row_to_id(r: &rusqlite::Row) -> rusqlite::Result<i64> {
    r.get(0)
}
