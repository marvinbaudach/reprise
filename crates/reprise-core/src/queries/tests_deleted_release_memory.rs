//! Deliberate Remove-from-Library deletion-memory coverage.

use super::*;

fn seed_release_and_track(db: &crate::db::Db) {
    db.conn()
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES (1, '/music/one.flac', 'Song', 'Artist', 'Artist', 'Album', 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, first_seen
             ) VALUES ('release', 'Artist', 'artist-id', 'Album', 'Album',
                       '2026-08-01', 1, 1)",
            [],
        )
        .unwrap();
}

fn remembered_count(db: &crate::db::Db) -> i64 {
    db.conn()
        .query_row("SELECT count(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn nr_32_undone_removal_writes_no_memory() {
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);

    assert_eq!(tombstone_tracks(&db, &[1], 100).unwrap(), 1);
    assert_eq!(undo_tombstone(&db, &[1]).unwrap(), 1);
    assert!(purge_tombstones(&db).unwrap().is_empty());

    assert_eq!(remembered_count(&db), 0);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
}

#[test]
fn nr_32_purged_removal_writes_deletion_memory_on_completion() {
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);

    assert_eq!(tombstone_tracks(&db, &[1], 100).unwrap(), 1);
    assert_eq!(purge_tombstones(&db).unwrap(), vec![1]);

    assert_eq!(remembered_count(&db), 2);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);
}
