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

/// The unify hint is a suggestion: it resolves the group it would open the tag
/// editor for and writes nothing itself.
#[test]
fn unify_spellings_resolves_the_group_ids_without_writing_tags() {
    let conn = test_conn();
    insert_track(&conn.borrow(), 1, "Lorna Shore");
    insert_track(&conn.borrow(), 2, "lorna shore ");
    let before = tag_snapshot(&conn.borrow());
    let changes_before = conn.borrow().total_changes();

    let ids = reprise_core::library::stats_screen::group_track_ids(
        &conn.borrow(),
        reprise_core::library::group_key::GroupKind::Artist,
        "name:lorna shore",
    )
    .unwrap();

    assert_eq!(ids, vec![1, 2]);
    assert_eq!(tag_snapshot(&conn.borrow()), before);
    assert_eq!(conn.borrow().total_changes(), changes_before);
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

fn tag_snapshot(conn: &Connection) -> Vec<(String, String, String)> {
    let mut statement = conn
        .prepare("SELECT artist, album_artist, genre FROM tracks ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
