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

fn metal_group() -> TopGenre {
    TopGenre {
        key: "name:metal".to_string(),
        label: "Metal".to_string(),
    }
}

fn source_track_ids(conn: &Connection, source: &ViewSource) -> Vec<i64> {
    let mut ids =
        reprise_core::queries::query_track_ids(conn, source, "title", "asc", "", &[]).unwrap();
    ids.sort_unstable();
    ids
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

/// A genre group written one single way is expressible as a rule, so the CTA
/// creates a real smart playlist — and it must hold exactly the tracks the
/// screen counted, not the ones an exact-case label comparison happens to hit.
#[test]
fn smart_mix_cta_creates_a_genre_smart_playlist() {
    let conn = test_conn();
    insert_track_with_genre(&conn.borrow(), 1, "A", "Metal");
    insert_track_with_genre(&conn.borrow(), 2, "B", "Metal");
    insert_track_with_genre(&conn.borrow(), 3, "C", "Jazz");

    let source = create_stats_smart_mix(&mut conn.borrow_mut(), &metal_group())
        .unwrap()
        .expect("a top genre creates a mix");

    let ViewSource::Smart(id) = source else {
        panic!("a single-spelling genre group is expressible as a rule: {source:?}");
    };
    let playlist = playlists::list_smart(&conn.borrow())
        .unwrap()
        .into_iter()
        .find(|playlist| playlist.id == id)
        .unwrap();
    assert!(playlist.rules_json.contains("genre"));
    assert_eq!(
        source_track_ids(&conn.borrow(), &source),
        group_track_ids(&conn.borrow(), GroupKind::Genre, "name:metal").unwrap()
    );
}

/// Several spellings fold into one group on screen, but the rule engine joins
/// rules with `AND` and has no alternation, so no rule set can express the
/// group. The mix then holds the group's tracks literally — what it must never
/// do is silently drop the spellings a single `genre = ?` misses.
#[test]
fn smart_mix_cta_covers_every_spelling_of_the_genre_group() {
    let conn = test_conn();
    insert_track_with_genre(&conn.borrow(), 1, "A", "Metal");
    insert_track_with_genre(&conn.borrow(), 2, "B", "metal");
    insert_track_with_genre(&conn.borrow(), 3, "C", " Metal ");
    insert_track_with_genre(&conn.borrow(), 4, "D", "Jazz");
    let group = group_track_ids(&conn.borrow(), GroupKind::Genre, "name:metal").unwrap();
    assert_eq!(group, vec![1, 2, 3]);

    let source = create_stats_smart_mix(&mut conn.borrow_mut(), &metal_group())
        .unwrap()
        .expect("a top genre creates a mix");

    assert_eq!(source_track_ids(&conn.borrow(), &source), group);
}

/// An empty group has nothing to mix and must not leave an empty playlist
/// behind.
#[test]
fn smart_mix_cta_creates_nothing_for_a_genre_without_tracks() {
    let conn = test_conn();
    insert_track_with_genre(&conn.borrow(), 1, "A", "Jazz");

    let created = create_stats_smart_mix(&mut conn.borrow_mut(), &metal_group()).unwrap();

    assert!(created.is_none());
    assert!(!playlists::list_smart(&conn.borrow())
        .unwrap()
        .iter()
        .any(|playlist| playlist.name.contains("My Stats")));
    assert!(!playlists::list(&conn.borrow())
        .unwrap()
        .iter()
        .any(|playlist| playlist.name.contains("My Stats")));
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

    let ids = stats_spotlight_track_ids(&conn, "name:lorna shore").unwrap();

    assert_eq!(ids, vec![1, 2]);
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
