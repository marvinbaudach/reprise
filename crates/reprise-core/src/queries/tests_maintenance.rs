//! Maintenance test coverage: hard-delete (`remove_missing_tracks`),
//! import-error triage, and `track_id_for_path` — split out of the former
//! single-file `queries.rs`'s inline test module (Refactoring &
//! Extensibility Task 1) purely to keep every file under the project's
//! 800-line rule; see `tests.rs`'s doc comment for the full split map. A
//! pure move, no assertion change.
//!
//! The tombstone/10-second-undo functions (`tombstone_tracks`/`undo_
//! tombstone`/`purge_tombstones`) also live in `maintenance.rs`, but their
//! tests live in `tests_issues.rs` instead of here — this file was already
//! close to the 800-line rule when that feature landed (Task 2.2); see
//! `tests_issues.rs`'s own module doc comment for the full rationale.

use super::*;
use crate::library::playlists;
use std::collections::HashSet;

fn seeded_sync_tracks() -> (tempfile::TempDir, Connection) {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, name, title, artist, duration, bytes) in [
        (1, "one.flac", "One", "First", 11_000, 11_usize),
        (2, "two.mp3", "Two", "Second", 22_000, 22),
        (3, "three.ogg", "Three", "Third", 33_000, 33),
    ] {
        let path = temp.path().join(name);
        std::fs::write(&path, vec![id as u8; bytes]).unwrap();
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,duration_ms,added_at) \
             VALUES (?1,?2,?3,?4,?5,0)",
            rusqlite::params![id, path.to_string_lossy(), title, artist, duration],
        )
        .unwrap();
    }
    (temp, conn)
}

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
fn filter_present_keeps_present_ids_in_order_and_drops_missing_and_unknown() {
    let conn = crate::db::open_migrated(None).unwrap();
    for (id, missing_since) in [(1, None::<i64>), (2, Some(1)), (3, None)] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at, missing_since) \
             VALUES (?1, ?2, '', '', 0, ?3)",
            rusqlite::params![id, format!("/x/{id}.flac"), missing_since],
        )
        .unwrap();
    }
    // Input mixes a present id, a missing id (row exists, missing_since set), an
    // unknown id, a duplicate present id, and another present id — out of order.
    let result = filter_present(&conn, &[3, 2, 999, 1, 1]).unwrap();
    // Only present ids survive (2 is missing, 999 unknown), input order kept and
    // the duplicate 1 collapsed.
    assert_eq!(result, vec![3, 1]);

    // Empty input short-circuits with no query.
    assert!(filter_present(&conn, &[]).unwrap().is_empty());
}

#[test]
fn feature_queries_filter_and_order_tracks_for_their_consumers() {
    let conn = crate::db::open_migrated(None).unwrap();
    for (id, path, title, missing_since) in [
        (1, "/music/b.flac", "SmokeFirst", None),
        (2, "/music/a.flac", "SmokeSlow", None),
        (3, "/music/gone.flac", "SmokeFast", Some(1)),
        (4, "/music/c.flac", "SmokeFirst", None),
    ] {
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,added_at,missing_since) \
             VALUES (?1,?2,?3,'',0,?4)",
            rusqlite::params![id, path, title, missing_since],
        )
        .unwrap();
    }

    assert_eq!(
        query_live_track_ids(&conn).unwrap(),
        HashSet::from([1, 2, 4])
    );
    assert_eq!(
        query_live_track_paths(&conn).unwrap(),
        vec![
            "/music/a.flac".to_string(),
            "/music/b.flac".to_string(),
            "/music/c.flac".to_string()
        ]
    );
    assert_eq!(
        query_track_ids_by_titles(&conn, &["SmokeFirst", "SmokeFast"])
            .unwrap()
            .get("SmokeFirst"),
        Some(&1)
    );
    assert_eq!(
        query_track_ids_by_titles(&conn, &["SmokeFast"])
            .unwrap()
            .get("SmokeFast"),
        Some(&3)
    );
    assert_eq!(
        query_track_ids_by_title_desc(&conn).unwrap(),
        vec![2, 1, 4, 3]
    );
}

// -- ImportErrors source -------------------------------------------------

#[test]
fn query_import_error_count_counts_the_table() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    assert_eq!(query_import_error_count(&conn).unwrap(), 0);

    conn.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen) \
         VALUES ('/x/a.flac', 'tag', 'bad tag', 0, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen) \
         VALUES ('/x/b.flac', 'tag', 'bad tag', 0, 0)",
        [],
    )
    .unwrap();
    assert_eq!(query_import_error_count(&conn).unwrap(), 2);
}

// -- remove_missing_tracks (Stage-3 close-out) ----------------------

/// THE core regression test for the "hard-delete broke a cross-task
/// invariant" finding: a playlist `[1,2,3,4,5]` (`pt.position` 0..4);
/// track 3 (position 2, the MIDDLE one) gets marked missing and then
/// hard-deleted via `remove_missing_tracks`. Before this fix, the
/// `ON DELETE CASCADE` on `playlist_tracks` would leave positions
/// `[0,1,3,4]` — a gap — which `library::playlists::move_position`
/// (treating a position as a literal `Vec` index) would silently
/// mis-resolve on the very next drag-reorder. This asserts the fix:
/// positions come out gapless (`0..n-1`) immediately after the delete.
#[test]
fn remove_missing_tracks_compacts_playlist_positions_after_a_middle_row_delete() {
    let mut conn = seeded_conn_with_tracks(5);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3, 4, 5]).unwrap();
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 3",
        [],
    )
    .unwrap();

    let removed = remove_missing_tracks(&mut conn, &[3]).unwrap();
    assert_eq!(removed, vec![3]);

    let (track_ids, positions): (Vec<i64>, Vec<i64>) = {
        let mut stmt = conn
            .prepare(
                "SELECT track_id, position FROM playlist_tracks \
                 WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap();
        let rows: Vec<(i64, i64)> = stmt
            .query_map(rusqlite::params![playlist_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        rows.into_iter().unzip()
    };
    assert_eq!(
        track_ids,
        vec![1, 2, 4, 5],
        "track 3 is gone, order preserved"
    );
    assert_eq!(
        positions,
        vec![0, 1, 2, 3],
        "positions must be gapless (0..n-1) after the hard-delete, not [0,1,3,4]"
    );

    // The wrong-row-move class this closes: moving the row now at
    // position 2 (track 4) must move track 4, not silently mis-resolve
    // because of a leftover gap.
    playlists::move_position(&mut conn, playlist_id, 2, 0).unwrap();
    let after_move: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(rusqlite::params![playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        after_move,
        vec![4, 1, 2, 5],
        "track 4 (now at position 2) moved to the front"
    );
}

#[test]
fn remove_missing_tracks_compacts_every_affected_playlist_in_one_call() {
    let mut conn = seeded_conn_with_tracks(4);
    let p1 = playlists::create(&conn, "P1").unwrap();
    let p2 = playlists::create(&conn, "P2").unwrap();
    playlists::add_tracks(&mut conn, p1, &[1, 2, 3]).unwrap();
    playlists::add_tracks(&mut conn, p2, &[2, 3, 4]).unwrap();
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id IN (2, 3)",
        [],
    )
    .unwrap();

    let mut removed = remove_missing_tracks(&mut conn, &[2, 3]).unwrap();
    removed.sort_unstable();
    assert_eq!(removed, vec![2, 3]);

    for playlist_id in [p1, p2] {
        let positions: Vec<i64> = conn
            .prepare(
                "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(rusqlite::params![playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            positions,
            (0..positions.len() as i64).collect::<Vec<_>>(),
            "playlist {playlist_id} must stay gapless"
        );
    }
}

#[test]
fn remove_missing_tracks_skips_ids_that_are_not_missing() {
    let mut conn = seeded_conn_with_tracks(3);
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 1",
        [],
    )
    .unwrap();
    // id 2 is left alone (still present, missing_since NULL).

    let removed = remove_missing_tracks(&mut conn, &[1, 2]).unwrap();

    assert_eq!(
        removed,
        vec![1],
        "only the actually-missing track is removed"
    );
    let count: i64 = conn
        .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn remove_missing_tracks_empty_slice_is_a_no_op() {
    let mut conn = seeded_conn_with_tracks(2);
    let removed = remove_missing_tracks(&mut conn, &[]).unwrap();
    assert!(removed.is_empty());
    let count: i64 = conn
        .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

/// Property-style regression test (the reviewer's ask): runs a scripted
/// sequence of add/remove/move/hard-delete operations against a real
/// playlist and a real `queue::Queue`, asserting the gapless-positions
/// invariant holds after EVERY mutating step — not just immediately
/// after one hard-delete, which is what would have caught the original
/// bug at commit time — and that the queue's own count of resolvable
/// ids tracks `query_track_count`'s `Queue` arm after each removal too.
#[test]
fn playlist_positions_stay_gapless_and_queue_count_stays_accurate_across_a_mixed_operation_sequence(
) {
    fn assert_gapless(conn: &Connection, playlist_id: i64) {
        let positions: Vec<i64> = conn
            .prepare(
                "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(rusqlite::params![playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            positions,
            (0..positions.len() as i64).collect::<Vec<_>>(),
            "playlist_tracks.position must stay gapless (0..n-1) after every operation"
        );
    }

    let mut conn = seeded_conn_with_tracks(8);
    let playlist_id = playlists::create(&conn, "Mix").unwrap();

    // 1. add: [1,2,3,4,5,6]
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3, 4, 5, 6]).unwrap();
    assert_gapless(&conn, playlist_id);

    // 2. remove (positions 1,3 -> ids 2,4): [1,3,5,6]
    playlists::remove_positions(&mut conn, playlist_id, &[1, 3]).unwrap();
    assert_gapless(&conn, playlist_id);

    // 3. move: [1,3,5,6] -> move index 0 to index 2 -> [3,5,1,6]
    playlists::move_position(&mut conn, playlist_id, 0, 2).unwrap();
    assert_gapless(&conn, playlist_id);

    // A queue holding the same surviving ids, in the same order.
    let mut queue = crate::queue::Queue::new();
    queue.set_tracks(vec![3, 5, 1, 6, 7], 0);

    // 4. hard-delete the middle-ish track (id 1, currently at playlist
    // position 2) after marking it missing — the exact bug scenario.
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 1",
        [],
    )
    .unwrap();
    let removed = remove_missing_tracks(&mut conn, &[1]).unwrap();
    assert_eq!(removed, vec![1]);
    assert_gapless(&conn, playlist_id);

    // Queue purge (mirrors `PlayerController::purge_queue_ids`) and the
    // count-arm invariant: queue's own resolvable count and `query_
    // track_count`'s `Queue` arm must agree, both before and after the
    // in-memory queue purge runs.
    let queue_ids_before_purge = queue.ids_in_order();
    let count_before_purge =
        query_track_count(&conn, &ViewSource::Queue, "", &queue_ids_before_purge).unwrap();
    assert_eq!(
        count_before_purge as usize,
        queue_ids_before_purge.len() - 1,
        "count arm must exclude the just-hard-deleted id even before the queue is purged"
    );

    assert!(queue.remove_ids(&removed));
    let queue_ids_after_purge = queue.ids_in_order();
    assert!(
        !queue_ids_after_purge.contains(&1),
        "purged id must be gone from the queue"
    );
    let count_after_purge =
        query_track_count(&conn, &ViewSource::Queue, "", &queue_ids_after_purge).unwrap();
    assert_eq!(
        count_after_purge as usize,
        queue_ids_after_purge.len(),
        "after purge, every remaining queued id must resolve — count == queue length"
    );

    // 5. one more move on the now-compacted playlist ([3,5,6]) must
    // still move the correct row.
    let before_final_move: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(rusqlite::params![playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(before_final_move, vec![3, 5, 6]);
    playlists::move_position(&mut conn, playlist_id, 2, 0).unwrap();
    let after_final_move: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(rusqlite::params![playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        after_final_move,
        vec![6, 3, 5],
        "track 6 (position 2) moved to the front"
    );
    assert_gapless(&conn, playlist_id);
}

// -- track_id_for_path (Stage 3 Task 7) ---

#[test]
fn track_id_for_path_finds_exact_match() {
    let conn = seeded_conn_with_tracks(3);
    let id = track_id_for_path(&conn, "/x/2.flac").unwrap();
    assert_eq!(id, Some(2));
}

#[test]
fn track_id_for_path_returns_none_for_unknown_path() {
    let conn = seeded_conn_with_tracks(3);
    let id = track_id_for_path(&conn, "/nowhere/x.flac").unwrap();
    assert_eq!(id, None);
}

#[test]
fn track_id_for_path_does_not_substring_match() {
    // A LIKE-style partial match would be wrong here: this must be an
    // exact match only.
    let conn = seeded_conn_with_tracks(3);
    let id = track_id_for_path(&conn, "/x/2").unwrap();
    assert_eq!(id, None);
}

// -- device synchronization ---------------------------------------------

#[test]
fn sync_tracks_preserve_input_order_and_deduplicate_ids() {
    let (_temp, conn) = seeded_sync_tracks();
    let tracks = query_sync_tracks(&conn, &[3, 1, 3, 2, 1]).unwrap();
    assert_eq!(
        tracks.iter().map(|track| track.id).collect::<Vec<_>>(),
        [3, 1, 2]
    );
}

#[test]
fn sync_tracks_exclude_unknown_missing_and_unavailable_paths() {
    let (temp, conn) = seeded_sync_tracks();
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 2",
        [],
    )
    .unwrap();
    std::fs::remove_file(temp.path().join("three.ogg")).unwrap();

    let tracks = query_sync_tracks(&conn, &[999, 1, 2, 3]).unwrap();
    assert_eq!(tracks.iter().map(|track| track.id).collect::<Vec<_>>(), [1]);
}

#[test]
fn sync_tracks_exclude_tombstoned_rows_even_when_the_file_still_exists() {
    let (_temp, conn) = seeded_sync_tracks();
    conn.execute("UPDATE tracks SET removed_at = 1 WHERE id = 2", [])
        .unwrap();

    let tracks = query_sync_tracks(&conn, &[1, 2, 3]).unwrap();
    assert_eq!(
        tracks.iter().map(|track| track.id).collect::<Vec<_>>(),
        [1, 3]
    );
}

#[test]
fn sync_tracks_include_copy_metadata_and_actual_file_size() {
    let (temp, conn) = seeded_sync_tracks();
    let tracks = query_sync_tracks(&conn, &[2]).unwrap();
    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    assert_eq!(track.source_path, temp.path().join("two.mp3"));
    assert_eq!(track.original_name, "two.mp3");
    assert_eq!(track.title, "Two");
    assert_eq!(track.artist, "Second");
    assert_eq!(track.duration_ms, 22_000);
    assert_eq!(track.size_bytes, 22);
}

// -- query_track_album_artist (player-bar artist deep-link) --------------

#[test]
fn track_album_artist_prefers_album_artist_then_falls_back_to_artist() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (id,path,title,artist,album_artist,added_at) VALUES \
         (1,'/a','A','Track Artist','  Album Artist  ',0), \
         (2,'/b','B','Solo Artist','',0), \
         (3,'/c','C','Solo Artist','   ',0), \
         (4,'/d','D','','',0)",
        [],
    )
    .unwrap();

    // Tagged album artist wins, trimmed by the EFFECTIVE_ALBUM_ARTIST fallback.
    assert_eq!(
        query_track_album_artist(&conn, 1).unwrap().as_deref(),
        Some("Album Artist")
    );
    // Empty album artist falls back to the (trimmed) track artist.
    assert_eq!(
        query_track_album_artist(&conn, 2).unwrap().as_deref(),
        Some("Solo Artist")
    );
    // Whitespace-only album artist also falls back to the track artist.
    assert_eq!(
        query_track_album_artist(&conn, 3).unwrap().as_deref(),
        Some("Solo Artist")
    );
    // Neither tagged: SQL yields the empty string (caller treats blank as none).
    assert_eq!(
        query_track_album_artist(&conn, 4).unwrap().as_deref(),
        Some("")
    );
    // Unknown id.
    assert_eq!(query_track_album_artist(&conn, 99).unwrap(), None);
}
