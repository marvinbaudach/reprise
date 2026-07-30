//! Test coverage for the 18a missing-file group queries (`query_missing_
//! groups`/`query_missing_rows`, Task 2.1) — split into its own file rather
//! than `tests_maintenance.rs` (already close to the project's 800-line
//! rule) purely for size, same reasoning as `tests.rs`'s own doc comment.
//!
//! Covers exactly the four behaviors the task brief calls out: two distinct
//! mount points among `unmounted` rows become two separate `Unavailable`
//! groups; the `unknown` reason forms its own group rather than joining
//! either `unmounted` or `deleted`; the `Deleted` group's count never
//! includes `unknown` rows; and `query_missing_rows` paginates via
//! `LIMIT`/`OFFSET` in `artist, album, track_no` order.
//!
//! The trailing `-- Tombstone operations --` section (Task 2.2) covers
//! `tombstone_tracks`/`undo_tombstone`/`purge_tombstones` — the 10-second-
//! undo primitives behind the Missing source's "Remove all N from library"
//! action. Landed here rather than in `tests_maintenance.rs` (the brief's
//! literal file assignment) purely because that file is already close to
//! the project's 800-line rule and these four tests, at this codebase's doc
//! density, would have pushed it over; `maintenance.rs` itself (where the
//! three functions actually live — see that file's own tombstone section)
//! had plenty of headroom, so only the tests moved.

use super::*;
use crate::library::playlists;
use crate::models::MissingReason;

/// Inserts one missing track row with every column `query_missing_groups`/
/// `query_missing_rows` reads or groups by. `id` doubles as the row's
/// `path`/`title` disambiguator so callers can seed many rows tersely
/// without naming each field.
#[allow(clippy::too_many_arguments)]
fn seed_missing_track(
    conn: &Connection,
    id: i64,
    artist: &str,
    album: &str,
    track_no: Option<i32>,
    reason: MissingReason,
    mount_point: Option<&str>,
) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at, \
         missing_since, missing_reason, mount_point) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 1, ?7, ?8)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            artist,
            album,
            track_no,
            reason.as_str(),
            mount_point,
        ],
    )
    .unwrap();
}

/// Bullet 1 of the brief: 2 mount points among `unmounted` rows must become
/// 2 separate `Unavailable` groups, never one mixed card — and they come
/// back sorted by mount point (case-insensitively), matching the 18a "one
/// card per drive" requirement.
#[test]
fn two_unmounted_drives_produce_two_unavailable_groups_sorted_by_mount() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(
        conn,
        1,
        "A",
        "Alpha",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb-b"),
    );
    seed_missing_track(
        conn,
        2,
        "B",
        "Beta",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb-a"),
    );
    seed_missing_track(
        conn,
        3,
        "C",
        "Gamma",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb-a"),
    );

    let groups = query_missing_groups(&db).unwrap();

    assert_eq!(
        groups,
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: Some("/media/usb-a".to_string())
                },
                track_count: 2,
            },
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: Some("/media/usb-b".to_string())
                },
                track_count: 1,
            },
        ]
    );
}

/// Bullet 2: `unknown` rows must never be silently folded into an
/// `unmounted` mount group or into `Deleted` — they form their own
/// `Unavailable { mount_point: None }` group, ordered after every per-mount
/// group (18a card order: per-mount, then unknown, then deleted).
#[test]
fn unknown_reason_forms_its_own_actionless_group_after_unavailable_groups() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(
        conn,
        1,
        "A",
        "Alpha",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb"),
    );
    seed_missing_track(conn, 2, "B", "Beta", Some(1), MissingReason::Unknown, None);
    seed_missing_track(conn, 3, "C", "Gamma", Some(1), MissingReason::Unknown, None);

    let groups = query_missing_groups(&db).unwrap();

    assert_eq!(
        groups,
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: Some("/media/usb".to_string())
                },
                track_count: 1,
            },
            MissingGroup {
                kind: MissingGroupKind::Unavailable { mount_point: None },
                track_count: 2,
            },
        ]
    );
}

/// Bullet 3, and the load-bearing invariant from Beschluss 1: the `Deleted`
/// group's count is `missing_reason = 'deleted'` ONLY. A DB with both
/// `deleted` and `unknown` rows must report `Deleted`'s count as exactly
/// the `deleted` rows — never the two reasons' combined total — since that
/// count backs a bulk hard-delete action.
#[test]
fn deleted_group_count_never_includes_unknown_reason_rows() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(conn, 1, "A", "Alpha", Some(1), MissingReason::Deleted, None);
    seed_missing_track(conn, 2, "B", "Beta", Some(1), MissingReason::Unknown, None);
    seed_missing_track(conn, 3, "C", "Gamma", Some(1), MissingReason::Unknown, None);

    let groups = query_missing_groups(&db).unwrap();

    assert_eq!(
        groups,
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable { mount_point: None },
                track_count: 2,
            },
            MissingGroup {
                kind: MissingGroupKind::Deleted,
                track_count: 1,
            },
        ]
    );
}

/// Bullet 4: `query_missing_rows` pages via `LIMIT`/`OFFSET` and orders by
/// `artist, album, track_no` (`COLLATE NOCASE` on the text columns) — the
/// same shape the rest of the library sorts by. Four `deleted` rows seeded
/// out of order; two 2-row pages must reconstruct the sorted sequence.
#[test]
fn missing_rows_are_paginated_and_ordered_by_artist_album_track_no() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(
        conn,
        1,
        "Zeta",
        "Album",
        Some(1),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(
        conn,
        2,
        "Alpha",
        "Second",
        Some(2),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(
        conn,
        3,
        "Alpha",
        "First",
        Some(1),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(
        conn,
        4,
        "alpha",
        "First",
        Some(0),
        MissingReason::Deleted,
        None,
    );

    let first_page = query_missing_rows(&db, &MissingGroupKind::Deleted, 0, 2).unwrap();
    let second_page = query_missing_rows(&db, &MissingGroupKind::Deleted, 2, 2).unwrap();

    assert_eq!(
        first_page.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![4, 3]
    );
    assert_eq!(
        second_page.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![2, 1]
    );
}

/// Finding 1 (Important): `query_missing_rows` for a specific mount point
/// must return ONLY that mount's rows, never other mounts, never `unknown`
/// rows, never `deleted` rows. The per-mount branch (the `mount_filter` SQL
/// clause and 4th bound parameter in `query_missing_rows`) has no direct
/// test; seeding two distinct mount points and filtering for one must
/// return exactly one row per mount, and querying the other mount must
/// return its own different row — proving the isolation that the 18a "one
/// card per drive" feature depends on.
#[test]
fn per_mount_rows_query_isolates_to_the_requested_mount_point() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    // Two rows on mount A
    seed_missing_track(
        conn,
        1,
        "Alice",
        "AlbumA",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/nas-a"),
    );
    seed_missing_track(
        conn,
        2,
        "Bob",
        "AlbumB",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/nas-a"),
    );
    // One row on mount B (different mount point)
    seed_missing_track(
        conn,
        3,
        "Charlie",
        "AlbumC",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/nas-b"),
    );
    // One `unknown` row (no mount point)
    seed_missing_track(
        conn,
        4,
        "Dave",
        "AlbumD",
        Some(1),
        MissingReason::Unknown,
        None,
    );
    // One `deleted` row
    seed_missing_track(
        conn,
        5,
        "Eve",
        "AlbumE",
        Some(1),
        MissingReason::Deleted,
        None,
    );

    // Query for mount A only; must return exactly the two rows on mount A
    let mount_a_rows = query_missing_rows(
        &db,
        &MissingGroupKind::Unavailable {
            mount_point: Some("/media/nas-a".into()),
        },
        0,
        100,
    )
    .unwrap();
    assert_eq!(
        mount_a_rows.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![1, 2]
    );

    // Query for mount B; must return only the one row on mount B
    let mount_b_rows = query_missing_rows(
        &db,
        &MissingGroupKind::Unavailable {
            mount_point: Some("/media/nas-b".into()),
        },
        0,
        100,
    )
    .unwrap();
    assert_eq!(
        mount_b_rows.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![3]
    );
}

/// Finding 2 (Minor): present tracks (missing_since IS NULL, i.e., not in
/// any missing group) must not appear in either `query_missing_groups` or
/// `query_missing_rows`. This is a sanity check on the `MISSING` predicate
/// constant being correct and consistently applied. The test seeds one
/// present track alongside missing tracks, then verifies that neither query
/// returns it.
#[test]
fn present_tracks_are_excluded_from_missing_queries() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    // One present track (missing_since = NULL, the default)
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        rusqlite::params![
            99,
            "/music/present.flac",
            "Present Track",
            "Present",
            "Album",
            1
        ],
    )
    .unwrap();
    // One missing (deleted) track for comparison
    seed_missing_track(
        conn,
        1,
        "Missing",
        "Album",
        Some(1),
        MissingReason::Deleted,
        None,
    );

    // `query_missing_groups` must include only the deleted row's group
    let groups = query_missing_groups(&db).unwrap();
    assert_eq!(
        groups,
        vec![MissingGroup {
            kind: MissingGroupKind::Deleted,
            track_count: 1,
        }]
    );

    // `query_missing_rows` for Deleted must return only the missing row
    let rows = query_missing_rows(&db, &MissingGroupKind::Deleted, 0, 100).unwrap();
    assert_eq!(rows.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1]);
}

/// Finding 3 (Minor): when there are no missing tracks at all,
/// `query_missing_groups` must return an empty `Vec`. This is the state that
/// makes the sidebar's ISSUES section disappear, so it is important to
/// pin explicitly.
#[test]
fn empty_missing_groups_when_no_missing_tracks() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    // Insert one present track only
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        rusqlite::params![1, "/music/track.flac", "Track", "Artist", "Album", 1],
    )
    .unwrap();

    let groups = query_missing_groups(&db).unwrap();
    assert!(groups.is_empty());
}

// -- Tombstone operations (Task 2.2) -------------------------------------

/// Inserts one ordinary, non-missing track row — the minimal fixture the
/// tombstone tests need (unlike `seed_missing_track`, these rows start
/// `PRESENT`; `tombstone_tracks` is exercised on top of that starting
/// state).
fn seed_live_track(conn: &Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (?1, ?2, ?3, 'Artist', 'Album', 1, 0)",
        rusqlite::params![id, format!("/music/{id}.flac"), format!("Track {id}")],
    )
    .unwrap();
}

fn removed_at_of(conn: &Connection, id: i64) -> Option<i64> {
    conn.query_row("SELECT removed_at FROM tracks WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .unwrap()
}

/// Bullet 1 of the brief: `tombstone_tracks` must hide the row from every
/// presence-based query (here, `query_live_track_ids`, `PRESENT`-backed)
/// while leaving `playlist_tracks` completely untouched — no cascade fires,
/// because nothing was deleted, only `removed_at` was set. This is the
/// crux the module doc's tombstone-vs-snapshot rationale rests on: if this
/// left the row deleted (even briefly), `playlist_tracks`'s `ON DELETE
/// CASCADE` would already have destroyed the membership/position row this
/// test asserts survives.
#[test]
fn tombstone_tracks_hides_rows_but_keeps_playlist_membership_and_position() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    for id in 1..=3 {
        seed_live_track(conn, id);
    }
    let playlist_id = playlists::create(&db, "Keep").unwrap();
    playlists::add_tracks(&db, playlist_id, &[1, 2, 3]).unwrap();

    let changed = tombstone_tracks(&db, &[2], 1_000).unwrap();
    assert_eq!(changed, 1);

    // Hidden from the library view immediately.
    assert_eq!(
        query_live_track_ids(&db).unwrap(),
        std::collections::HashSet::from([1, 3]),
        "a tombstoned row must disappear from PRESENT queries at once"
    );
    assert_eq!(removed_at_of(conn, 2), Some(1_000));

    // Playlist membership AND position are untouched — this is the whole
    // point of a tombstone over a hard delete-then-restore.
    let rows: Vec<(i64, i64)> = conn
        .prepare("SELECT track_id, position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map([playlist_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![(1, 0), (2, 1), (3, 2)],
        "tombstoning must not renumber or remove any playlist_tracks row"
    );

    // Re-tombstoning an already-tombstoned id is a no-op (the guard is
    // `removed_at IS NULL`) — the toast's countdown must not reset itself
    // if the user clicks "Remove" again during the undo window.
    assert_eq!(tombstone_tracks(&db, &[2], 2_000).unwrap(), 0);
    assert_eq!(
        removed_at_of(conn, 2),
        Some(1_000),
        "a second tombstone call must not overwrite the original timestamp"
    );
}

/// Bullet 2: `undo_tombstone` reverses a still-open tombstone with zero data
/// loss — there was never anything to restore, since nothing was deleted.
/// Also pins the no-op guard: undoing an id that was never tombstoned (or
/// is no longer) changes nothing.
#[test]
fn undo_tombstone_clears_removed_at_and_restores_presence() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_live_track(conn, 1);
    seed_live_track(conn, 2);
    tombstone_tracks(&db, &[1], 500).unwrap();
    assert_eq!(
        query_live_track_ids(&db).unwrap(),
        std::collections::HashSet::from([2])
    );

    let restored = undo_tombstone(&db, &[1]).unwrap();
    assert_eq!(restored, 1);
    assert_eq!(removed_at_of(conn, 1), None);
    assert_eq!(
        query_live_track_ids(&db).unwrap(),
        std::collections::HashSet::from([1, 2]),
        "undo must make the row visible again immediately"
    );

    // No-op guard: id 2 was never tombstoned.
    assert_eq!(undo_tombstone(&db, &[2]).unwrap(), 0);
}

/// Bullet 3: `purge_tombstones` is where a tombstone finally becomes
/// irreversible — it must select every currently-tombstoned id and hand
/// them to `remove_tracks_impl` (not reimplement deletion), so a MIDDLE
/// playlist row's removal still compacts positions gaplessly, exactly like
/// `remove_missing_tracks`'s own middle-row regression test in
/// `tests_maintenance.rs`. The returned ids are what the caller (toast
/// timeout / app startup) must purge from its in-memory playback queue.
#[test]
fn purge_tombstones_hard_deletes_tombstoned_rows_compacts_playlist_and_returns_purged_ids() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    for id in 1..=5 {
        seed_live_track(conn, id);
    }
    let playlist_id = playlists::create(&db, "Purge").unwrap();
    playlists::add_tracks(&db, playlist_id, &[1, 2, 3, 4, 5]).unwrap();
    tombstone_tracks(&db, &[3], 1_000).unwrap();

    let purged = purge_tombstones(&db).unwrap();
    assert_eq!(purged, vec![3]);

    let track_count: i64 = conn
        .query_row("SELECT count(*) FROM tracks WHERE id = 3", [], |r| r.get(0))
        .unwrap();
    assert_eq!(track_count, 0, "the tombstoned row must be hard-deleted");

    let rows: Vec<(i64, i64)> = conn
        .prepare("SELECT track_id, position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map([playlist_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![(1, 0), (2, 1), (4, 2), (5, 3)],
        "positions must stay gapless after the purge's hard delete"
    );

    // Idempotent: nothing left to purge.
    assert!(purge_tombstones(&db).unwrap().is_empty());
}

/// Bullet 4, the scanner-resurrect interop test: a row tombstoned and then
/// resurrected (the scanner's evidence rule clears `removed_at` the instant
/// it finds the file again — `library::scanner`'s tombstone-resurrect arms,
/// covered end-to-end in `scanner_tombstone_tests.rs`) BEFORE `purge_
/// tombstones` runs must not be purged: a "Remove" whose object came back is
/// moot. This test simulates the resurrection with the same direct
/// `removed_at = NULL` update the scanner itself performs (see e.g.
/// `scanner.rs`'s fast-path-restore branch), rather than driving a real
/// scan, to stay a query-layer test of `purge_tombstones`'s own selection
/// logic — the scanner's own test suite already proves it actually clears
/// the column on resurrection.
#[test]
fn purge_tombstones_skips_a_row_resurrected_before_the_purge_runs() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_live_track(conn, 1);
    seed_live_track(conn, 2);
    tombstone_tracks(&db, &[1, 2], 1_000).unwrap();

    // The scanner found track 1's file again and resurrected it — exactly
    // the SQL `library::scanner` uses on its fast-path-restore branch.
    conn.execute("UPDATE tracks SET removed_at = NULL WHERE id = 1", [])
        .unwrap();

    let purged = purge_tombstones(&db).unwrap();

    assert_eq!(
        purged,
        vec![2],
        "only the still-tombstoned id may be purged"
    );
    let track_count: i64 = conn
        .query_row("SELECT count(*) FROM tracks WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        track_count, 1,
        "the resurrected row must survive the purge untouched"
    );
    assert_eq!(
        query_live_track_ids(&db).unwrap(),
        std::collections::HashSet::from([1]),
        "the resurrected row must be visible again, not silently purged"
    );
}

/// Finding 1 (Important, review pass): proves `purge_tombstones`'s guard
/// against a resurrection that lands mid-purge, not just before it. The
/// test above (`purge_tombstones_skips_a_row_resurrected_before_the_purge_
/// runs`) only covers the easy case — the resurrection happens, THEN
/// `purge_tombstones` is called, so its own `SELECT id FROM tracks WHERE
/// removed_at IS NOT NULL` never even sees the resurrected id. That test
/// would still pass against the unguarded `DELETE FROM tracks WHERE id =
/// ?1` this fix replaced, because the id was never in the snapshot to begin
/// with.
///
/// The real bug is a TOCTOU race: `purge_tombstones`'s `SELECT` and its
/// per-id `DELETE` are not one transaction, so the watcher thread (its own
/// `rusqlite::Connection`, concurrent under WAL) can commit a resurrection
/// AFTER an id is captured in the snapshot but BEFORE that id's `DELETE`
/// runs. A real thread race can't be scheduled deterministically in a unit
/// test, so this proves the guard directly instead: it calls the private
/// `remove_tracks_impl` with `RemoveGuard::TombstonedOnly` — the exact
/// statement `purge_tombstones` now runs per id — against a *stale* id list
/// that still contains an id whose `removed_at` was already cleared by
/// direct SQL, standing in for "what the watcher would have committed in
/// the race window." A stale-list call is the right proof shape because it
/// isolates the one thing that changed: whether the `DELETE` re-checks
/// `removed_at` at execution time instead of trusting the caller's
/// snapshot. Before this fix (`missing_only: bool` with no tombstone-aware
/// branch — `remove_tracks_impl` only ever ran a bare `DELETE FROM tracks
/// WHERE id = ?1` for a non-`missing_only` caller), this exact call would
/// have hard-deleted the resurrected row and cascaded away its playlist
/// membership and listen history right along with it; this test fails
/// against that code and passes against the `TombstonedOnly` guard.
#[test]
fn purge_tombstones_survives_a_resurrection_racing_the_delete_itself() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    for id in 1..=3 {
        seed_live_track(conn, id);
    }
    let playlist_id = playlists::create(&db, "Race").unwrap();
    playlists::add_tracks(&db, playlist_id, &[1, 2, 3]).unwrap();
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (2, 1000, 5000)",
        [],
    )
    .unwrap();

    tombstone_tracks(&db, &[1, 2, 3], 1_000).unwrap();

    // Simulate the watcher committing a resurrection of id 2 in the window
    // between `purge_tombstones`'s SELECT and its DELETE reaching that id —
    // the stale snapshot below (`[1, 2, 3]`) is what that SELECT would have
    // already captured before this write landed.
    conn.execute("UPDATE tracks SET removed_at = NULL WHERE id = 2", [])
        .unwrap();
    let stale_snapshot = vec![1, 2, 3];

    let deleted = remove_tracks_impl(conn, &stale_snapshot, RemoveGuard::TombstonedOnly).unwrap();

    assert_eq!(
        deleted,
        vec![1, 3],
        "only the ids still tombstoned at DELETE time may be removed"
    );

    let track_count: i64 = conn
        .query_row("SELECT count(*) FROM tracks WHERE id = 2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        track_count, 1,
        "the mid-purge-resurrected row must survive, not be hard-deleted"
    );
    assert_eq!(
        removed_at_of(conn, 2),
        None,
        "the survivor's resurrected (non-tombstoned) state must be untouched"
    );

    let playlist_rows: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map([playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        playlist_rows,
        vec![2],
        "the survivor's playlist membership must not be cascaded away"
    );

    let listen_event_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM listen_events WHERE track_id = 2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        listen_event_count, 1,
        "the survivor's listening history must not be cascaded away"
    );
}
