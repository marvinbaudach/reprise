use chrono::NaiveDate;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, 11).unwrap()
}

fn insert_release(db: &crate::db::Db, mbid: &str, title: &str, release_type: &str) {
    db.conn()
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, first_seen
             ) VALUES (?1, 'Release Artist', 'artist-id', ?2, ?3,
                       '2026-08-01', 1, 1)",
            rusqlite::params![mbid, title, release_type],
        )
        .unwrap();
}

fn insert_track(db: &crate::db::Db, id: i64, title: &str, album: &str) {
    db.conn()
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES (?1, ?2, ?3, 'Track Artist', 'Release Artist', ?4, 0)",
            rusqlite::params![id, format!("/music/{id}.flac"), title, album],
        )
        .unwrap();
}

fn remove_from_library(db: &crate::db::Db, ids: &[i64]) {
    let tracks = ids
        .iter()
        .map(|id| (*id, std::path::PathBuf::from(format!("/music/{id}.flac"))))
        .collect::<Vec<_>>();
    assert_eq!(
        crate::queries::exclude_tracks_matching_paths(db, &tracks, 100).unwrap(),
        ids
    );
}

#[test]
fn nr_32_deleting_the_last_track_of_an_album_hides_its_gap() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Album", "One Album");
    insert_track(&db, 99, "Library Anchor", "Other Album");

    remove_from_library(&db, &[1]);

    assert!(crate::artist_news::query_releases_view(
        &db,
        &crate::artist_news::ReleasesFilter::default(),
        today(),
    )
    .unwrap()
    .is_empty());
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);
}

#[test]
fn nr_32_deleting_one_track_keeps_the_album_gap_but_hides_its_single() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_release(&db, "single", "One Song", "Single");
    insert_track(&db, 1, "One Song", "One Album");
    insert_track(&db, 2, "Two Song", "One Album");

    remove_from_library(&db, &[1]);

    let visible = crate::artist_news::query_releases_view(
        &db,
        &crate::artist_news::ReleasesFilter::widest(false),
        today(),
    );
    assert_eq!(
        visible
            .unwrap()
            .into_iter()
            .map(|release| release.release_group_mbid)
            .collect::<Vec<_>>(),
        ["album"]
    );
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);
}

#[test]
fn nr_32_deleted_song_hides_only_its_single_row() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Song", "Album");
    insert_release(&db, "single", "One Song", "Single");
    insert_track(&db, 1, "One Song", "");
    insert_track(&db, 99, "Library Anchor", "Other Album");

    remove_from_library(&db, &[1]);

    let visible = crate::artist_news::query_releases_view(
        &db,
        &crate::artist_news::ReleasesFilter {
            release_types: crate::artist_news::ReleaseTypeSelection::all(),
            window: crate::artist_news::ReleaseWindow::All,
            hidden: false,
        },
        today(),
    )
    .unwrap();
    assert_eq!(
        visible
            .into_iter()
            .map(|release| release.release_group_mbid)
            .collect::<Vec<_>>(),
        ["album"]
    );
}

#[test]
fn nr_32_missing_sibling_writes_no_memory() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_release(&db, "single", "One Song", "Single");
    insert_track(&db, 1, "One Song", "One Album");
    insert_track(&db, 2, "One Song", "One Album");
    db.conn()
        .execute("UPDATE tracks SET missing_since = 50 WHERE id = 2", [])
        .unwrap();

    remove_from_library(&db, &[1]);

    let remembered: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remembered, 0);
}

#[test]
fn nr_32_missing_track_counts_as_held_in_every_memory_reconciliation_path() {
    let deletion_db = crate::db::Db::open_in_memory().unwrap();
    insert_track(&deletion_db, 1, "Ghost", "Ghost");
    insert_track(&deletion_db, 2, "Ghost", "Ghost");
    deletion_db
        .conn()
        .execute("UPDATE tracks SET missing_since = 50 WHERE id = 2", [])
        .unwrap();

    remove_from_library(&deletion_db, &[1]);

    let deletion_memories: i64 = deletion_db
        .conn()
        .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        deletion_memories, 0,
        "deletion must see the missing survivor"
    );

    let apply_db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&apply_db, "album", "Ghost", "Album");
    insert_release(&apply_db, "single", "Ghost", "Single");
    insert_track(&apply_db, 1, "Ghost", "Ghost");
    apply_db
        .conn()
        .execute_batch(
            "UPDATE tracks SET missing_since = 50 WHERE id = 1;
             INSERT INTO deleted_releases (artist_key, title_key, scope, deleted_at)
             VALUES ('release artist', 'ghost', 'album', 10),
                    ('release artist', 'ghost', 'track', 10);
             UPDATE new_releases
             SET hidden = 1, hidden_at = 10, hidden_by_deleted_memory = 1;",
        )
        .unwrap();

    crate::deleted_releases::apply_deleted_release_memory(apply_db.conn()).unwrap();

    let applied_memories: i64 = apply_db
        .conn()
        .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        applied_memories, 0,
        "apply must forget both acquired scopes"
    );
    assert_eq!(
        crate::artist_news::hidden_release_count(&apply_db).unwrap(),
        0
    );

    let undo_db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&undo_db, "album", "Ghost", "Album");
    insert_release(&undo_db, "single", "Ghost", "Single");
    insert_track(&undo_db, 1, "Ghost", "Ghost");
    undo_db
        .conn()
        .execute_batch(
            "UPDATE tracks SET missing_since = 50, removed_at = 100 WHERE id = 1;
             INSERT INTO deleted_releases (artist_key, title_key, scope, deleted_at)
             VALUES ('release artist', 'ghost', 'album', 10),
                    ('release artist', 'ghost', 'track', 10);
             UPDATE new_releases
             SET hidden = 1, hidden_at = 10, hidden_by_deleted_memory = 1;",
        )
        .unwrap();

    assert_eq!(crate::queries::undo_tombstone(&undo_db, &[1]).unwrap(), 1);

    let undo_memories: i64 = undo_db
        .conn()
        .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(undo_memories, 0, "undo must forget both restored scopes");
    assert_eq!(
        crate::artist_news::hidden_release_count(&undo_db).unwrap(),
        0
    );
    let missing_since: Option<i64> = undo_db
        .conn()
        .query_row("SELECT missing_since FROM tracks WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        missing_since,
        Some(50),
        "undo must not rewrite missing state"
    );
}

#[test]
fn nr_32_album_memory_also_hides_the_ep_twin() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Work", "Album");
    insert_release(&db, "ep", "Shared Work", "EP");
    insert_track(&db, 1, "One Song", "Shared Work");
    insert_track(&db, 99, "Library Anchor", "Other Album");

    remove_from_library(&db, &[1]);

    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 2);
}

#[test]
fn nr_32_show_again_forgets_the_selected_release_scope() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Song", "One Album");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    remove_from_library(&db, &[1]);

    crate::artist_news::set_release_hidden(&db, "album", false).unwrap();
    let hidden_again = crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    assert_eq!(hidden_again, 0);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
    let remembered_scopes = db
        .conn()
        .prepare("SELECT scope FROM deleted_releases ORDER BY scope")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(remembered_scopes, ["track"]);
}

#[test]
fn nr_32_reacquiring_the_album_forgets_the_deletion() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Album", "One Album");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    remove_from_library(&db, &[1]);

    insert_track(&db, 2, "One Album", "One Album");
    let hidden = crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    assert_eq!(hidden, 0);
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 0);
    let remembered: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remembered, 0);
}

#[test]
fn nr_32_reacquiring_an_album_keeps_its_absent_same_titled_single_hidden() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Title", "Album");
    insert_release(&db, "single", "Shared Title", "Single");
    insert_track(&db, 1, "Shared Title", "Shared Title");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    remove_from_library(&db, &[1]);

    insert_track(&db, 2, "Different Song", "Shared Title");
    crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    let visible = crate::artist_news::query_releases_view(
        &db,
        &crate::artist_news::ReleasesFilter::widest(false),
        today(),
    )
    .unwrap();
    assert_eq!(
        visible
            .into_iter()
            .map(|release| release.release_group_mbid)
            .collect::<Vec<_>>(),
        ["album"]
    );
    let memories = db
        .conn()
        .prepare("SELECT scope FROM deleted_releases ORDER BY scope")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(memories, ["track"]);
}

#[test]
fn nr_32_show_again_restores_every_row_hidden_by_the_same_album_memory() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Work", "Album");
    insert_release(&db, "ep", "Shared Work", "EP");
    insert_track(&db, 1, "One Song", "Shared Work");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    remove_from_library(&db, &[1]);

    crate::artist_news::set_release_hidden(&db, "album", false).unwrap();

    let still_hidden = db
        .conn()
        .prepare("SELECT release_group_mbid FROM new_releases WHERE hidden = 1 ORDER BY 1")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(still_hidden.is_empty(), "still hidden: {still_hidden:?}");
    let scopes = db
        .conn()
        .prepare("SELECT scope FROM deleted_releases ORDER BY scope")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(scopes, ["track"]);
}

#[test]
fn nr_32_show_again_keeps_a_row_covered_by_surviving_track_memory_hidden() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Title", "Album");
    insert_release(&db, "single", "Shared Title", "Single");
    insert_track(&db, 1, "Shared Title", "Shared Title");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    remove_from_library(&db, &[1]);

    crate::artist_news::set_release_hidden(&db, "album", false).unwrap();

    let hidden = db
        .conn()
        .prepare("SELECT release_group_mbid FROM new_releases WHERE hidden = 1 ORDER BY 1")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(hidden, ["single"]);
    assert_eq!(
        crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap(),
        0,
        "a refresh must not make the still-remembered single flap"
    );
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);
}

#[test]
fn nr_32_show_again_preserves_a_manually_hidden_twin() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Work", "Album");
    insert_release(&db, "ep", "Shared Work", "EP");
    insert_track(&db, 1, "One Song", "Shared Work");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    crate::artist_news::set_release_hidden(&db, "ep", true).unwrap();
    remove_from_library(&db, &[1]);

    crate::artist_news::set_release_hidden(&db, "album", false).unwrap();

    let hidden = db
        .conn()
        .prepare("SELECT release_group_mbid FROM new_releases WHERE hidden = 1 ORDER BY 1")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(hidden, ["ep"]);
}

#[test]
fn nr_32_show_again_rolls_back_memory_and_twins_when_one_unhide_fails() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Work", "Album");
    insert_release(&db, "ep", "Shared Work", "EP");
    insert_track(&db, 1, "One Song", "Shared Work");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    remove_from_library(&db, &[1]);
    db.conn()
        .execute_batch(
            "CREATE TRIGGER fail_ep_unhide
             BEFORE UPDATE OF hidden ON new_releases
             WHEN OLD.release_group_mbid = 'ep' AND NEW.hidden = 0
             BEGIN
               SELECT RAISE(ABORT, 'injected unhide failure');
             END;",
        )
        .unwrap();

    assert!(crate::artist_news::set_release_hidden(&db, "album", false).is_err());

    let hidden_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM new_releases WHERE hidden = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let album_memory: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM deleted_releases WHERE scope = 'album'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hidden_count, 2);
    assert_eq!(album_memory, 1);
}

#[test]
fn nr_32_unrelated_deletion_forgets_a_reacquired_release_before_hiding() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "target", "Target Album", "Album");
    insert_track(&db, 1, "Target Song", "Target Album");
    insert_track(&db, 99, "Library Anchor", "Anchor Album");
    remove_from_library(&db, &[1]);

    insert_track(&db, 2, "Target Song", "Target Album");
    insert_track(&db, 3, "Unrelated Song", "Unrelated Album");
    remove_from_library(&db, &[3]);

    let target_memory: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM deleted_releases
             WHERE title_key = 'target album' AND scope = 'album'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let target_hidden: bool = db
        .conn()
        .query_row(
            "SELECT hidden FROM new_releases WHERE release_group_mbid = 'target'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(target_memory, 0);
    assert!(!target_hidden);
}

#[test]
fn deletion_memory_rejects_an_album_without_an_artist_identity() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (
               id, path, title, artist, album_artist, album, added_at
             ) VALUES (1, '/music/1.flac', 'Song', '', '', 'Anonymous Album', 0)",
            [],
        )
        .unwrap();

    remove_from_library(&db, &[1]);

    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn applying_empty_memory_does_not_require_library_or_catalog_tables() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE deleted_releases (
           artist_key TEXT NOT NULL,
           title_key TEXT NOT NULL,
           scope TEXT NOT NULL,
           deleted_at INTEGER NOT NULL,
           PRIMARY KEY (artist_key, title_key, scope)
         );",
    )
    .unwrap();

    assert_eq!(
        crate::deleted_releases::apply_deleted_release_memory(&conn).unwrap(),
        0
    );
}
