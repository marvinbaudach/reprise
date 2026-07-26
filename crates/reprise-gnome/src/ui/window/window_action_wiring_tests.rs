use super::*;

fn test_conn() -> Rc<RefCell<Connection>> {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    Rc::new(RefCell::new(conn))
}

fn insert_track(conn: &Connection, id: i64, artist: &str) {
    insert_track_with_genre(conn, id, artist, "Metal");
}

fn insert_track_with_genre(conn: &Connection, id: i64, artist: &str, genre: &str) {
    conn.execute(
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
    insert_track(&conn.borrow(), 7, "Artist");
    conn.borrow()
        .execute(
            "UPDATE tracks SET album = 'Album', album_artist = 'Album Artist' WHERE id = 7",
            [],
        )
        .unwrap();

    assert_eq!(
        stats_album_target_for_path(&conn.borrow(), "/music/7.flac").unwrap(),
        Some((7, "Album".into(), "Album Artist".into()))
    );
    assert_eq!(
        stats_album_target_for_path(&conn.borrow(), "/missing.flac").unwrap(),
        None
    );
}
