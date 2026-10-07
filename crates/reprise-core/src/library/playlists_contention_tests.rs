//! Playlist writes that read before they write must survive a rival commit
//! landing between the two steps (#1185). The shared fixture places the rival
//! commit deterministically; see `rival_commit_test_support`.

use std::sync::atomic::Ordering;

use super::*;
use crate::db::Db;
use crate::library::rival_commit_test_support::arm;

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
