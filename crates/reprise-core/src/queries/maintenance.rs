//! Track/import-error maintenance queries: missing-file marking and hard
//! delete, import-error triage, path/summary lookups. Split out of the
//! former single-file `queries.rs` (Refactoring & Extensibility Task 1) — a
//! pure move, no behavior change.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::device_sync::SyncTrack;
use crate::library::playlists;
use crate::models::MissingReason;
use rusqlite::{Connection, OptionalExtension};

use super::clauses::{MISSING, PRESENT};
use super::queue::QUEUE_LIMIT;
use super::{ImportErrorRow, TrackSummary};

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
        "SELECT path, title, artist, album, album_artist, year, duration_ms FROM tracks WHERE id = ?1",
        rusqlite::params![id],
        |r| {
            Ok(TrackSummary {
                path: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                album: r.get(3)?,
                album_artist: r.get(4)?,
                year: r.get(5)?,
                duration_ms: r.get(6)?,
            })
        },
    )
    .optional()
}

/// Returns every non-missing track id for validating persisted playback
/// queues. A set matches the caller's membership-only use and avoids leaking
/// query ordering into session semantics.
pub fn query_live_track_ids(conn: &Connection) -> Result<HashSet<i64>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!("SELECT id FROM tracks WHERE {PRESENT}"))?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Returns every non-missing media path in stable path order for cover batch
/// scheduling.
pub fn query_live_track_paths(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "SELECT path FROM tracks WHERE {PRESENT} ORDER BY path"
    ))?;
    let paths = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(paths)
}

/// Resolves exact titles to deterministic track ids. This is intended for
/// synthetic smoke fixtures, so duplicate titles choose the lowest id and
/// missing rows remain eligible exactly as they did in the original hook.
pub fn query_track_ids_by_titles(
    conn: &Connection,
    titles: &[&str],
) -> Result<HashMap<String, i64>, rusqlite::Error> {
    let mut statement =
        conn.prepare("SELECT id FROM tracks WHERE title = ?1 ORDER BY id LIMIT 1")?;
    let mut ids = HashMap::with_capacity(titles.len());
    for &title in titles {
        if let Some(id) = statement.query_row([title], |row| row.get(0)).optional()? {
            ids.insert(title.to_string(), id);
        }
    }
    Ok(ids)
}

/// Returns all library ids in descending title order for the playlist smoke
/// seed. Missing rows are intentionally retained to preserve the hook's
/// historical behavior.
pub fn query_track_ids_by_title_desc(conn: &Connection) -> Result<Vec<i64>, rusqlite::Error> {
    let mut statement = conn.prepare("SELECT id FROM tracks ORDER BY title DESC")?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Resolves one track id to its *effective album artist* — the album artist
/// when tagged, otherwise the track artist, using the exact same
/// `EFFECTIVE_ALBUM_ARTIST` SQL fallback (with `TRIM`) the Artists library
/// view groups by. The player-bar artist deep-link (`ui::player_controller`'s
/// `current_track_album_artist`) needs this to select the correct master row,
/// since the now-playing display cache only carries the *track* artist.
/// `Ok(None)` for an unknown id; the returned string may be empty when neither
/// tag is set (the caller treats blank as "no artist to navigate to").
pub fn query_track_album_artist(
    conn: &Connection,
    id: i64,
) -> Result<Option<String>, rusqlite::Error> {
    let effective = super::library_views::EFFECTIVE_ALBUM_ARTIST;
    conn.query_row(
        &format!("SELECT {effective} FROM tracks WHERE id = ?1"),
        rusqlite::params![id],
        |r| r.get(0),
    )
    .optional()
}

/// Resolves a drag payload into copy-ready tracks without trusting stale UI
/// metadata. Input order is preserved, repeated ids are emitted once, and
/// rows that are unknown, marked missing, or no longer regular local files
/// are omitted. The file size is read at enqueue time so progress totals
/// describe the bytes that will actually be copied.
pub fn query_sync_tracks(
    conn: &Connection,
    ids: &[i64],
) -> Result<Vec<SyncTrack>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "SELECT path,title,artist,album,album_artist,track_no,duration_ms \
         FROM tracks WHERE id = ?1 AND {PRESENT}"
    ))?;
    let mut seen = HashSet::new();
    let mut tracks = Vec::with_capacity(ids.len());
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let row = statement
            .query_row(rusqlite::params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<u32>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })
            .optional()?;
        let Some((path, title, artist, album, album_artist, track_number, duration_ms)) = row
        else {
            continue;
        };
        let source_path = PathBuf::from(path);
        let Ok(metadata) = source_path.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let Some(original_name) = source_path.file_name() else {
            continue;
        };
        tracks.push(SyncTrack {
            id,
            source_path: source_path.clone(),
            original_name: original_name.to_string_lossy().into_owned(),
            title,
            artist,
            album,
            album_artist,
            track_number,
            duration_ms,
            size_bytes: metadata.len(),
            source_mtime: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or(0),
        });
    }
    Ok(tracks)
}

/// Marks track `track_id` as missing (Stage 2 Task 5: a physically deleted
/// file must never crash or dead-end the app — this is the DB-side half of
/// that guarantee). Every windowed/count/id query for `ViewSource::Library`/
/// `Playlist`/`Smart` already filters on `PRESENT`, so the row disappears
/// from those views and from a freshly-seeded queue on the very next
/// reload, without deleting the row itself — ratings/play history/etc. are
/// preserved, and the row resurfaces in `ViewSource::Missing` (Stage 3 Task
/// 3) instead of vanishing outright.
///
/// Task 1.2: this is the playback-fault call site `Track::is_missing`'s doc
/// comment refers to. Task 1.5 swapped the blanket `MissingReason::Unknown`
/// this used to always write for a real verdict from `library::mounts::
/// classify_missing`, the same classifier the scanner's own folded-in
/// mark-vanished phase (`library::scanner::scan_folder`) uses — one `SELECT`
/// of the row's `path`/`device` first, since this function's only input is
/// `track_id`. `Ok(())` no-ops (writes `Unknown`, same as the pre-classifier
/// behavior, then updates 0 rows) for an id that no longer exists, matching
/// this function's pre-1.5 contract of never erroring on a stale id — the
/// playback-fault call site races against the row being removed out from
/// under it (e.g. a concurrent watcher reconcile) more plausibly than most
/// callers in this codebase.
pub fn mark_track_missing(conn: &Connection, track_id: i64) -> Result<(), rusqlite::Error> {
    let row: Option<(String, Option<i64>)> = conn
        .query_row(
            "SELECT path, device FROM tracks WHERE id = ?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let reason = match &row {
        Some((path, device)) => crate::library::mounts::classify_missing(*device, Path::new(path)),
        None => MissingReason::Unknown,
    };
    conn.execute(
        "UPDATE tracks SET missing_since = strftime('%s','now'), missing_reason = ?2 \
         WHERE id = ?1",
        rusqlite::params![track_id, reason.as_str()],
    )?;
    Ok(())
}

/// "Remove from library" (Stage 3 Task 8's Missing-source action): deletes
/// `track_id`'s row outright. This is a DATABASE-ONLY delete — it never
/// touches the file on disk, which is the whole point of the app's "we never
/// delete your files" promise; there is nothing to delete here anyway, since
/// this action only exists for a track already flagged `missing` (the file
/// is already gone from disk by definition).
///
/// The `WHERE ... AND` `MISSING` guard is a defensive belt-and-braces
/// check, not just `WHERE id = ?1`: it makes this call a no-op (`Ok(false)`)
/// against a track that somehow isn't actually missing any more (e.g. a
/// rescan raced ahead of a stale Missing-view selection and restored the
/// file), rather than silently deleting a live library row's history because
/// the UI's idea of "this row is missing" was one reload out of date.
/// Returns whether a row was actually deleted.
///
/// This lone-row primitive does NOT renumber any playlist the deleted track
/// belonged to, or purge it from the playback queue — see [`remove_missing_
/// tracks`]'s doc comment for the cross-task invariant a bare delete like
/// this one breaks (`playlist_tracks.position` gaps -> `library::playlists::
/// move_position` moving the wrong row; a phantom id left in `queue::
/// Queue`). Every real caller should go through `remove_missing_tracks`
/// instead — this function is kept for the tests that pin its own no-op
/// guard in isolation. The batch path now shares `remove_tracks_impl` with
/// the explicit live-row removal API rather than calling this wrapper.
pub fn remove_missing_track(conn: &Connection, track_id: i64) -> Result<bool, rusqlite::Error> {
    let deleted = conn.execute(
        &format!("DELETE FROM tracks WHERE id = ?1 AND {MISSING}"),
        rusqlite::params![track_id],
    )?;
    Ok(deleted > 0)
}

/// Batch, TRANSACTIONAL "Remove from library" (Stage-3 close-out fix): the
/// version every real caller (`ui::track_actions::remove_missing_selected`)
/// uses instead of looping over [`remove_missing_track`] directly — the
/// difference matters. A bare per-id `remove_missing_track` loop deletes
/// each `tracks` row but leaves two cross-task invariants broken behind it
/// (this is the bug this function's introduction fixes; Task 3's own
/// `playlist_tracks`-related comments in this module documented reliance on
/// "nothing hard-deletes a `tracks` row" — see the two `invariant:` comments
/// this task updated, on `query_track_window_queue` and `query_track_count`):
///
/// - `playlist_tracks` has `ON DELETE CASCADE` (`db.rs`), so the deleted
///   track's row in every playlist it belonged to disappears too — WITHOUT
///   renumbering the survivors, leaving gapped positions (e.g. `[0,1,3,4]`).
///   `library::playlists::move_position` treats a position as a literal
///   `Vec` index (`tracks.remove(from as usize)`), an assumption that only
///   holds while positions stay gapless — a gap makes a later drag-reorder
///   silently move the wrong row (the Task 5 wrong-row bug class, through a
///   side door this task closes).
/// - A track hard-deleted while sitting in the playback queue leaves its id
///   there as a phantom (`queue::Queue` has no delete-awareness of its own).
///   This function only owns the DB side of the fix; the caller is
///   responsible for purging the queue too, using this function's return
///   value — see `ui::player_controller::PlayerController::purge_queue_ids`,
///   invoked from `ui::track_list_context_menu::handle_remove_from_library`.
///
/// Every id's delete, and every playlist position renumber
/// (`library::playlists::renumber_positions`) it triggers, happens inside
/// ONE transaction — all ids succeed/fail together, so a crash/error
/// partway through a multi-id remove can never leave a playlist's positions
/// gapped in the committed database. Affected playlists are looked up
/// *before* each delete (the FK cascade is about to remove those very
/// `playlist_tracks` rows). Returns the ids actually deleted — a subset of
/// `ids`, in input order; an id that wasn't/isn't-anymore missing is
/// silently skipped (matching `remove_missing_track`'s own no-op contract),
/// not an error. A no-op (`Ok(vec![])`, no transaction opened) for an empty
/// `ids` slice.
pub fn remove_missing_tracks(
    conn: &mut Connection,
    ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    remove_tracks_impl(conn, ids, RemoveGuard::MissingOnly)
}

/// Removes every row currently marked missing from the library database.
/// This is the bulk counterpart to [`remove_missing_tracks`]: it never
/// touches media files, preserves live rows, compacts affected playlists in
/// the same transaction, and returns the exact ids the caller must purge
/// from its playback queue. The stable id order keeps callback behavior and
/// tests deterministic.
pub fn remove_all_missing_tracks(conn: &mut Connection) -> Result<Vec<i64>, rusqlite::Error> {
    let ids = {
        let mut statement = conn.prepare(&format!(
            "SELECT id FROM tracks WHERE {MISSING} ORDER BY id"
        ))?;
        let ids = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        ids
    };
    remove_missing_tracks(conn, &ids)
}

/// Explicit, DATABASE-ONLY removal for live or missing tracks. This never
/// touches a media file. Like [`remove_missing_tracks`], it deletes and
/// compacts every affected playlist in one transaction and returns the
/// unique ids actually removed in first-input order. The UI must separately
/// purge these exact ids from its playback queue.
pub fn remove_tracks(conn: &mut Connection, ids: &[i64]) -> Result<Vec<i64>, rusqlite::Error> {
    remove_tracks_impl(conn, ids, RemoveGuard::Any)
}

/// Which rows `remove_tracks_impl`'s per-id `DELETE` is allowed to actually
/// touch — an extra condition appended to `WHERE id = ?1`, re-checked at
/// delete time rather than trusted from whatever snapshot the caller
/// selected `ids` from. `Any` is the explicit live-or-missing removal API
/// (`remove_tracks`, no extra guard). `MissingOnly` is `remove_missing_
/// track[s]`'s belt-and-braces check against a row that raced back to
/// present since the caller's selection (see `remove_missing_track`'s doc
/// comment). `TombstonedOnly` is `purge_tombstones`'s guard against the
/// mirror race on the other side of the same problem: the scanner's
/// resurrect-on-evidence write (`library::watcher`, its own OS thread and
/// `rusqlite::Connection`, racing the purge's connection under WAL) can
/// clear a row's `removed_at` between `purge_tombstones`'s own `SELECT` and
/// this loop reaching that id — without this guard the `DELETE` would still
/// fire on a row that is, as of the moment it matters, no longer
/// tombstoned, hard-deleting a live track's playlist history irrecoverably.
/// `AutoCleanEligible` is [`remove_auto_clean_eligible_tracks`]'s guard for
/// the same shape of race on `queries::issues::run_auto_clean`'s unattended
/// delete — see that function's doc comment for the full race description
/// and why only the missing-state/reason half of eligibility needs
/// re-checking here, not the deadline arithmetic.
#[derive(Clone, Copy)]
pub(crate) enum RemoveGuard {
    Any,
    MissingOnly,
    TombstonedOnly,
    AutoCleanEligible,
}

pub(crate) fn remove_tracks_impl(
    conn: &mut Connection,
    ids: &[i64],
    guard: RemoveGuard,
) -> Result<Vec<i64>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let tx = conn.transaction()?;
    let mut removed = Vec::with_capacity(ids.len());
    for &id in ids {
        let mut stmt =
            tx.prepare("SELECT DISTINCT playlist_id FROM playlist_tracks WHERE track_id = ?1")?;
        let affected_playlists: Vec<i64> = stmt
            .query_map(rusqlite::params![id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);

        let deleted = match guard {
            RemoveGuard::Any => {
                tx.execute("DELETE FROM tracks WHERE id = ?1", rusqlite::params![id])?
            }
            RemoveGuard::MissingOnly => tx.execute(
                &format!("DELETE FROM tracks WHERE id = ?1 AND {MISSING}"),
                rusqlite::params![id],
            )?,
            RemoveGuard::TombstonedOnly => tx.execute(
                "DELETE FROM tracks WHERE id = ?1 AND removed_at IS NOT NULL",
                rusqlite::params![id],
            )?,
            RemoveGuard::AutoCleanEligible => tx.execute(
                &format!("DELETE FROM tracks WHERE id = ?1 AND {MISSING} AND missing_reason = ?2"),
                rusqlite::params![id, MissingReason::Deleted.as_str()],
            )?,
        };
        if deleted == 0 {
            continue;
        }
        removed.push(id);
        for playlist_id in affected_playlists {
            playlists::renumber_positions(&tx, playlist_id)?;
        }
    }
    tx.commit()?;
    Ok(removed)
}

/// Auto-clean's own DATABASE-ONLY removal (Finding 1, review pass on Task
/// 2.3): the guarded version `queries::issues::run_auto_clean` calls instead
/// of the unguarded [`remove_tracks`]. `run_auto_clean` selects eligible ids
/// via `auto_clean_eligible`, then hands them here — this function re-checks
/// each id's eligibility (still missing, still `missing_reason = 'deleted'`)
/// at delete time via [`RemoveGuard::AutoCleanEligible`], closing the same
/// shape of TOCTOU window [`purge_tombstones`]'s `TombstonedOnly` guard
/// closes: the scanner/watcher runs on its own OS thread with its own
/// `rusqlite::Connection`, a genuine concurrent writer under this database's
/// WAL mode (see the tombstone section header comment below), and can
/// resurrect a selected id — the file reappeared, so the row is legitimately
/// live again — in the window between `auto_clean_eligible`'s `SELECT` and
/// this loop reaching that id's `DELETE`. Without this guard the row would
/// be hard-deleted anyway, cascading away a live track's rating, playlist
/// membership and listening history with no undo — auto-clean runs
/// completely unattended, so there is nobody watching to notice and nothing
/// left to restore.
///
/// The guard only re-checks what can realistically change under it: the
/// row's missing state and reason. It deliberately does NOT re-run the
/// `days`/`armed_at` deadline arithmetic — time only moves forward, so an id
/// that was already past its deadline at `auto_clean_eligible`'s `SELECT` is
/// still past it by the time this `DELETE` runs; re-deriving the same
/// monotonic fact here would be redundant, not safer, so do not "fix" this
/// by adding it back.
pub(crate) fn remove_auto_clean_eligible_tracks(
    conn: &mut Connection,
    ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    remove_tracks_impl(conn, ids, RemoveGuard::AutoCleanEligible)
}

// -- Tombstone operations (Task 2.2, 10-second undo) ---------------------
//
// The Missing source's "Remove all N from library" action needs an undo
// window, and a hard delete cannot honestly offer one. `remove_tracks`
// (above) is a real, immediate, irreversible delete — correct for its own
// callers, but the wrong primitive for a "Remove" the user might click
// "Undo" on ten seconds later.
//
// A snapshot-and-restore undo (save the row, delete it, re-insert on undo)
// is the obvious design and is DISQUALIFIED, not merely inelegant:
// `tracks.id` is a plain `INTEGER PRIMARY KEY` with no `AUTOINCREMENT`, so
// SQLite reuses `max(id)+1` for the next insert. Deleting the
// highest-numbered row and then having a scan (the folder watcher fires on
// its own, independent of any UI action) insert a new track during the
// 10-second window would hand that brand-new track the exact id the undo
// is about to try to reclaim — the undo then either collides or, worse,
// silently grafts the deleted row's rating/play history/playlist
// membership onto a completely unrelated file. An undo that can race the
// watcher into corrupting unrelated data is not an undo.
//
// The tombstone avoids the race by never freeing the id in the first
// place: `tombstone_tracks` only sets `removed_at`, so the row — and every
// FK-cascaded child row that depends on it (`playlist_tracks` membership
// AND position, `listen_events`, `device_files`) — stays exactly where it
// is for the whole window. `undo_tombstone` is then just clearing that one
// column back to `NULL`; there is nothing to restore because nothing was
// ever lost. `purge_tombstones` is the one place a tombstone finally
// becomes the real, `remove_tracks`-powered delete, once the window has
// closed without an undo.
//
// Tests for this section live in `tests_issues.rs`, not the `tests_
// maintenance.rs` this file's other functions are covered by — see that
// file's own module doc comment for why (`tests_maintenance.rs` was already
// close to the project's 800-line rule; the functions themselves stayed
// here, only the tests moved).

/// "Remove from library" for the tombstone/10-second-undo flow: marks every
/// id in `ids` as removed by setting `removed_at = now`, without touching
/// anything else. See this section's header comment for why a tombstone
/// (not a snapshot-and-delete) is the only race-free way to offer an undo
/// here.
///
/// `PRESENT`/`MISSING` both require `removed_at IS NULL`, so a
/// tombstoned row disappears from every view (Library, Missing, Playlist,
/// Smart, …) — including the very Missing card it was just removed from —
/// the instant this call returns, with zero rows actually deleted: no
/// cascade fires, so `playlist_tracks` membership/position, `listen_
/// events`, and `device_files` all survive untouched for the whole undo
/// window.
///
/// The `removed_at IS NULL` guard in the `WHERE` clause makes re-
/// tombstoning an already-tombstoned id a no-op that keeps its ORIGINAL
/// timestamp rather than overwriting it — a second "Remove" click on a row
/// already mid-countdown must not silently restart the toast's 10-second
/// timer.
///
/// Returns the number of rows actually tombstoned — a subset of `ids.len()`
/// when some ids were already tombstoned or don't exist. `Ok(0)` for an
/// empty `ids` slice, no query issued.
///
/// Builds one `WHERE id IN (?2,?3,…)` placeholder per id rather than looping
/// per id like every other bulk-id function in this file — a deliberate,
/// narrow exception. A "Remove all N" click is the one caller here that can
/// plausibly pass a large `ids` batch in one call, and SQLite's bound-
/// parameter ceiling (`SQLITE_MAX_VARIABLE_NUMBER`, 32766 by default) is far
/// beyond any realistic library removal batch, so the single-statement form
/// is safe in practice; [`undo_tombstone`] below mirrors the same shape for
/// the same reason.
pub fn tombstone_tracks(
    conn: &Connection,
    ids: &[i64],
    now: i64,
) -> Result<usize, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = (2..=ids.len() + 1)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "UPDATE tracks SET removed_at = ?1 WHERE id IN ({placeholders}) AND removed_at IS NULL"
    );
    let params: Vec<i64> = std::iter::once(now).chain(ids.iter().copied()).collect();
    conn.execute(&sql, rusqlite::params_from_iter(params))
}

/// Reverses [`tombstone_tracks`] within the undo window: clears `removed_at`
/// on every id in `ids` that is currently tombstoned, restoring it to
/// whatever presence view (`PRESENT`/`MISSING`) its `missing_since` already
/// says it belongs to. There is nothing to "restore" beyond that one
/// column — `tombstone_tracks` never deleted the row, so no data was ever
/// lost.
///
/// The `removed_at IS NOT NULL` guard makes undoing an id that isn't (or is
/// no longer — e.g. resurrected by a scan in the meantime, see `purge_
/// tombstones`'s doc comment) tombstoned a no-op rather than an error.
///
/// Returns the number of rows actually restored. `Ok(0)` for an empty `ids`
/// slice, no query issued.
pub fn undo_tombstone(conn: &Connection, ids: &[i64]) -> Result<usize, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = (1..=ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "UPDATE tracks SET removed_at = NULL WHERE id IN ({placeholders}) AND removed_at IS NOT NULL"
    );
    conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
}

/// Hard-deletes every currently-tombstoned row: the real, irreversible
/// delete a tombstone eventually becomes once its undo window has closed.
/// This has exactly two callers — the toast's 10-second timeout, and app
/// startup (a quit that happens *inside* the undo window must commit the
/// removal on the next launch rather than silently rolling it back, so the
/// count the user read — "7 removed" — stays true; the startup call site
/// is wired by a later task). Both funnel through this one function so
/// there is exactly one place a tombstone turns into a real delete.
///
/// Deliberately reuses the same shared deletion path as [`remove_tracks`]
/// (both funnel through `remove_tracks_impl`) rather than re-implementing
/// deletion: that path already gets the hard part right inside one
/// transaction — every affected playlist's positions compacted, the FK
/// cascades (`playlist_tracks`, `listen_events`, `device_files`) fired, and
/// the exact ids the caller must purge from its own in-memory playback
/// queue returned. Selecting the tombstoned ids and handing them to
/// `remove_tracks_impl` keeps there being exactly one deletion path — no
/// risk of this one drifting from that one over time.
///
/// Interop with the scanner's resurrect-on-evidence behavior (`library::
/// scanner`/`library::watcher`, which runs on its own OS thread with its
/// own `rusqlite::Connection` — a genuine concurrent writer under this
/// database's WAL mode, not a hypothetical): a row the scanner finds is
/// still there gets its `removed_at` cleared back to `NULL` the moment
/// that's discovered — a "Remove" whose object came back is moot. This
/// function's own `SELECT` above and its `DELETE` below are NOT one atomic
/// transaction, so a resurrection can land in the gap between them — after
/// an id is captured in `ids` here, but before `remove_tracks_impl`'s loop
/// reaches that id's `DELETE`. `remove_tracks_impl` is therefore called
/// with `RemoveGuard::TombstonedOnly`, which re-checks `removed_at IS NOT
/// NULL` at delete time rather than trusting this snapshot — a row
/// resurrected mid-purge is simply not deleted by that guarded statement,
/// surviving with its playlist membership and listen history intact. See
/// `tests_issues.rs`'s `purge_tombstones_survives_a_resurrection_racing_
/// the_delete_itself` for the regression test this guards against (as
/// opposed to the pre-existing `purge_tombstones_skips_a_row_resurrected_
/// before_the_purge_runs`, which only covers a resurrection that lands
/// *before* this function is even called — the easy case).
pub fn purge_tombstones(conn: &mut Connection) -> Result<Vec<i64>, rusqlite::Error> {
    let ids = {
        let mut statement =
            conn.prepare("SELECT id FROM tracks WHERE removed_at IS NOT NULL ORDER BY id")?;
        let ids = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        ids
    };
    remove_tracks_impl(conn, &ids, RemoveGuard::TombstonedOnly)
}

/// Bare count of rows in `import_errors` (the last scan's import failures) —
/// see the module doc's `ImportErrors` section for why this is the only
/// piece of that source this task builds. Used by `ui::sidebar` (Task 4) for
/// the "Import errors" badge count.
pub fn query_import_error_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
}

/// Loads every `import_errors` row, most recent first (`last_seen DESC`,
/// falling back to `path DESC` for same-second ties so the ordering is
/// deterministic — schema v10 (Task 1.1) made `path` the table's primary
/// key, so it replaces the old surrogate `id` as the tie-break column) —
/// capped at `QUEUE_LIMIT` for the same defense-in-depth reason every other
/// unbounded list query in this module is.
pub fn query_import_errors(conn: &Connection) -> Result<Vec<ImportErrorRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT path, reason_detail, last_seen FROM import_errors \
         ORDER BY last_seen DESC, path DESC LIMIT {QUEUE_LIMIT}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(ImportErrorRow {
            path: r.get(0)?,
            reason: r.get(1)?,
            occurred_at: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// "Dismiss" action (Stage 3 Task 8's ImportErrors source): deletes one
/// `import_errors` row by its `path` — the table's primary key since schema
/// v10 (Task 1.1). This never touches `tracks` or any file on disk — it
/// only clears the recorded failure itself.
pub fn delete_import_error(conn: &Connection, path: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM import_errors WHERE path = ?1",
        rusqlite::params![path],
    )?;
    Ok(())
}

/// Dismisses every recorded import failure and returns the number cleared.
/// The diagnostic table is independent of `tracks`; no library row or media
/// file is touched.
pub fn delete_all_import_errors(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.execute("DELETE FROM import_errors", [])
}

/// Looks up a track's id by its exact, parameterized `path` (Stage 3 Task 7:
/// M3U import matches each parsed/resolved path line against this). `None`
/// if no track has that exact path — not an error; the caller (`ui::
/// playlist_io::import_playlist`) treats an unmatched path as "not found",
/// counted but not added.
pub fn track_id_for_path(conn: &Connection, path: &str) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT id FROM tracks WHERE path = ?1",
        rusqlite::params![path],
        |r| r.get(0),
    )
    .optional()
}
