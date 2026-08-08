use super::*;

// UX FIL-1d: Music free text names what a row is called. Genre is a
// classification and belongs to the dedicated facet instead.
#[test]
fn fil_1d_music_free_text_matches_title_artist_and_album_but_not_genre() {
    let db = crate::db::Db::open_in_memory().unwrap();
    for (id, title, artist, album, genre) in [
        (
            1,
            "Needle title",
            "Other artist",
            "Other album",
            "Other genre",
        ),
        (
            2,
            "Other title",
            "Needle artist",
            "Other album",
            "Other genre",
        ),
        (
            3,
            "Other title",
            "Other artist",
            "Needle album",
            "Other genre",
        ),
        (
            4,
            "Other title",
            "Other artist",
            "Other album",
            "Needle genre",
        ),
    ] {
        db.conn()
            .execute(
                "INSERT INTO tracks (id,path,title,artist,album,genre,added_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,0)",
                rusqlite::params![id, format!("/music/{id}.flac"), title, artist, album, genre],
            )
            .unwrap();
    }

    let rows = query_track_window(
        &db,
        &ViewSource::Library,
        "title",
        "asc",
        "needle",
        0,
        10,
        &[],
    )
    .unwrap();

    assert_eq!(
        rows.iter().map(|track| track.id).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(
        query_track_count(&db, &ViewSource::Library, "needle", &[]).unwrap(),
        3
    );
}

// UX FIL-1d: removing Genre from free text does not remove genre filtering;
// the explicit facet remains the classification seam.
#[test]
fn fil_1d_genre_facet_still_returns_a_genre_only_match() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (id,path,title,artist,album,genre,added_at) \
             VALUES (1,'/music/one.flac','Other title','Other artist','Other album','Needle genre',0)",
            [],
        )
        .unwrap();
    let browse = BrowseFilter {
        genre: Some("Needle genre".to_owned()),
        ..BrowseFilter::default()
    };

    let rows = query_track_window_browsed(
        &db,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        &browse,
        0,
        10,
        &[],
    )
    .unwrap();

    assert_eq!(rows.iter().map(|track| track.id).collect::<Vec<_>>(), [1]);
}
