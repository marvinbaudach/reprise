use super::*;

#[test]
fn fil_1c_genre_source_remains_restricted_after_facets_are_cleared() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
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
        query_track_count(
            &db,
            &TrackViewQuery::new(&source).with_browse(&BrowseFilter::default())
        )
        .unwrap(),
        2
    );
    let rows = query_track_window(
        &db,
        &TrackViewQuery::new(&source).with_browse(&BrowseFilter::default()),
        test_sort("title", "asc"),
        test_rows(0, 10),
        AiColumn::Project,
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|track| track.genre == "Metalcore"));
    assert_eq!(
        query_track_ids(
            &db,
            &TrackViewQuery::new(&source).with_browse(&BrowseFilter::default()),
            test_sort("title", "asc")
        )
        .unwrap(),
        vec![1, 2]
    );
}
