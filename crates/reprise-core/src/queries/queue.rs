//! `ViewSource::Queue` window/count queries — a window over a caller-supplied
//! id list (the queue's own current play order) rather than a `WHERE`
//! clause, plus the queue-ids cap (`QUEUE_LIMIT`) and its overflow check
//! (`is_queue_capped`). Split out of the former single-file `queries.rs`
//! (Refactoring & Extensibility Task 1) — a pure move, no behavior change.

use std::collections::HashMap;

use crate::models::Track;

use super::clauses::row_to_track;
use super::MAX_WINDOW_LIMIT;
use rusqlite::Connection;

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

/// Window over an explicit id list, in that list's own order — see the
/// module doc's `Queue` section for why this slices in Rust rather than
/// asking SQL to preserve an arbitrary order.
pub(super) fn query_track_window_queue(
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

    // Resolve each *distinct* id once — a duplicated id must still render
    // once per occurrence in `slice` (see below), so the id list handed to
    // `IN (...)` is deduplicated first to keep the query and its parameter
    // count independent of how many times an id repeats in this page.
    let distinct_ids: Vec<i64> = slice
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let placeholders = (1..=distinct_ids.len())
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
        .query_map(
            rusqlite::params_from_iter(distinct_ids.iter()),
            row_to_track,
        )?
        .collect::<Result<_, _>>()?;

    // invariant: an id in `ids` with no matching `tracks` row is silently
    // dropped here (via `filter_map`), so this window's row count can be
    // *less* than `slice.len()`. Stage-3 close-out: hard-delete now exists
    // (`remove_missing_track`/`remove_missing_tracks`), so this is reachable
    // — a queued id can genuinely stop resolving mid-life. Two things keep
    // this from desyncing the UI: (1) the queue itself is purged of any
    // hard-deleted id in lockstep, via `ui::player_controller::
    // PlayerController::purge_queue_ids`, called from the "Remove from
    // library" flow (`ui::track_list_context_menu::handle_remove_from_
    // library`); (2) belt-and-braces, `query_track_count`'s `Queue` arm
    // (`query_track_count_queue`) counts actually-matched rows rather than
    // trusting `queue_ids.len()` verbatim, so even an unpurged stale id
    // can't make a `ColumnView` believe there are more rows than this
    // function will ever render (see `queue_count_matches_window_row_count_
    // when_some_ids_do_not_resolve` below).
    //
    // A *duplicated* id within `slice` is resolved independently per
    // occurrence (via `by_id.get(id).cloned()`, never `remove`), so two
    // copies of the same queued id yield two rows, each in its own slot —
    // this used to be a `HashMap::remove`-based drain that silently
    // swallowed every occurrence after the first, desyncing DnD reorder
    // (which uses view row position as queue index) for every row after the
    // duplicate. See `queue_window_renders_a_duplicated_id_once_per_
    // occurrence` below.
    let by_id: HashMap<i64, Track> = rows.into_iter().map(|t| (t.id, t)).collect();
    Ok(slice
        .iter()
        .filter_map(|id| by_id.get(id).cloned())
        .collect())
}

/// Counts how many of `queue_ids` still resolve to a live `tracks` row —
/// the `Queue` count arm of `query_track_count` (Stage-3 close-out: hard-
/// delete now exists — `remove_missing_track`/`remove_missing_tracks` — so a
/// queued id can no longer be assumed to resolve; see that function's doc
/// comment for the full history). Every occurrence in `queue_ids` is
/// counted independently (not deduplicated first), matching `query_track_
/// window_queue`'s own per-slot resolution — a track queued twice that
/// still exists counts twice, and (Stage-3 close-out, dup-id follow-up)
/// `query_track_window_queue` now genuinely renders it twice too: it used
/// to resolve slots via a `HashMap::remove`-based drain, which silently
/// dropped every occurrence of a duplicated id after the first, so this
/// invariant held for the count but not for the window it was compared
/// against. Both are now id-resolution-independent-per-slot, so this count
/// equals the window's row count for any `queue_ids`, duplicates included
/// — see `queue_window_renders_a_duplicated_id_once_per_occurrence` and
/// `queue_count_matches_window_row_count_with_a_duplicated_id` below.
pub(super) fn query_track_count_queue(
    conn: &Connection,
    queue_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    if queue_ids.is_empty() {
        return Ok(0);
    }
    let unique_ids: std::collections::HashSet<i64> = queue_ids.iter().copied().collect();
    let placeholders = (1..=unique_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT id FROM tracks WHERE id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let existing: std::collections::HashSet<i64> = stmt
        .query_map(rusqlite::params_from_iter(unique_ids.iter()), |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(queue_ids.iter().filter(|id| existing.contains(id)).count() as i64)
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
