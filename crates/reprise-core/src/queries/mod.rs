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
//!   Every row also carries its true `pt.position` in `Track::playlist_
//!   position` (via `row_to_playlist_track`) regardless of `ORDER BY` —
//!   the fix for "remove from playlist" targeting the wrong row once a
//!   column sort or live search filter makes on-screen order diverge from
//!   `pt.position`; see that field's doc comment and `ui::track_actions::
//!   remove_selected_from_playlist`.
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

use crate::models::Track;
use crate::view_source::ViewSource;
use rusqlite::Connection;

mod clauses;
mod library;
mod maintenance;
mod playlist;
mod queue;
mod smart;

pub use clauses::build_track_ids_query;
// `build_track_query`'s only current caller is this module's own test suite
// (`tests::query_builder_whitelists_and_sorts` et al.) — re-exported `pub`
// regardless, to keep `crate::queries::build_track_query` resolving exactly
// as it did before this split, matching this file's own non-test build
// where the re-export would otherwise look unused.
#[allow(unused_imports)]
pub use clauses::build_track_query;
pub use maintenance::{
    delete_import_error, mark_track_missing, query_import_error_count, query_import_errors,
    query_track_summary, remove_missing_tracks, track_id_for_path,
};
// `remove_missing_track`'s only external caller (beyond `remove_missing_
// tracks`'s own internal use) is this module's test suite — same reasoning
// as `build_track_query` above.
#[allow(unused_imports)]
pub use maintenance::remove_missing_track;
pub use playlist::query_playlist_tracks_full;
pub use queue::{is_queue_capped, QUEUE_LIMIT};

use clauses::{build_track_ids_query_base, like_pattern, row_to_id};

/// Global constraint: window queries never return more rows than this in one
/// page, regardless of what the caller requests. SQLite treats a negative
/// `LIMIT` as "unlimited", so this also protects against a bad UI-side page
/// size from turning into a full-table scan. Limits capped.
const MAX_WINDOW_LIMIT: i64 = 500;

#[derive(Debug)]
pub struct LibraryStats {
    pub track_count: i64,
    pub total_duration_ms: i64,
    /// `Some(n)` while a search filter is active (status line shows "N of M
    /// tracks"), `None` when it isn't. See `query_library_stats`.
    pub filtered_count: Option<i64>,
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
            library::query_track_window_library(conn, sort_field, sort_dir, filter, offset, limit)
        }
        ViewSource::Missing => {
            library::query_track_window_missing(conn, sort_field, sort_dir, filter, offset, limit)
        }
        ViewSource::Playlist(id) => playlist::query_track_window_playlist(
            conn, *id, sort_field, sort_dir, filter, offset, limit,
        ),
        ViewSource::Smart(id) => smart::query_track_window_smart(conn, *id, filter, offset, limit),
        ViewSource::Queue => queue::query_track_window_queue(conn, queue_ids, offset, limit),
        ViewSource::ImportErrors => Ok(Vec::new()),
    }
}

/// Counts rows matching `(source, filter)` — see the module doc for how
/// each source defines "matching". `queue_ids` is only read for
/// `ViewSource::Queue`.
pub fn query_track_count(
    conn: &Connection,
    source: &ViewSource,
    filter: &str,
    queue_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    match source {
        ViewSource::Library => library::query_track_count_library(conn, filter),
        ViewSource::Missing => library::query_track_count_missing(conn, filter),
        ViewSource::Playlist(id) => playlist::query_track_count_playlist(conn, *id, filter),
        ViewSource::Smart(id) => smart::query_track_count_smart(conn, *id, filter),
        // Stage-3 close-out fix: this used to trust `queue_ids.len()`
        // verbatim, on the documented assumption that nothing hard-deletes a
        // `tracks` row. That assumption no longer holds (`remove_missing_
        // track`/`remove_missing_tracks` do exactly that) — the queue itself
        // is purged in lockstep by `ui::player_controller::PlayerController::
        // purge_queue_ids` whenever a hard-delete happens through the app's
        // own UI, but counting matched rows here (rather than trusting the
        // caller's `queue_ids` slice) is a second, independent guarantee
        // that a `ColumnView` can never be told there are more rows than
        // `query_track_window_queue` will actually render, even if some
        // future caller forgets to purge the queue after a hard-delete.
        ViewSource::Queue => queue::query_track_count_queue(conn, queue_ids),
        ViewSource::ImportErrors => Ok(0),
    }
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
            let like = like_pattern(filter.trim());
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
            let like = like_pattern(filter.trim());
            let rows = if has_filter {
                stmt.query_map(rusqlite::params![like], row_to_id)?
            } else {
                stmt.query_map([], row_to_id)?
            };
            rows.collect()
        }
        ViewSource::Playlist(id) => playlist::query_track_ids_playlist(conn, *id, filter),
        ViewSource::Smart(id) => smart::query_track_ids_smart(conn, *id, filter),
        ViewSource::Queue => Ok(queue_ids.to_vec()),
        ViewSource::ImportErrors => Ok(Vec::new()),
    }
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
        Some(library::query_track_count_library(conn, filter)?)
    };
    Ok(LibraryStats {
        track_count,
        total_duration_ms,
        filtered_count,
    })
}

/// One `import_errors` row, as rendered by the ImportErrors source (Stage 3
/// Task 8: this task builds the real backing query/columns the module doc's
/// `ImportErrors` section describes — `path`/`reason`/`occurred_at`, the
/// exact three columns `import_errors` has always had).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportErrorRow {
    pub id: i64,
    pub path: String,
    pub reason: String,
    pub occurred_at: i64,
}

// `tests.rs` holds the core suite (query-builder/whitelist/LIKE-escaping,
// Library/Missing); the Playlist/Smart/Queue/maintenance sections of the
// same original `queries.rs` test module are split into the sibling files
// below purely to keep every file under the project's 800-line rule — see
// `tests.rs`'s own doc comment.
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_maintenance;
#[cfg(test)]
mod tests_playlist;
#[cfg(test)]
mod tests_queue;
#[cfg(test)]
mod tests_smart;
