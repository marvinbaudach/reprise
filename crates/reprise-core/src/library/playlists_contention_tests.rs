//! Playlist writes that read before they write must survive a rival commit
//! landing between the two steps (#1185). The shared fixture places the rival
//! commit deterministically; see `rival_commit_test_support`.

use std::sync::atomic::Ordering;

use super::*;
use crate::db::Db;
use crate::library::rival_commit_test_support::{arm, arm_on_first_write};

fn contended_db() -> (tempfile::TempDir, Db) {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open_migrated(Some(&directory.path().join("reprise.db"))).unwrap();
    (directory, db)
}

fn arm_on_playlists(
    directory: &tempfile::TempDir,
    db: &Db,
    table: &'static str,
) -> impl Fn() -> bool {
    let flag = arm(db.conn(), &directory.path().join("reprise.db"), table);
    move || flag.load(Ordering::SeqCst)
}

#[test]
fn creating_a_playlist_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let interleaved = arm_on_playlists(&directory, &db, "playlists");

    let id = create(&db, "Contested").unwrap();

    assert!(
        interleaved(),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(get(&db, id).unwrap().unwrap().name, "Contested");
}

#[test]
fn ensuring_a_role_playlist_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let interleaved = arm_on_playlists(&directory, &db, "playlists");

    let id = ensure_role_playlist(&db, "Conversion", "conversion").unwrap();

    assert!(
        interleaved(),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(find_role_playlist(&db, "conversion").unwrap(), Some(id));
}

#[test]
fn creating_a_smart_playlist_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let interleaved = arm_on_playlists(&directory, &db, "smart_playlists");

    let id = create_smart(&db, "Contested", GENRE_RULES, "title", "asc", None).unwrap();

    assert!(
        interleaved(),
        "the rival must have committed mid-transaction"
    );
    assert!(list_smart(&db).unwrap().iter().any(|smart| smart.id == id));
}

#[test]
fn ensuring_an_existing_role_playlist_never_waits_for_the_write_lock() {
    let (directory, db) = contended_db();
    db.conn().pragma_update(None, "busy_timeout", 0).unwrap();
    let id = ensure_role_playlist(&db, "Conversion", "conversion").unwrap();
    let holder = rusqlite::Connection::open(directory.path().join("reprise.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let again = ensure_role_playlist(&db, "Conversion", "conversion").unwrap();

    assert_eq!(again, id);
}

fn seed_tracks(db: &Db, count: i64) -> Vec<i64> {
    (1..=count)
        .map(|id| {
            db.conn()
                .execute(
                    "INSERT INTO tracks (id, path, title, added_at) VALUES (?1, ?2, 'Track', 0)",
                    params![id, format!("/x/{id}.flac")],
                )
                .unwrap();
            id
        })
        .collect()
}

#[test]
fn appending_tracks_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let tracks = seed_tracks(&db, 3);
    let playlist = create(&db, "Contested").unwrap();
    let interleaved = arm_on_playlists(&directory, &db, "playlist_tracks");

    let inserted = add_tracks(&db, playlist, &tracks).unwrap();

    assert!(
        interleaved(),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(inserted, 3);
    assert_eq!(track_ids(&db, playlist).unwrap(), tracks);
}

#[test]
fn creating_a_playlist_with_tracks_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let tracks = seed_tracks(&db, 2);
    let interleaved = arm_on_playlists(&directory, &db, "playlists");

    let id = create_with_tracks(&db, "Contested", &tracks).unwrap();

    assert!(
        interleaved(),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(track_ids(&db, id).unwrap(), tracks);
}

#[test]
fn moving_a_track_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let tracks = seed_tracks(&db, 3);
    let playlist = create_with_tracks(&db, "Contested", &tracks).unwrap();
    let flag = arm_on_first_write(
        db.conn(),
        &directory.path().join("reprise.db"),
        "playlist_tracks",
    );

    move_position(&db, playlist, 0, 2).unwrap();

    assert!(
        flag.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(track_ids(&db, playlist).unwrap(), vec![2, 3, 1]);
}

/// `remove_positions` opens with a DELETE, so its first statement already
/// takes the write lock: the audit leaves it deferred, and this keeps that
/// reading honest.
#[test]
fn removing_positions_opens_with_a_write_and_survives_a_rival_commit() {
    let (directory, db) = contended_db();
    let tracks = seed_tracks(&db, 3);
    let playlist = create_with_tracks(&db, "Contested", &tracks).unwrap();
    let flag = arm_on_first_write(
        db.conn(),
        &directory.path().join("reprise.db"),
        "playlist_tracks",
    );

    let removed = remove_positions(&db, playlist, &[1]).unwrap();

    assert!(
        flag.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(removed, 1);
    assert_eq!(track_ids(&db, playlist).unwrap(), vec![1, 3]);
}
