//! Rule-named feedback acceptance tests for the issue workflows.

use super::*;
use crate::library::playlists;
use crate::library::settings::{self, AutoCleanSetting};
use crate::models::MissingReason;

fn seed_track(conn: &Connection, id: i64, rating: i64, play_count: i64) {
    conn.execute(
        "INSERT INTO tracks (id,path,title,artist,rating,play_count,added_at) \
         VALUES (?1,?2,?3,'Artist',?4,?5,0)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            rating,
            play_count
        ],
    )
    .unwrap();
}

// UX FB-7: tombstones preserve identity and user history for exact Undo,
// while expiry commits the removal and its cascades.
#[test]
fn fb_7_tombstone_undo_is_exact_and_expiry_commits_cascades() {
    let mut conn = crate::db::open_migrated(None).unwrap();
    seed_track(&conn, 1, 4, 17);
    seed_track(&conn, 2, 2, 3);
    let playlist_id = playlists::create(&conn, "Keep order").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2]).unwrap();
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, 10, 2000)",
        [],
    )
    .unwrap();

    assert_eq!(tombstone_tracks(&conn, &[1], 1_000).unwrap(), 1);
    let preserved: (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT id,rating,play_count,removed_at FROM tracks WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(preserved, (1, 4, 17, 1_000));
    let preserved_playlist: Vec<(i64, i64)> = conn
        .prepare(
            "SELECT track_id,position FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position",
        )
        .unwrap()
        .query_map([playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(preserved_playlist, vec![(1, 0), (2, 1)]);
    assert_eq!(undo_tombstone(&conn, &[1]).unwrap(), 1);
    let restored: (i64, i64, i64, Option<i64>) = conn
        .query_row(
            "SELECT id,rating,play_count,removed_at FROM tracks WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(restored, (1, 4, 17, None));
    let restored_playlist: Vec<(i64, i64)> = conn
        .prepare(
            "SELECT track_id,position FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position",
        )
        .unwrap()
        .query_map([playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(restored_playlist, preserved_playlist);

    tombstone_tracks(&conn, &[1], 2_000).unwrap();
    assert_eq!(purge_tombstones(&mut conn).unwrap(), vec![1]);
    let playlist_rows: Vec<(i64, i64)> = conn
        .prepare(
            "SELECT track_id,position FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position",
        )
        .unwrap()
        .query_map([playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(playlist_rows, vec![(2, 0)]);
    let listens: i64 = conn
        .query_row(
            "SELECT count(*) FROM listen_events WHERE track_id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(listens, 0);

    // The same rule's unattended exception is opt-in and evidence-bound:
    // default Off removes nothing, and an armed 30-day run removes only a
    // proven-deleted row, never unmounted/unknown rows.
    for (id, reason) in [
        (3, MissingReason::Deleted),
        (4, MissingReason::Unmounted),
        (5, MissingReason::Unknown),
    ] {
        seed_track(&conn, id, 1, 1);
        conn.execute(
            "UPDATE tracks SET missing_since=0,missing_reason=?1 WHERE id=?2",
            rusqlite::params![reason.as_str(), id],
        )
        .unwrap();
    }
    assert!(run_auto_clean(&mut conn, 90 * 86_400).unwrap().is_empty());
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Days(30)).unwrap();
    settings::set_auto_clean_armed_at(&conn, 0).unwrap();
    assert_eq!(run_auto_clean(&mut conn, 30 * 86_400).unwrap(), vec![3]);
    let survivors: Vec<i64> = conn
        .prepare("SELECT id FROM tracks WHERE id IN (4,5) ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(survivors, vec![4, 5]);
}
