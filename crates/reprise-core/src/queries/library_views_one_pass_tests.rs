//! The one-pass artist and album lists must equal the windowed queries read to
//! the end. Kept as a sibling module, not appended to `library_views_tests.rs`,
//! to stay clear of that file's 800-line cap.

use super::*;

const SMALL_LIMIT: i64 = 2;

fn library() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks
               (id,path,title,artist,album,album_artist,year,duration_ms,play_count,
                last_played_at,added_at,missing_since) VALUES
             (1,'/m/1.flac','A','Solo',' First ','',1999,100,2,50,10,NULL),
             (2,'/m/2.flac','B','Solo','first','',2001,200,1,70,20,NULL),
             (3,'/m/3.flac','C','Other Artist','First','',0,300,0,0,30,NULL),
             (4,'/m/4.flac','D','Guest A','Compilation','Various Artists',2005,400,0,0,40,NULL),
             (5,'/m/5.flac','E','Guest B','Compilation','Various Artists',2005,500,4,90,50,NULL),
             (6,'/m/6.flac','F','Nobody','','',0,600,0,0,60,NULL),
             (7,'/m/7.flac','G','Solo','Lost','',0,700,0,0,70,999999999),
             (8,'/m/8.flac','H','zed','Zulu','',2010,800,0,0,80,NULL),
             (9,'/m/9.flac','I','Ärzte','Hits','',2011,900,0,0,90,NULL);",
        )
        .unwrap();
    db
}

fn read_every_artist_window(db: &crate::db::Db) -> Vec<ArtistSummary> {
    let mut offset = 0;
    let mut rows = Vec::new();
    loop {
        let window = query_artists(
            db,
            "",
            WindowRange {
                offset,
                limit: SMALL_LIMIT,
            },
        )
        .unwrap();
        offset += i64::try_from(window.rows.len()).unwrap();
        rows.extend(window.rows);
        if !window.has_more {
            return rows;
        }
    }
}

fn read_every_album_window(db: &crate::db::Db) -> Vec<AlbumSummary> {
    let mut offset = 0;
    let mut rows = Vec::new();
    loop {
        let window = query_albums(
            db,
            "",
            WindowRange {
                offset,
                limit: SMALL_LIMIT,
            },
        )
        .unwrap();
        offset += i64::try_from(window.rows.len()).unwrap();
        rows.extend(window.rows);
        if !window.has_more {
            return rows;
        }
    }
}

#[test]
fn all_artists_equal_every_window_of_the_windowed_query() {
    let db = library();
    let windowed = read_every_artist_window(&db);
    assert!(windowed.len() > 2 * SMALL_LIMIT as usize, "several windows");
    assert_eq!(query_all_artists(&db).unwrap(), windowed);
}

#[test]
fn all_albums_equal_every_window_of_the_windowed_query() {
    let db = library();
    let windowed = read_every_album_window(&db);
    assert!(windowed.len() > 2 * SMALL_LIMIT as usize, "several windows");
    assert_eq!(query_all_albums(&db).unwrap(), windowed);
}

#[test]
fn the_one_pass_lists_are_empty_for_an_empty_library() {
    let db = crate::db::Db::open_in_memory().unwrap();
    assert!(query_all_artists(&db).unwrap().is_empty());
    assert!(query_all_albums(&db).unwrap().is_empty());
}
