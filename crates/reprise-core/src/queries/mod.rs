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

mod artist_context;
pub mod autocomplete;
mod browse;
mod clauses;
mod import_errors;
mod issues;
mod library;
pub(crate) mod library_views;
mod maintenance;
mod playlist;
mod queue;
mod smart;

pub use artist_context::query_artist_albums;
pub use browse::{query_browse_values, BrowseFacet, BrowseFilter, BrowseValue};
pub use clauses::build_track_ids_query;
// Task 1.2: the centralized presence predicate, re-exported so modules
// outside this one (`library::scanner`, `library::artist_detail`, `db::
// pending_waveform_tracks`) can share the exact same "row is present" SQL
// fragment as every query in this module tree — see `clauses::PRESENT`'s
// doc comment for why a flag-plus-date pair is retired in favor of this one
// predicate.
pub(crate) use clauses::PRESENT;
// `MISSING`'s only current caller outside this module tree is `library::
// scanner_vanished_tests`'s `missing_count` helper, which mirrors this
// predicate for a direct-SQL assertion — re-exported regardless, same
// reasoning as `build_track_query` below, to keep that one string in sync
// with the predicate it is meant to test rather than drifting as a
// hand-copied literal.
#[allow(unused_imports)]
pub(crate) use clauses::MISSING;
// `build_track_query`'s only current caller is this module's own test suite
// (`tests::query_builder_whitelists_and_sorts` et al.) — re-exported `pub`
// regardless, to keep `crate::queries::build_track_query` resolving exactly
// as it did before this split, matching this file's own non-test build
// where the re-export would otherwise look unused.
#[allow(unused_imports)]
pub use clauses::build_track_query;
// Task 2.1: the missing-file group queries the 18a "self-healing" card list
// is built directly against — see `issues`'s module doc for the full
// `MissingGroupKind` taxonomy and why `unknown` never joins `Deleted`.
// `pub use` (not `pub(crate)`) so `reprise-gnome` can name these types
// directly, the same reachability fix Task 1's `ImportErrorKind` move to
// `models` made for the same reason (see that commit's message).
pub use issues::{query_missing_groups, query_missing_rows, MissingGroup, MissingGroupKind};
// Task 2.5: the sidebar badge counts, keyed on `last_viewed_*` — see
// `issues`'s "Badge counts" section for the `count_missing`/`count_new_
// missing` split. `pub use` for the same cross-crate reachability reason as
// `query_missing_groups` above.
pub use issues::{count_missing, count_new_missing};
pub use issues::{mark_mount_unavailable, verify_unmounted_tracks};
// Task 2.3: the auto-clean read/act split — `auto_clean_eligible` for a
// preview, `run_auto_clean` for the real unattended deletion. `pub use` for
// the same cross-crate reachability reason as `query_missing_groups` above:
// the GUI (a later task) needs to name both directly as `reprise_core::
// queries::{auto_clean_eligible, run_auto_clean}`.
pub use issues::{auto_clean_eligible, run_auto_clean};
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
pub use library_views::{
    query_album_canonical_track_ids, query_album_count, query_album_track_ids, query_albums,
    query_artist_count, query_artist_detail_albums, query_artists, AlbumSummary, ArtistAlbum,
    ArtistSummary,
};
pub use maintenance::{
    exclude_tracks_matching_paths, filter_present, mark_track_missing_if_current, purge_tombstones,
    query_has_live_tracks, query_import_error_count, query_live_track_ids, query_live_track_paths,
    query_queue_purge_track_ids, query_queue_retained_track_ids, query_random_live_track_ids,
    query_sync_tracks, query_track_album_artist, query_track_ids_by_title_desc,
    query_track_ids_by_titles, query_track_summary, remove_missing_tracks,
    remove_tracks_matching_paths, tombstone_tracks, track_id_for_path, undo_tombstone,
};
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
pub use queue::{is_queue_capped, query_queue_duration_ms, QUEUE_LIMIT};

use clauses::build_track_ids_query_browsed;
use clauses::{build_track_ids_query_base, like_pattern, row_to_id};
use rusqlite::types::Value;

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
    query_track_window_browsed(
        conn,
        source,
        sort_field,
        sort_dir,
        filter,
        &BrowseFilter::default(),
        offset,
        limit,
        queue_ids,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn query_track_window_browsed(
    conn: &mut Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    offset: i64,
    limit: i64,
    queue_ids: &[i64],
) -> Result<Vec<Track>, rusqlite::Error> {
    query_track_window_browsed_ai(
        conn, source, sort_field, sort_dir, filter, browse, offset, limit, queue_ids, false, true,
    )
}

/// Like [`query_track_window_browsed`] but honoring two AI concerns:
///
/// - `exclude_ai` (plan 2.4/8, Beschluss 17): when set, tracks flagged in
///   `track_provenance` are hidden. Only `Library` honors it — that is where
///   the browse filter row lives.
/// - `project_ai` (INST-10 / FIX-4): whether to project the real `is_ai` column
///   (the correlated provenance `EXISTS`) or a literal `0`. The AI badge only
///   renders while the experimental switch is on, so the GTK layer passes
///   `experimental_on` here; when off, every source's window carries no per-row
///   provenance subquery. Honored by **all** sources (the badge can appear on
///   any track row). The default entry points above pass `false`/`true`.
#[allow(clippy::too_many_arguments)]
pub fn query_track_window_browsed_ai(
    conn: &mut Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    offset: i64,
    limit: i64,
    queue_ids: &[i64],
    exclude_ai: bool,
    project_ai: bool,
) -> Result<Vec<Track>, rusqlite::Error> {
    match source {
        ViewSource::Library => library::query_track_window_library(
            conn, sort_field, sort_dir, filter, offset, limit, browse, exclude_ai, project_ai,
        ),
        ViewSource::RecentlyAdded => {
            let browse = recently_added_browse(browse);
            library::query_track_window_library(
                conn, sort_field, sort_dir, filter, offset, limit, &browse, exclude_ai, project_ai,
            )
        }
        ViewSource::Missing => library::query_track_window_missing(
            conn, sort_field, sort_dir, filter, offset, limit, project_ai,
        ),
        ViewSource::Playlist(id) => playlist::query_track_window_playlist(
            conn, *id, sort_field, sort_dir, filter, offset, limit, project_ai,
        ),
        ViewSource::Smart(id) => smart::query_track_window_smart(
            conn,
            *id,
            (sort_field, sort_dir),
            filter,
            offset,
            limit,
            project_ai,
        ),
        ViewSource::Queue => {
            queue::query_track_window_queue(conn, queue_ids, offset, limit, project_ai)
        }
        ViewSource::Album {
            album,
            album_artist,
        } => library_views::query_album_track_window(
            conn,
            album,
            album_artist,
            sort_field,
            sort_dir,
            filter,
            browse,
            offset,
            limit,
            project_ai,
        ),
        ViewSource::Artist(artist) => library_views::query_artist_track_window(
            conn, artist, sort_field, sort_dir, filter, browse, offset, limit, project_ai,
        ),
        ViewSource::Genre(genre) => {
            let browse = genre_browse(genre, browse);
            library::query_track_window_library(
                conn, sort_field, sort_dir, filter, offset, limit, &browse, exclude_ai, project_ai,
            )
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

/// Counts rows matching `(source, filter)` — see the module doc for how
/// each source defines "matching". `queue_ids` is only read for
/// `ViewSource::Queue`.
pub fn query_track_count(
    conn: &Connection,
    source: &ViewSource,
    filter: &str,
    queue_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    query_track_count_browsed(conn, source, filter, &BrowseFilter::default(), queue_ids)
}

pub fn query_track_count_browsed(
    conn: &Connection,
    source: &ViewSource,
    filter: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    match source {
        ViewSource::Library => library::query_track_count_library(conn, filter, browse),
        ViewSource::RecentlyAdded => {
            library::query_track_count_library(conn, filter, &recently_added_browse(browse))
        }
        ViewSource::Missing => library::query_track_count_missing(conn, filter),
        ViewSource::Playlist(id) => playlist::query_track_count_playlist(conn, *id, filter),
        ViewSource::Smart(id) => smart::query_track_count_smart(conn, *id, filter),
        // Stage-3 close-out fix: this used to trust `queue_ids.len()`
        // verbatim, on the documented assumption that nothing hard-deletes a
        // `tracks` row. That assumption no longer holds (`remove_missing_tracks`
        // does exactly that) — the queue itself
        // is purged in lockstep by `ui::player_controller::PlayerController::
        // purge_queue_ids` whenever a hard-delete happens through the app's
        // own UI, but counting matched rows here (rather than trusting the
        // caller's `queue_ids` slice) is a second, independent guarantee
        // that a `ColumnView` can never be told there are more rows than
        // `query_track_window_queue` will actually render, even if some
        // future caller forgets to purge the queue after a hard-delete.
        ViewSource::Queue => queue::query_track_count_queue(conn, queue_ids),
        ViewSource::Album {
            album,
            album_artist,
        } => library_views::query_album_track_count(conn, album, album_artist, filter, browse),
        ViewSource::Artist(artist) => {
            library_views::query_artist_track_count(conn, artist, filter, browse)
        }
        ViewSource::Genre(genre) => {
            library::query_track_count_library(conn, filter, &genre_browse(genre, browse))
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

/// The absolute on-disk path of a track by id, or `None` if the row is gone.
/// The focused lookup an instrumental worker uses to resolve a job's
/// `source_track_id` to the file its backend reads (P3b) — cheaper than
/// fetching a whole [`maintenance::query_track_summary`], and the seam that
/// keeps productive frontend code out of assembling SQL.
pub fn track_source_path(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<std::path::PathBuf>, rusqlite::Error> {
    use rusqlite::OptionalExtension;
    conn.query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .map(|path| path.map(std::path::PathBuf::from))
}

/// Like [`query_track_count_browsed`] but honoring the FIL-7 AI-exclude filter
/// (Beschluss 17), matching [`query_track_ids_browsed_ai`]: only `Library`
/// honors `exclude_ai`, so every other source ignores it and delegates. This is
/// the cheap `COUNT(*)` the AI-filtered view uses for its total instead of an
/// id-list length, which would silently cap at `QUEUE_LIMIT`.
pub fn query_track_count_browsed_ai(
    conn: &Connection,
    source: &ViewSource,
    filter: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
    exclude_ai: bool,
) -> Result<i64, rusqlite::Error> {
    match source {
        ViewSource::Library => {
            library::query_track_count_library_ai(conn, filter, browse, exclude_ai)
        }
        ViewSource::Genre(genre) => library::query_track_count_library_ai(
            conn,
            filter,
            &genre_browse(genre, browse),
            exclude_ai,
        ),
        ViewSource::RecentlyAdded => library::query_track_count_library_ai(
            conn,
            filter,
            &recently_added_browse(browse),
            exclude_ai,
        ),
        _ => query_track_count_browsed(conn, source, filter, browse, queue_ids),
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
    query_track_ids_browsed(
        conn,
        source,
        sort_field,
        sort_dir,
        filter,
        &BrowseFilter::default(),
        queue_ids,
    )
}

pub fn query_track_ids_browsed(
    conn: &Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    query_track_ids_browsed_ai(
        conn, source, sort_field, sort_dir, filter, browse, queue_ids, false,
    )
}

/// Like [`query_track_ids_browsed`] but honoring the AI-exclude filter on the
/// flat Library source (plan 2.4/8, Beschluss 17): the queue seam "Play all"
/// builds from hides AI-flagged tracks when `exclude_ai` is set, so
/// at-queue-end refill follows the visible view. Only `Library` honors it.
#[allow(clippy::too_many_arguments)]
pub fn query_track_ids_browsed_ai(
    conn: &Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
    exclude_ai: bool,
) -> Result<Vec<i64>, rusqlite::Error> {
    match source {
        ViewSource::Library => {
            let has_filter = !filter.trim().is_empty();
            let sql =
                build_track_ids_query_browsed(sort_field, sort_dir, has_filter, browse, exclude_ai);
            let mut stmt = conn.prepare(&sql)?;
            let mut params = Vec::new();
            if has_filter {
                params.push(Value::Text(like_pattern(filter.trim())));
            }
            let (_, browse_values) = browse::browse_clause(browse, params.len() + 1);
            params.extend(browse_values.into_iter().map(Value::Text));
            let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_id)?;
            rows.collect()
        }
        ViewSource::RecentlyAdded => {
            query_track_ids_recently_added(conn, sort_field, sort_dir, filter, browse, exclude_ai)
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
        ViewSource::Playlist(id) => playlist::query_playable_track_ids_playlist(conn, *id, filter),
        ViewSource::Smart(id) => {
            smart::query_track_ids_smart(conn, *id, sort_field, sort_dir, filter)
        }
        ViewSource::Queue => Ok(queue_ids.to_vec()),
        ViewSource::Album {
            album,
            album_artist,
        } => library_views::query_album_track_ids_browsed(
            conn,
            album,
            album_artist,
            sort_field,
            sort_dir,
            filter,
            browse,
        ),
        ViewSource::Artist(artist) => library_views::query_artist_track_ids(
            conn, artist, sort_field, sort_dir, filter, browse,
        ),
        ViewSource::Genre(genre) => {
            let browse = genre_browse(genre, browse);
            let has_filter = !filter.trim().is_empty();
            let sql = build_track_ids_query_browsed(
                sort_field, sort_dir, has_filter, &browse, exclude_ai,
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut params = Vec::new();
            if has_filter {
                params.push(Value::Text(like_pattern(filter.trim())));
            }
            let (_, browse_values) = browse::browse_clause(&browse, params.len() + 1);
            params.extend(browse_values.into_iter().map(Value::Text));
            let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_id)?;
            rows.collect()
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

fn genre_browse(genre: &str, browse: &BrowseFilter) -> BrowseFilter {
    let mut scoped = browse.clone();
    scoped.genre = Some(genre.trim().to_owned());
    scoped
}

fn recently_added_browse(browse: &BrowseFilter) -> BrowseFilter {
    const SEVEN_DAYS_SECONDS: i64 = 7 * 24 * 60 * 60;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    BrowseFilter {
        added_since: Some(now.saturating_sub(SEVEN_DAYS_SECONDS).to_string()),
        ..browse.clone()
    }
}

fn query_track_ids_recently_added(
    conn: &Connection,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    exclude_ai: bool,
) -> Result<Vec<i64>, rusqlite::Error> {
    let browse = recently_added_browse(browse);
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_ids_query_browsed(sort_field, sort_dir, has_filter, &browse, exclude_ai);
    let mut stmt = conn.prepare(&sql)?;
    let mut params = Vec::new();
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let (_, browse_values) = browse::browse_clause(&browse, params.len() + 1);
    params.extend(browse_values.into_iter().map(Value::Text));
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_id)?;
    rows.collect()
}

/// Returns the ids represented by the current visible view. This differs
/// from [`query_track_ids_browsed`] only for manual playlists: their missing
/// members remain selectable at their durable positions, while playback
/// continues to seed queues from playable rows only.
pub fn query_visible_track_ids_browsed(
    conn: &Connection,
    source: &ViewSource,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    match source {
        ViewSource::Playlist(id) => {
            playlist::query_visible_track_ids_playlist(conn, *id, sort_field, sort_dir, filter)
        }
        _ => query_track_ids_browsed(
            conn, source, sort_field, sort_dir, filter, browse, queue_ids,
        ),
    }
}

/// The subset of a track's columns the player bar and queue playback path
/// need: the file to hand `Player::play`, display metadata, and the duration
/// play-tracking's 50%-listened check requires
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
    /// Raw album artist tag (may be empty). Use `effective_album_artist()` to
    /// get the display value that matches `AlbumSummary::album_artist` — i.e.
    /// `album_artist` when non-empty, `artist` otherwise. Loaded alongside the
    /// other summary fields so `notify_now_playing_album_changed` can send the
    /// same effective-artist key the album grid uses for EQ-marker matching.
    pub album_artist: String,
    /// Raw genre and artist MBID are retained by the in-flight playback
    /// snapshot so local listen history remains complete after catalog
    /// deletion.
    pub genre: String,
    pub artist_mbid: Option<String>,
    /// Optional release year displayed by metadata-rich player surfaces.
    pub year: Option<i32>,
    pub duration_ms: i64,
}

impl TrackSummary {
    /// Returns the effective album artist: `album_artist` when non-empty
    /// (trimmed), `artist` otherwise. Mirrors the SQL expression
    /// `CASE WHEN TRIM(album_artist) <> '' THEN TRIM(album_artist) ELSE
    /// TRIM(artist) END` that `query_albums` uses for `AlbumSummary::
    /// album_artist`, so the two sources always agree on the grouping key.
    pub fn effective_album_artist(&self) -> &str {
        if self.album_artist.trim().is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }
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
pub fn query_library_stats(
    conn: &Connection,
    filter: &str,
) -> Result<LibraryStats, rusqlite::Error> {
    query_library_stats_browsed(conn, filter, &BrowseFilter::default())
}

pub fn query_library_stats_browsed(
    conn: &Connection,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<LibraryStats, rusqlite::Error> {
    let (track_count, total_duration_ms) = conn.query_row(
        &format!("SELECT count(*), coalesce(sum(duration_ms),0) FROM tracks WHERE {PRESENT}"),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
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
mod tests_genre_scope;
#[cfg(test)]
mod tests_import_errors;
#[cfg(test)]
mod tests_issues;
#[cfg(test)]
mod tests_issues_badges;
#[cfg(test)]
mod tests_maintenance;
#[cfg(test)]
mod tests_mount_events;
#[cfg(test)]
mod tests_playlist;
#[cfg(test)]
mod tests_queue;
#[cfg(test)]
mod tests_smart;
#[cfg(test)]
mod tests_source_path_ai;
#[cfg(test)]
mod tests_ux_feedback;
