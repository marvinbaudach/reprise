use super::*;

#[test]
fn fil_1c_genre_source_remains_restricted_after_facets_are_cleared() {
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, genre) in [(1, "Metalcore"), (2, "Metalcore"), (3, "Jazz")] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, genre, added_at)
             VALUES (?1, ?2, ?3, 'Artist', ?4, 0)",
            rusqlite::params![id, format!("/x/{id}.flac"), format!("Track {id}"), genre],
        )
        .unwrap();
    }

    let source = ViewSource::Genre("Metalcore".into());
    assert_eq!(
        query_track_count_browsed(&conn, &source, "", &BrowseFilter::default(), &[]).unwrap(),
        2
    );
    let rows = query_track_window_browsed(
        &mut conn,
        &source,
        "title",
        "asc",
        "",
        &BrowseFilter::default(),
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|track| track.genre == "Metalcore"));
    assert_eq!(
        query_track_ids_browsed(
            &conn,
            &source,
            "title",
            "asc",
            "",
            &BrowseFilter::default(),
            &[],
        )
        .unwrap(),
        vec![1, 2]
    );
}
