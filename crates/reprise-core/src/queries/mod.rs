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
//! - **Library**: `clauses::PRESENT` — unchanged in shape from before this
//!   task; only the underlying predicate moved from the legacy `missing = 0`
//!   literal to `missing_since IS NULL AND removed_at IS NULL` (Task 1.2).
//! - **Missing**: identical shape to Library, `clauses::MISSING` instead.
//! - **Playlist(id)**: `JOIN playlist_tracks pt ON pt.track_id = tracks.id
//!   WHERE pt.playlist_id = id AND` `clauses::PRESENT`. Duplicates (the same track
//!   added to a playlist twice) surface as separate, position-keyed rows —
//!   a natural consequence of the join, matching Task 2's manual-playlist
//!   semantics. Default order is `pt.position` via a whitelist *sentinel*
//!   sort field, `"playlist_order"` (see `SORT_WHITELIST`) — not a
//!   passthrough of arbitrary text, so the whitelist is never weakened by
//!   this addition. A column header click still works: `track_list.rs`
//!   passes a normal whitelisted field (e.g. `"title"`) instead, and this
//!   module's shared `order_clause` treats it exactly like any other
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
//!   smart_rules_to_sql`'s WHERE fragment with `clauses::PRESENT` and the live
//!   search filter. Its own `sort_field`/`sort_dir`/`limit_count` choose the
//!   member set first (a "Top 50" definition must keep meaning Top 50), then
//!   the track list's current column sort orders those members for display.
//!   Both sort pairs run through the shared `order_clause`, so a
//!   hand-edited (DB-tampered) sort field silently falls back to title order,
//!   same as every other source.
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
//!   order; the *outer* query applies the user's current column sort and
//!   slices out the caller's window via its own `LIMIT`/`OFFSET`.
//!   `query_track_count`'s smart arm
//!   mirrors this with plain arithmetic (`raw_count.min(limit_count)`)
//!   since a count has no rows to slice.
//! - **Queue**: typed items are supplied by the caller in manual queue order.
//!   The window is sliced in Rust, each item kind present is resolved by one
//!   batched query, and results are restored to occurrence order. Missing
//!   tracks and unavailable episodes are silently skipped. Track-only query
//!   surfaces project just the track entries until their callers become
//!   item-aware. The live search filter is intentionally ignored.
//! - **ImportErrors**: Task 8 defines the real (non-`tracks`) row shape and
//!   columns; every query here degrades to an empty window/zero count for
//!   this source in the meantime (see `ViewSource`'s own doc comment).
//!   `query_import_error_count` exposes the one piece of this source this
//!   task builds ahead of time: a bare count of the existing `import_errors`
//!   table, for a future sidebar badge.

use crate::db::Db;
#[cfg(test)]
use crate::{up_next::QueueItem, view_source::ViewSource};
#[cfg(test)]
use rusqlite::Connection;

#[cfg(test)]
fn test_sort<'a>(field: &'a str, dir: &'a str) -> TrackSort<'a> {
    TrackSort { field, dir }
}

#[cfg(test)]
fn test_rows(offset: i64, limit: i64) -> RowWindow {
    RowWindow { offset, limit }
}

mod album_directories;
mod artist_context;
pub mod autocomplete;
mod browse;
mod clauses;
mod doctor;
mod import_errors;
mod issues;
mod library;
pub(crate) mod library_views;
mod maintenance;
mod maintenance_delete;
mod maintenance_missing;
mod playlist;
mod queue;
mod smart;
mod stats;
mod surface_browse;
mod track_summary;
mod track_view;

pub use album_directories::query_album_directories;
pub use artist_context::{query_artist_album_titles, query_stats_album_target_for_path};
pub use browse::{query_browse_values, BrowseFacet, BrowseFilter, BrowseValue};
pub use clauses::{build_track_ids_query, sort_key_columns};
pub use doctor::{count_doctor_findings, count_pending_doctor_findings, DoctorFindingCounts};
// Task 1.2: the centralized presence predicate, re-exported so modules
// outside this one (`library::scanner`, `library::artist_detail`, `db::
// pending_waveform_tracks`) can share the exact same "row is present" SQL
// fragment as every query in this module tree — see `clauses::PRESENT`'s
// doc comment for why a flag-plus-date pair is retired in favor of this one
// predicate.
pub(crate) use clauses::PRESENT;
// Library code and test helpers share this predicate through the query seam;
// rustc does not count same-crate references through a re-export as import use.
#[allow(
    unused_imports,
    reason = "same-crate callers use the shared predicate through this query seam"
)]
pub(crate) use clauses::MISSING;
// Tests and the scalability example share this seam; the library target alone
// does not count the public re-export as used.
#[allow(
    unused_imports,
    reason = "tests and the scalability example use this public query seam"
)]
pub use clauses::build_track_query;
// Task 2.1: the missing-file group queries the 18a "self-healing" card list
// is built directly against — see `issues`'s module doc for the full
// `MissingGroupKind` taxonomy and why `unknown` never joins `Deleted`.
// `pub use` (not `pub(crate)`) so `reprise-gnome` can name these types
// directly, the same reachability fix Task 1's `ImportErrorKind` move to
// `models` made for the same reason (see that commit's message).
pub use issues::{
    query_missing_groups, query_missing_groups_matching, query_missing_rows,
    query_missing_rows_matching, MissingGroup, MissingGroupKind,
};
// Task 2.5: the sidebar badge counts, keyed on `last_viewed_*` — see
// `issues`'s "Badge counts" section for the `count_missing`/`count_new_
// missing` split. `pub use` for the same cross-crate reachability reason as
// `query_missing_groups` above.
pub use issues::{count_missing, count_new_missing};
pub use issues::{
    mark_mount_unavailable, verify_unmounted_tracks, verify_unmounted_tracks_with_source,
};
// Task 2.3: the auto-clean read/act split — `auto_clean_eligible` for a
// preview, `run_auto_clean` for the real unattended deletion. `pub use` for
// the same cross-crate reachability reason as `query_missing_groups` above:
// the GUI (a later task) needs to name both directly as `reprise_core::
// queries::{auto_clean_eligible, run_auto_clean}`.
pub use issues::{auto_clean_eligible, run_auto_clean, tombstone_still_missing};
// Task 2.4: the grouped import-error read/write queries the ImportErrors
// triage UI is built against — see `import_errors`'s module doc for the
// hint contract and the dismiss/restore semantics. `pub use` for the same
// cross-crate reachability reason as `query_missing_groups` above.
pub use import_errors::{
    count_dismissed_import_errors, dismiss_all_import_errors, dismiss_import_error,
    query_dismissed_import_errors, query_import_errors_grouped, restore_import_error,
    ImportErrorEntry,
};
// Task 2.5: the import-errors half of the sidebar badge counts — see
// `import_errors`'s own "Badge counts" section for the hint-inclusion split
// between the two. `pub use` for the same cross-crate reachability reason as
// `query_missing_groups` above.
pub use import_errors::{count_import_errors_active, count_new_import_errors};
pub use library_views::*;
pub(crate) use maintenance::remove_tracks_matching_paths_remembering_releases;
pub use maintenance::{
    exclude_tracks_matching_paths, filter_present, purge_tombstones, query_has_live_tracks,
    query_import_error_count, query_live_track_ids, query_live_track_paths,
    query_live_track_summaries, query_queue_purge_track_ids, query_queue_retained_track_ids,
    query_random_live_track_ids, query_sync_tracks, query_sync_tracks_with_source,
    query_track_album_artist, query_track_ids_by_title_desc, query_track_ids_by_titles,
    query_track_summaries_added_since, query_track_summary, remove_missing_tracks,
    remove_tracks_matching_paths, tombstone_tracks, track_id_for_path, undo_tombstone,
};
pub use maintenance_missing::mark_track_missing_if_current;
pub use track_summary::TrackSummary;
// `remove_tracks_impl`/`RemoveGuard` are the internal shared deletion path
// `remove_missing_tracks`/`purge_tombstones`/`remove_tracks_matching_paths` all funnel
// through; not part of the crate's public API, but `tests_issues.rs`'s
// mid-purge-resurrection regression test (Finding 1) needs to call the
// `TombstonedOnly`-guarded delete directly — a real thread race can't be
// scheduled deterministically, so the test proves the guard by driving this
// same statement with a stale id snapshot instead.
#[cfg(test)]
pub(crate) use maintenance::{remove_tracks_impl, RemoveGuard};
pub use playlist::query_playlist_tracks_full;
pub use queue::{
    is_queue_capped, query_available_episode_ids, query_queue_duration_ms, query_queue_item_window,
    QueueItemMetadata, QUEUE_LIMIT,
};
pub use stats::{query_library_stats, query_library_stats_browsed, LibraryStats};
pub use surface_browse::*;
pub(crate) use track_view::query_track_ids_in;
pub use track_view::{
    query_track_count, query_track_ids, query_track_window, query_visible_track_ids_browsed,
    AiColumn, RowWindow, TrackSort, TrackViewQuery,
};

/// Global constraint: window queries never return more rows than this in one
/// page, regardless of what the caller requests. SQLite treats a negative
/// `LIMIT` as "unlimited", so this also protects against a bad UI-side page
/// size from turning into a full-table scan. Limits capped.
pub const MAX_WINDOW_LIMIT: i64 = 500;

/// The absolute on-disk path of a track by id, or `None` if the row is gone.
/// The focused lookup an instrumental worker uses to resolve a job's
/// `source_track_id` to the file its backend reads (P3b) — cheaper than
/// fetching a whole [`maintenance::query_track_summary`], and the seam that
/// keeps productive frontend code out of assembling SQL.
pub fn track_source_path(
    db: &Db,
    track_id: i64,
) -> Result<Option<std::path::PathBuf>, rusqlite::Error> {
    let conn = db.conn();
    use rusqlite::OptionalExtension;
    conn.query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .map(|path| path.map(std::path::PathBuf::from))
}

// `tests.rs` holds the core suite (query-builder/whitelist/LIKE-escaping,
// Library/Missing); the Playlist/Smart/Queue/maintenance sections of the
// same original `queries.rs` test module are split into the sibling files
// below purely to keep every file under the project's 800-line rule — see
// `tests.rs`'s own doc comment.
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_auto_clean;
#[cfg(test)]
mod tests_deleted_release_memory;
#[cfg(test)]
mod tests_genre_scope;
#[cfg(test)]
mod tests_import_errors;
#[cfg(test)]
mod tests_issues;
#[cfg(test)]
mod tests_issues_badges;
#[cfg(test)]
mod tests_issues_unlocatable;
#[cfg(test)]
mod tests_maintenance;
#[cfg(test)]
mod tests_mount_events;
#[cfg(test)]
mod tests_playlist;
#[cfg(test)]
mod tests_queue;
#[cfg(test)]
mod tests_search_fields;
#[cfg(test)]
mod tests_smart;
#[cfg(test)]
mod tests_source_path_ai;
#[cfg(test)]
mod tests_track_view;
#[cfg(test)]
mod tests_ux_feedback;
