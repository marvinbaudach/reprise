//! Trim-mismatch regression coverage for the display queries in
//! `library_views.rs` that were not part of `#1022`'s delete-path fix.
//!
//! `str::trim` strips a no-break space (U+00A0); SQLite's `TRIM()` does not.
//! Every function tested here used to bind `argument.trim()` in Rust and
//! compare it against a SQL-side `TRIM(...)` column, so an artist or album
//! tag ending in a no-break space would resolve to the plain-spelled row's
//! content instead of its own. Kept as a sibling module, not appended to
//! `library_views_tests.rs`, to stay clear of that file's 800-line cap.

use super::*;
use crate::queries::{
    query_track_count, query_track_ids, query_track_window, test_rows, test_sort,
};
use crate::view_source::ViewSource;

fn full_window() -> WindowRange {
    WindowRange {
        offset: 0,
        limit: MAX_WINDOW_LIMIT,
    }
}

/// Two effective artists that the artist list shows as separate rows: a
/// plain "Artist" and an "Artist" tag followed by a no-break space. Each has
/// a distinct album and untagged-track count so a query that wrongly
/// collapses them onto the plain row is caught by the numbers, not just by
/// row identity.
fn seeded_artists_with_nbsp_sibling() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks
           (id,path,title,artist,album,album_artist,added_at,missing_since) VALUES
         (10,'/music/a1.flac','A1','Artist','AlbumOne','',0,NULL),
         (11,'/music/a2.flac','A2','Artist','AlbumTwo','',0,NULL),
         (12,'/music/u1.flac','U1','Artist','','',0,NULL),
         (13,'/music/u2.flac','U2','Artist','','',0,NULL),
         (20,'/music/na.flac','N-A','Artist\u{a0}','AlbumThree\u{a0}','',0,NULL),
         (21,'/music/nu.flac','N-U','Artist\u{a0}','','',0,NULL);",
    )
    .unwrap();
    db
}

#[test]
fn artist_page_album_window_and_count_trim_like_the_row_they_were_listed_from() {
    let db = seeded_artists_with_nbsp_sibling();

    // The artist list shows two rows here, exactly as in the canonical-id
    // regression test.
    assert_eq!(query_artists(&db, "", full_window()).unwrap().total, 2);

    assert_eq!(query_artist_album_count(&db, "Artist").unwrap(), 2);
    assert_eq!(query_artist_album_count(&db, "Artist\u{a0}").unwrap(), 1);
    assert_eq!(
        query_artist_albums(&db, "Artist", full_window())
            .unwrap()
            .rows
            .len(),
        2
    );
    assert_eq!(
        query_artist_albums(&db, "Artist\u{a0}", full_window())
            .unwrap()
            .rows
            .len(),
        1
    );

    // Ordinary padding must still be tolerated, so moving the trim into SQL
    // does not regress the existing " Artist " leniency.
    assert_eq!(query_artist_album_count(&db, " Artist ").unwrap(), 2);
}

#[test]
fn artist_page_untagged_window_and_count_trim_like_the_row_they_were_listed_from() {
    let db = seeded_artists_with_nbsp_sibling();

    assert_eq!(query_artist_untagged_track_count(&db, "Artist").unwrap(), 2);
    assert_eq!(
        query_artist_untagged_track_count(&db, "Artist\u{a0}").unwrap(),
        1
    );
    assert_eq!(
        query_artist_untagged_tracks(&db, "Artist", full_window())
            .unwrap()
            .rows
            .len(),
        2
    );
    assert_eq!(
        query_artist_untagged_tracks(&db, "Artist\u{a0}", full_window())
            .unwrap()
            .rows
            .len(),
        1
    );

    assert_eq!(
        query_artist_untagged_track_count(&db, " Artist ").unwrap(),
        2
    );
}

#[test]
fn artist_detail_albums_trim_like_the_row_it_was_listed_from() {
    let db = seeded_artists_with_nbsp_sibling();

    assert_eq!(query_artist_detail_albums(&db, "Artist").unwrap().len(), 2);
    assert_eq!(
        query_artist_detail_albums(&db, "Artist\u{a0}")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        query_artist_detail_albums(&db, " Artist ").unwrap().len(),
        2
    );
}

#[test]
fn artist_track_window_count_and_ids_trim_like_the_row_they_were_listed_from() {
    let db = seeded_artists_with_nbsp_sibling();
    let plain = ViewSource::Artist("Artist".into());
    let nbsp = ViewSource::Artist("Artist\u{a0}".into());
    let padded = ViewSource::Artist(" Artist ".into());

    assert_eq!(
        query_track_count(&db, &TrackViewQuery::new(&plain)).unwrap(),
        4
    );
    assert_eq!(
        query_track_count(&db, &TrackViewQuery::new(&nbsp)).unwrap(),
        2
    );
    assert_eq!(
        query_track_count(&db, &TrackViewQuery::new(&padded)).unwrap(),
        4
    );

    assert_eq!(
        query_track_window(
            &db,
            &TrackViewQuery::new(&plain),
            test_sort("title", "asc"),
            test_rows(0, 20),
            AiColumn::Project
        )
        .unwrap()
        .into_iter()
        .map(|track| track.title)
        .collect::<Vec<_>>(),
        ["A1", "A2", "U1", "U2"]
    );
    assert_eq!(
        query_track_window(
            &db,
            &TrackViewQuery::new(&nbsp),
            test_sort("title", "asc"),
            test_rows(0, 20),
            AiColumn::Project
        )
        .unwrap()
        .into_iter()
        .map(|track| track.title)
        .collect::<Vec<_>>(),
        ["N-A", "N-U"]
    );

    assert_eq!(
        query_track_ids(&db, &TrackViewQuery::new(&plain), test_sort("title", "asc")).unwrap(),
        [10, 11, 12, 13]
    );
    assert_eq!(
        query_track_ids(&db, &TrackViewQuery::new(&nbsp), test_sort("title", "asc")).unwrap(),
        [20, 21]
    );
}

/// Same asymmetry on the album side: an album/album-artist pair tagged with
/// a trailing no-break space must not resolve to its plain-spelled sibling's
/// tracks in the Album detail view's window, count or queue-order id list.
#[test]
fn album_track_window_count_and_ids_trim_like_the_row_they_were_listed_from() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks
           (id,path,title,artist,album,album_artist,track_no,added_at,missing_since) VALUES
         (30,'/music/x1.flac','X1','Zed','AlbumX','Zed',1,0,NULL),
         (31,'/music/x2.flac','X2','Zed','AlbumX','Zed',2,0,NULL),
         (32,'/music/nx.flac','NX1','Zed\u{a0}','AlbumX\u{a0}','Zed\u{a0}',1,0,NULL);",
    )
    .unwrap();

    let plain = ViewSource::Album {
        album: "AlbumX".into(),
        album_artist: "Zed".into(),
    };
    let nbsp = ViewSource::Album {
        album: "AlbumX\u{a0}".into(),
        album_artist: "Zed\u{a0}".into(),
    };

    assert_eq!(
        query_track_count(&db, &TrackViewQuery::new(&plain)).unwrap(),
        2
    );
    assert_eq!(
        query_track_count(&db, &TrackViewQuery::new(&nbsp)).unwrap(),
        1
    );

    assert_eq!(
        query_track_window(
            &db,
            &TrackViewQuery::new(&plain),
            test_sort("title", "asc"),
            test_rows(0, 20),
            AiColumn::Project
        )
        .unwrap()
        .into_iter()
        .map(|track| track.title)
        .collect::<Vec<_>>(),
        ["X1", "X2"]
    );
    assert_eq!(
        query_track_window(
            &db,
            &TrackViewQuery::new(&nbsp),
            test_sort("title", "asc"),
            test_rows(0, 20),
            AiColumn::Project
        )
        .unwrap()
        .into_iter()
        .map(|track| track.title)
        .collect::<Vec<_>>(),
        ["NX1"]
    );

    assert_eq!(
        query_track_ids(&db, &TrackViewQuery::new(&plain), test_sort("title", "asc")).unwrap(),
        [30, 31]
    );
    assert_eq!(
        query_track_ids(&db, &TrackViewQuery::new(&nbsp), test_sort("title", "asc")).unwrap(),
        [32]
    );
}
