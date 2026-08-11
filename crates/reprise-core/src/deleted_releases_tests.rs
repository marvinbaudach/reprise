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

#[test]
fn nr_32_deleting_the_last_track_of_an_album_hides_its_gap() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Album", "One Album");
    insert_track(&db, 99, "Library Anchor", "Other Album");

    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    let hidden = crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    assert_eq!(hidden, 1);
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
fn nr_32_deleting_one_track_of_an_album_keeps_the_gap() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Song", "One Album");
    insert_track(&db, 2, "Two Song", "One Album");

    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    let hidden = crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    assert_eq!(hidden, 0);
    assert_eq!(
        crate::artist_news::query_releases_view(
            &db,
            &crate::artist_news::ReleasesFilter::default(),
            today(),
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn nr_32_deleted_song_hides_only_its_single_row() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Song", "Album");
    insert_release(&db, "single", "One Song", "Single");
    insert_track(&db, 1, "One Song", "");
    insert_track(&db, 99, "Library Anchor", "Other Album");

    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    let hidden = crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    assert_eq!(hidden, 1);
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

    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();

    let remembered: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remembered, 0);
}

#[test]
fn nr_32_album_memory_also_hides_the_ep_twin() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "Shared Work", "Album");
    insert_release(&db, "ep", "Shared Work", "EP");
    insert_track(&db, 1, "One Song", "Shared Work");
    insert_track(&db, 99, "Library Anchor", "Other Album");

    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();

    assert_eq!(
        crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap(),
        2
    );
    assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 2);
}

#[test]
fn nr_32_show_again_forgets_the_deletion() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Album", "One Album");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    crate::artist_news::set_release_hidden(&db, "album", false).unwrap();
    let hidden_again = crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

    assert_eq!(hidden_again, 0);
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
fn nr_32_reacquiring_the_album_forgets_the_deletion() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_release(&db, "album", "One Album", "Album");
    insert_track(&db, 1, "One Album", "One Album");
    insert_track(&db, 99, "Library Anchor", "Other Album");
    crate::deleted_releases::remember_deleted_releases(db.conn(), &[1], 100).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    crate::deleted_releases::apply_deleted_release_memory(db.conn()).unwrap();

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
