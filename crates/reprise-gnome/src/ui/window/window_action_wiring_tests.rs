use super::*;

fn test_conn() -> Rc<RefCell<Connection>> {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    Rc::new(RefCell::new(conn))
}

fn insert_track(conn: &Connection, id: i64, artist: &str) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album_artist, genre, duration_ms, added_at) \
         VALUES (?1, ?2, ?3, ?4, '', 'Metal', 180000, 1)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            artist
        ],
    )
    .unwrap();
}

#[test]
fn spotlight_play_uses_the_group_track_ids() {
    let conn = test_conn();
    insert_track(&conn.borrow(), 1, "Lorna Shore");
    insert_track(&conn.borrow(), 2, "Lorna\tShore");

    let group_ids = stats_spotlight_track_ids(&conn, "name:lorna shore").unwrap();
    let label_ids = artist_track_ids(&conn, "Lorna Shore".to_string()).unwrap();

    assert_eq!(group_ids, vec![1, 2]);
    assert!(group_ids.len() > label_ids.len());
    assert!(label_ids.iter().all(|id| group_ids.contains(id)));
}

#[test]
fn smart_mix_cta_creates_a_genre_smart_playlist() {
    let conn = test_conn();
    let id = create_stats_smart_mix(&conn.borrow(), &["Metal".to_string()])
        .unwrap()
        .expect("a top genre creates a mix");
    let playlist = reprise_core::library::playlists::list_smart(&conn.borrow())
        .unwrap()
        .into_iter()
        .find(|playlist| playlist.id == id)
        .unwrap();

    assert!(playlist.rules_json.contains("genre"));
    assert!(playlist.rules_json.contains("Metal"));
}

#[test]
fn unify_spellings_callback_opens_the_tag_editor_for_the_group_ids() {
    let conn = test_conn();
    insert_track(&conn.borrow(), 1, "Lorna Shore");
    insert_track(&conn.borrow(), 2, "lorna shore ");
    let before = tag_snapshot(&conn.borrow());
    let changes_before = conn.borrow().total_changes();
    let ids = stats_spotlight_track_ids(&conn, "name:lorna shore").unwrap();
    let mut forwarded = Vec::new();

    forward_unify_spellings(&ids, |received| forwarded = received.to_vec());

    assert_eq!(forwarded, ids);
    assert_eq!(tag_snapshot(&conn.borrow()), before);
    assert_eq!(conn.borrow().total_changes(), changes_before);
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
