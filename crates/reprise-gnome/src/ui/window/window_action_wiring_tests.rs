use super::*;
use reprise_core::db::Db;

fn test_conn() -> Rc<Db> {
    Rc::new(crate::test_db::open().unwrap())
}

fn insert_track(db: &Db, id: i64, artist: &str) {
    insert_track_with_genre(db, id, artist, "Metal");
}

fn insert_track_with_genre(db: &Db, id: i64, artist: &str, genre: &str) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO tracks \
         (id, path, title, artist, album_artist, genre, duration_ms, added_at) \
         VALUES (?1, ?2, ?3, ?4, '', ?5, 180000, 1)",
            rusqlite::params![
                id,
                format!("/music/{id}.flac"),
                format!("Track {id}"),
                artist,
                genre
            ],
        )
        .unwrap();
}

#[test]
fn genre_cover_path_resolves_the_album_navigation_target() {
    let conn = test_conn();
    insert_track(&conn, 7, "Artist");
    crate::test_db::connection(&conn)
        .execute(
            "UPDATE tracks SET album = 'Album', album_artist = 'Album Artist' WHERE id = 7",
            [],
        )
        .unwrap();

    assert_eq!(
        query_stats_album_target_for_path(&conn, "/music/7.flac").unwrap(),
        Some((7, "Album".into(), "Album Artist".into()))
    );
    assert_eq!(
        query_stats_album_target_for_path(&conn, "/missing.flac").unwrap(),
        None
    );
}
