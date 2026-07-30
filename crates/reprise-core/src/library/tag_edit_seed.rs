use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, Row};

use super::tag_edit::EditableTags;
use crate::db::Db;

/// Database-backed values needed to start a tag-editing session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackEditSeed {
    pub id: i64,
    pub path: PathBuf,
    pub tags: EditableTags,
    pub rating: i32,
    pub bitrate_kbps: Option<u32>,
}

pub fn track_edit_seed_by_id(db: &Db, id: i64) -> Result<Option<TrackEditSeed>, rusqlite::Error> {
    let conn = db.conn();
    track_edit_seed_by_id_in(conn, id)
}

fn track_edit_seed_by_id_in(
    conn: &Connection,
    id: i64,
) -> Result<Option<TrackEditSeed>, rusqlite::Error> {
    conn.query_row(
        "SELECT id,path,title,artist,album,album_artist,year,track_no,genre,rating,bitrate_kbps
         FROM tracks
         WHERE id = ?1",
        [id],
        track_edit_seed_from_row,
    )
    .optional()
}

pub fn live_track_edit_seed_by_path(
    db: &Db,
    path: &str,
) -> Result<Option<TrackEditSeed>, rusqlite::Error> {
    let conn = db.conn();
    live_track_edit_seed_by_path_in(conn, path)
}

fn live_track_edit_seed_by_path_in(
    conn: &Connection,
    path: &str,
) -> Result<Option<TrackEditSeed>, rusqlite::Error> {
    conn.query_row(
        "SELECT id,path,title,artist,album,album_artist,year,track_no,genre,rating,bitrate_kbps
         FROM tracks
         WHERE path = ?1 AND removed_at IS NULL",
        [path],
        track_edit_seed_from_row,
    )
    .optional()
}

fn track_edit_seed_from_row(row: &Row<'_>) -> rusqlite::Result<TrackEditSeed> {
    let year = row
        .get::<_, Option<i32>>(6)?
        .and_then(|value| u32::try_from(value).ok());
    let track_no = row
        .get::<_, Option<i32>>(7)?
        .and_then(|value| u32::try_from(value).ok());
    let bitrate_kbps = row
        .get::<_, Option<i64>>(10)?
        .and_then(|value| u32::try_from(value).ok());
    Ok(TrackEditSeed {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        tags: EditableTags {
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            album_artist: row.get(5)?,
            year,
            track_no,
            genre: row.get(8)?,
        },
        rating: row.get(9)?,
        bitrate_kbps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_preserve_live_path_and_numeric_conversion_contracts() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO tracks
                    (id,path,title,artist,album,album_artist,year,track_no,genre,rating,
                     bitrate_kbps,added_at,removed_at)
                 VALUES
                    (41,'/present.flac','Title','Artist','Album','Album Artist',
                     2001,7,'Genre',4,320,0,NULL),
                    (42,'/removed.flac','Gone','Artist','Album','',
                     -1,-2,'Genre',2,-3,0,1);",
            )
            .unwrap();

        let present = track_edit_seed_by_id(&db, 41).unwrap().unwrap();
        assert_eq!(present.id, 41);
        assert_eq!(present.path, PathBuf::from("/present.flac"));
        assert_eq!(present.tags.title, "Title");
        assert_eq!(present.tags.year, Some(2001));
        assert_eq!(present.tags.track_no, Some(7));
        assert_eq!(present.rating, 4);
        assert_eq!(present.bitrate_kbps, Some(320));

        let removed = track_edit_seed_by_id(&db, 42).unwrap().unwrap();
        assert_eq!(removed.tags.year, None);
        assert_eq!(removed.tags.track_no, None);
        assert_eq!(removed.bitrate_kbps, None);
        assert!(live_track_edit_seed_by_path(&db, "/removed.flac")
            .unwrap()
            .is_none());
        assert_eq!(
            live_track_edit_seed_by_path(&db, "/present.flac")
                .unwrap()
                .unwrap(),
            present
        );
    }
}
