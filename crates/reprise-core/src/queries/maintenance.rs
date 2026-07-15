//! Track/import-error maintenance queries: missing-file marking and hard
//! delete, import-error triage, path/summary lookups. Split out of the
//! former single-file `queries.rs` (Refactoring & Extensibility Task 1) — a
//! pure move, no behavior change.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::device_sync::SyncTrack;
use crate::library::playlists;
use rusqlite::{Connection, OptionalExtension};

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

/// Resolves a drag payload into copy-ready tracks without trusting stale UI
/// metadata. Input order is preserved, repeated ids are emitted once, and
/// rows that are unknown, marked missing, or no longer regular local files
/// are omitted. The file size is read at enqueue time so progress totals
/// describe the bytes that will actually be copied.
pub fn query_sync_tracks(
    conn: &Connection,
    ids: &[i64],
) -> Result<Vec<SyncTrack>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT path,title,artist,duration_ms FROM tracks WHERE id = ?1 AND missing = 0",
    )?;
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
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()?;
        let Some((path, title, artist, duration_ms)) = row else {
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
            duration_ms,
            size_bytes: metadata.len(),
        });
    }
    Ok(tracks)
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

/// "Remove from library" (Stage 3 Task 8's Missing-source action): deletes
/// `track_id`'s row outright. This is a DATABASE-ONLY delete — it never
/// touches the file on disk, which is the whole point of the app's "we never
/// delete your files" promise; there is nothing to delete here anyway, since
/// this action only exists for a track already flagged `missing` (the file
/// is already gone from disk by definition).
///
/// The `WHERE ... AND missing = 1` guard is a defensive belt-and-braces
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
        "DELETE FROM tracks WHERE id = ?1 AND missing = 1",
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
    remove_tracks_impl(conn, ids, true)
}

/// Removes every row currently marked missing from the library database.
/// This is the bulk counterpart to [`remove_missing_tracks`]: it never
/// touches media files, preserves live rows, compacts affected playlists in
/// the same transaction, and returns the exact ids the caller must purge
/// from its playback queue. The stable id order keeps callback behavior and
/// tests deterministic.
pub fn remove_all_missing_tracks(conn: &mut Connection) -> Result<Vec<i64>, rusqlite::Error> {
    let ids = {
        let mut statement = conn.prepare("SELECT id FROM tracks WHERE missing = 1 ORDER BY id")?;
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
    remove_tracks_impl(conn, ids, false)
}

fn remove_tracks_impl(
    conn: &mut Connection,
    ids: &[i64],
    missing_only: bool,
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

        let deleted = if missing_only {
            tx.execute(
                "DELETE FROM tracks WHERE id = ?1 AND missing = 1",
                rusqlite::params![id],
            )?
        } else {
            tx.execute("DELETE FROM tracks WHERE id = ?1", rusqlite::params![id])?
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

/// Bare count of rows in `import_errors` (the last scan's import failures) —
/// see the module doc's `ImportErrors` section for why this is the only
/// piece of that source this task builds. Used by `ui::sidebar` (Task 4) for
/// the "Import errors" badge count.
pub fn query_import_error_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
}

/// Loads every `import_errors` row, most recent first (`occurred_at DESC`,
/// falling back to `id DESC` for same-second ties so the ordering is
/// deterministic) — capped at `QUEUE_LIMIT` for the same defense-in-depth
/// reason every other unbounded list query in this module is.
pub fn query_import_errors(conn: &Connection) -> Result<Vec<ImportErrorRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT id, path, reason, occurred_at FROM import_errors \
         ORDER BY occurred_at DESC, id DESC LIMIT {QUEUE_LIMIT}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(ImportErrorRow {
            id: r.get(0)?,
            path: r.get(1)?,
            reason: r.get(2)?,
            occurred_at: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// "Dismiss" action (Stage 3 Task 8's ImportErrors source): deletes one
/// `import_errors` row by id. This never touches `tracks` or any file on
/// disk — it only clears the recorded failure itself.
pub fn delete_import_error(conn: &Connection, id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM import_errors WHERE id = ?1",
        rusqlite::params![id],
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
