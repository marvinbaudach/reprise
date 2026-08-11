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
fn nr_32_missing_file_writes_no_memory() {
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);
    db.conn()
        .execute(
            "UPDATE tracks SET missing_since = 50, missing_reason = 'deleted' WHERE id = 1",
            [],
        )
        .unwrap();

    assert_eq!(tombstone_tracks(&db, &[1], 100).unwrap(), 1);
    assert_eq!(purge_tombstones(&db).unwrap(), vec![1]);

    assert_eq!(remembered_count(&db), 0);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
}

#[test]
fn nr_32_remove_from_library_writes_deletion_memory_on_completion() {
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);

    assert_eq!(
        exclude_tracks_matching_paths(
            &db,
            &[(1, std::path::PathBuf::from("/music/one.flac"))],
            100,
        )
        .unwrap(),
        vec![1]
    );

    assert_eq!(remembered_count(&db), 2);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);
}

#[test]
fn nr_32_undo_reapplies_memory_after_a_tombstoned_sibling_returns() {
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);
    db.conn()
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES (2, '/music/two.flac', 'Other Song', 'Artist', 'Artist', 'Album', 0)",
            [],
        )
        .unwrap();

    assert_eq!(tombstone_tracks(&db, &[2], 100).unwrap(), 1);
    assert_eq!(
        exclude_tracks_matching_paths(
            &db,
            &[(1, std::path::PathBuf::from("/music/one.flac"))],
            100,
        )
        .unwrap(),
        vec![1]
    );
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);

    assert_eq!(undo_tombstone(&db, &[2]).unwrap(), 1);

    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
    assert_eq!(remembered_count(&db), 1, "the deleted song memory remains");
}

#[test]
fn nr_32_undo_reconciles_only_the_tracks_it_restored() {
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES
               (2, '/music/two.flac', 'Other Song', 'Artist', 'Artist', 'Album', 0);",
        )
        .unwrap();

    assert_eq!(tombstone_tracks(&db, &[2], 100).unwrap(), 1);
    assert_eq!(
        exclude_tracks_matching_paths(
            &db,
            &[(1, std::path::PathBuf::from("/music/one.flac"))],
            100,
        )
        .unwrap(),
        vec![1]
    );
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES
               (3, '/music/unrelated.flac', 'Unrelated', X'80', '', 'Other', 0);
             INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at
             ) VALUES
               ('unrelated', X'80', 'unrelated-artist', 'Other', 'Album',
                '2026-08-01', 1);",
        )
        .unwrap();

    assert_eq!(undo_tombstone(&db, &[2]).unwrap(), 1);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
    assert_eq!(remembered_count(&db), 1, "the deleted song memory remains");
}

#[test]
fn nr_32_auto_clean_writes_no_memory() {
    const DAY: i64 = 86_400;
    let db = crate::db::Db::open_in_memory().unwrap();
    seed_release_and_track(&db);
    crate::library::settings::set_missing_auto_clean(
        &db,
        crate::library::settings::AutoCleanSetting::Days(30),
    )
    .unwrap();
    crate::library::settings::set_auto_clean_armed_at(&db, 0).unwrap();
    db.conn()
        .execute(
            "UPDATE tracks SET missing_since = 0, missing_reason = 'deleted' WHERE id = 1",
            [],
        )
        .unwrap();

    assert_eq!(run_auto_clean(&db, 30 * DAY).unwrap(), vec![1]);

    assert_eq!(remembered_count(&db), 0);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
}
