use super::*;
use crate::queries::{query_track_count, query_track_ids, query_track_window, WindowRange};
use crate::view_source::ViewSource;

fn seeded_library() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks
           (id,path,title,artist,album,album_artist,added_at,missing_since) VALUES
         (1,'/music/first-a.flac','A','Solo',' First ','',0,NULL),
         (2,'/music/first-b.flac','B','Solo','first','',0,NULL),
         (3,'/music/other.flac','Other','Other Artist','First','',0,NULL),
         (4,'/music/mix-a.flac','Mix A','Guest A','Compilation','Various Artists',0,NULL),
         (5,'/music/mix-b.flac','Mix B','Guest B','Compilation','Various Artists',0,NULL),
         (6,'/music/blank.flac','Blank','Nobody','','',0,NULL),
         (7,'/music/missing.flac','Missing','Solo','Lost','',0,999999999);",
    )
    .unwrap();
    db
}

fn full_window() -> WindowRange {
    WindowRange {
        offset: 0,
        limit: MAX_WINDOW_LIMIT,
    }
}

#[test]
fn artist_and_album_counts_match_the_grouped_summaries() {
    let db = seeded_library();
    // These fixture-sized windows include every grouped summary, while
    // the response total still comes from the dedicated count query.
    assert_eq!(
        query_artist_count(&db, "").unwrap(),
        query_artists(&db, "", full_window()).unwrap().rows.len() as i64
    );
    assert_eq!(
        query_album_count(&db, "").unwrap(),
        query_albums(&db, "", full_window()).unwrap().rows.len() as i64
    );
    // Nobody, Other Artist, Solo, Various Artists; the missing "Lost" and
    // blank-album rows are excluded exactly as in the summaries.
    assert_eq!(query_artist_count(&db, "").unwrap(), 4);
    assert_eq!(query_album_count(&db, "").unwrap(), 3);
}

#[test]
fn album_and_artist_summaries_are_counted_bounded_windows() {
    let db = seeded_library();
    let range = WindowRange {
        offset: 0,
        limit: 2,
    };

    let albums = query_albums(&db, "", range).unwrap();
    let artists = query_artists(&db, "", range).unwrap();

    assert_eq!(albums.total, 3);
    assert_eq!(albums.rows.len(), 2);
    assert!(albums.has_more);
    assert_eq!(artists.total, 4);
    assert_eq!(artists.rows.len(), 2);
    assert!(artists.has_more);
}

#[test]
fn albums_group_by_trimmed_case_insensitive_title_and_effective_artist() {
    let db = seeded_library();

    assert_eq!(
        query_albums(&db, "", full_window()).unwrap().rows,
        vec![
            AlbumSummary {
                album: "Compilation".into(),
                album_artist: "Various Artists".into(),
                representative_path: "/music/mix-a.flac".into(),
                track_count: 2,
                year: None,
                total_duration_ms: 0,
                max_added_at: 0,
                total_play_count: 0,
            },
            AlbumSummary {
                album: "First".into(),
                album_artist: "Other Artist".into(),
                representative_path: "/music/other.flac".into(),
                track_count: 1,
                year: None,
                total_duration_ms: 0,
                max_added_at: 0,
                total_play_count: 0,
            },
            AlbumSummary {
                album: "First".into(),
                album_artist: "Solo".into(),
                representative_path: "/music/first-a.flac".into(),
                track_count: 2,
                year: None,
                total_duration_ms: 0,
                max_added_at: 0,
                total_play_count: 0,
            },
        ]
    );
}

#[test]
fn albums_query_is_read_only_and_excludes_blank_or_missing_rows() {
    let db = seeded_library();
    let conn = db.conn();
    let changes_before = conn.total_changes();

    let albums = query_albums(&db, "", full_window()).unwrap().rows;

    assert_eq!(conn.total_changes(), changes_before);
    assert!(albums.iter().all(|album| !album.album.is_empty()));
    assert!(albums.iter().all(|album| album.album != "Lost"));
}

#[test]
fn album_source_count_window_and_ids_select_the_exact_album_artist_group() {
    let db = seeded_library();
    let source = ViewSource::Album {
        album: "FIRST".into(),
        album_artist: "solo".into(),
    };

    assert_eq!(query_track_count(&db, &source, "", &[]).unwrap(), 2);
    assert_eq!(
        query_track_window(&db, &source, "title", "desc", "", 0, 20, &[])
            .unwrap()
            .into_iter()
            .map(|track| track.title)
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
    assert_eq!(
        query_track_ids(&db, &source, "title", "asc", "A", &[]).unwrap(),
        [1]
    );
}

#[test]
fn canonical_album_ids_order_disc_then_track_with_stable_null_fallbacks() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks
           (id,path,title,artist,album,album_artist,disc_no,track_no,added_at,missing_since) VALUES
         (10,'/music/z.flac','D2T1','Artist','Album','Artist',2,1,0,NULL),
         (20,'/music/b.flac','D1T2','Artist','Album','Artist',1,2,0,NULL),
         (30,'/music/c.flac','Legacy T1','Artist','Album','Artist',NULL,1,0,NULL),
         (40,'/music/d.flac','D1 unknown','Artist','Album','Artist',1,NULL,0,NULL),
         (50,'/music/a.flac','Legacy unknown','Artist','Album','Artist',NULL,NULL,0,NULL),
         (60,'/music/first.flac','D1T1','Artist','Album','Artist',1,1,0,NULL),
         (70,'/music/missing.flac','Missing','Artist','Album','Artist',1,1,0,99);",
    )
    .unwrap();

    assert_eq!(
        query_album_canonical_track_ids(&db, "album", "artist").unwrap(),
        [30, 60, 20, 50, 40, 10]
    );
}

#[test]
fn artists_group_by_effective_album_artist_with_aggregates() {
    let db = seeded_library();
    let artists = query_artists(&db, "", full_window()).unwrap().rows;
    let names: Vec<&str> = artists.iter().map(|a| a.artist.as_str()).collect();
    assert_eq!(
        names,
        vec!["Nobody", "Other Artist", "Solo", "Various Artists"]
    );
    let solo = artists.iter().find(|a| a.artist == "Solo").unwrap();
    assert_eq!(solo.track_count, 2);
    assert_eq!(solo.album_count, 1);
    assert_eq!(solo.total_plays, 0);
    let va = artists
        .iter()
        .find(|a| a.artist == "Various Artists")
        .unwrap();
    assert_eq!(va.track_count, 2);
    assert_eq!(va.album_count, 1);
    assert_eq!(va.representative_path, "/music/mix-a.flac");
}

#[test]
fn artists_sum_play_count_and_max_last_played_at_across_group_rows() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks
           (id,path,title,artist,album,album_artist,added_at,play_count,last_played_at) VALUES
         (1,'/music/a.flac','Track A','Solo','Album','',0,3,100),
         (2,'/music/b.flac','Track B','Solo','Album','',0,5,200);",
    )
    .unwrap();

    let artists = query_artists(&db, "", full_window()).unwrap().rows;
    let solo = artists.iter().find(|a| a.artist == "Solo").unwrap();

    // A per-row read (e.g. only the representative row's play_count) or a
    // MIN/first-value bug would yield 3 or 5, not the summed/maxed value.
    assert_eq!(solo.total_plays, 8);
    assert_eq!(solo.last_played_at, 200);
    assert_eq!(solo.representative_path, "/music/a.flac");
}

#[test]
fn artists_query_is_read_only_and_excludes_blank_or_missing_rows() {
    let db = seeded_library();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO tracks (path,title,artist,album,added_at) \
         VALUES ('/music/no-artist.flac','No Artist',' ','First',0)",
        [],
    )
    .unwrap();
    let changes_before = conn.total_changes();

    let artists = query_artists(&db, "", full_window()).unwrap().rows;

    assert_eq!(conn.total_changes(), changes_before);
    assert!(artists.iter().all(|artist| !artist.artist.is_empty()));
    assert_eq!(
        artists
            .iter()
            .find(|artist| artist.artist == "Solo")
            .unwrap()
            .track_count,
        2
    );
}

#[test]
fn artist_source_count_window_and_ids_select_the_exact_artist_group() {
    let db = seeded_library();
    let source = ViewSource::Artist(" SOLO ".into());

    assert_eq!(query_track_count(&db, &source, "", &[]).unwrap(), 2);
    assert_eq!(
        query_track_window(&db, &source, "title", "desc", "", 0, 20, &[])
            .unwrap()
            .into_iter()
            .map(|track| track.title)
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
    assert_eq!(
        query_track_ids(&db, &source, "title", "asc", "A", &[]).unwrap(),
        [1]
    );
}

#[test]
fn albums_include_year_duration_added_and_play_count_aggregates() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks
           (id,path,title,artist,album,album_artist,year,duration_ms,added_at,play_count) VALUES
         (1,'/a.flac','A','Solo','Album','',2020,180000,1000,5),
         (2,'/b.flac','B','Solo','Album','',2020,240000,2000,3),
         (3,'/c.flac','C','Solo','Album','',0,120000,500,0);",
    )
    .unwrap();

    let albums = super::query_albums(&db, "", full_window()).unwrap().rows;
    assert_eq!(albums.len(), 1);
    let album = &albums[0];
    assert_eq!(album.year, Some(2020));
    assert_eq!(album.total_duration_ms, 540000);
    assert_eq!(album.max_added_at, 2000);
    assert_eq!(album.total_play_count, 8);
}

#[test]
fn artist_source_matches_by_effective_album_artist() {
    let db = seeded_library();
    let solo = ViewSource::Artist(" SOLO ".into());
    assert_eq!(query_track_count(&db, &solo, "", &[]).unwrap(), 2);
    let va = ViewSource::Artist("Various Artists".into());
    assert_eq!(query_track_count(&db, &va, "", &[]).unwrap(), 2);
}

#[test]
fn artist_albums_are_newest_first() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks (path,title,artist,album,album_artist,year,added_at) VALUES
         ('/a','A','Solo','Old','Solo',2010,0),
         ('/b','B','Solo','New','Solo',2024,0);",
    )
    .unwrap();
    let albums = query_artist_detail_albums(&db, "Solo").unwrap();
    assert_eq!(
        albums.iter().map(|a| a.album.as_str()).collect::<Vec<_>>(),
        vec!["New", "Old"]
    );
}

#[test]
fn album_filter_matches_titles_and_effective_artists_with_exact_counts() {
    let db = seeded_library();

    let first_page = query_albums(
        &db,
        "FIRST",
        WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap();
    let all_rows = query_albums(&db, "first", full_window()).unwrap();
    let artist_match = query_albums(&db, "various artists", full_window()).unwrap();

    assert_eq!(first_page.total, 2);
    assert_eq!(first_page.rows.len(), 1);
    assert!(first_page.has_more);
    assert_eq!(
        query_album_count(&db, "first").unwrap(),
        all_rows.rows.len() as i64
    );
    assert_eq!(
        artist_match
            .rows
            .iter()
            .map(|album| album.album.as_str())
            .collect::<Vec<_>>(),
        ["Compilation"]
    );
}

#[test]
fn album_filter_treats_like_wildcards_as_literal_text() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (path,title,artist,album,album_artist,added_at) VALUES
             ('/percent','Percent','Artist','100% Hits','Artist',0),
             ('/percent-control','Percent control','Artist','100X Hits','Artist',0),
             ('/underscore','Underscore','Artist','Under_score','Artist',0),
             ('/underscore-control','Underscore control','Artist','UnderXscore','Artist',0);",
        )
        .unwrap();

    assert_eq!(query_albums(&db, "%", full_window()).unwrap().rows.len(), 1);
    assert_eq!(query_albums(&db, "_", full_window()).unwrap().rows.len(), 1);
}

#[test]
fn artist_filter_is_case_insensitive_counted_and_literal() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (path,title,artist,album,album_artist,added_at) VALUES
             ('/slowdive','Track','Guest','Album','Slowdive',0),
             ('/percent','Percent','Guest','Album','50% Artist',0),
             ('/percent-control','Percent control','Guest','Album','50X Artist',0),
             ('/underscore','Underscore','Guest','Album','Under_score',0),
             ('/underscore-control','Underscore control','Guest','Album','UnderXscore',0);",
        )
        .unwrap();

    let slowdive = query_artists(&db, "SLOWDIVE", full_window()).unwrap();
    assert_eq!(slowdive.rows.len(), 1);
    assert_eq!(slowdive.rows[0].artist, "Slowdive");
    assert_eq!(query_artist_count(&db, "slowdive").unwrap(), 1);
    assert_eq!(
        query_artists(&db, "%", full_window()).unwrap().rows.len(),
        1
    );
    assert_eq!(
        query_artists(&db, "_", full_window()).unwrap().rows.len(),
        1
    );
}
