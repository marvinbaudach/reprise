use super::clauses::PRESENT;
use crate::db::Db;
use rusqlite::{Connection, OptionalExtension};

/// Returns the distinct, non-empty local albums credited either directly to
/// `artist` or through the album-artist field. Missing tracks are ignored.
pub fn query_artist_albums(db: &Db, artist: &str) -> Result<Vec<String>, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(&format!(
        "SELECT MIN(TRIM(album))
         FROM tracks
         WHERE {PRESENT}
           AND TRIM(album) <> ''
           AND (artist = ?1 COLLATE NOCASE OR album_artist = ?1 COLLATE NOCASE)
         GROUP BY LOWER(TRIM(album))
         ORDER BY LOWER(TRIM(album)) ASC"
    ))?;
    let albums = statement
        .query_map([artist], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(albums)
}

/// Resolves the album context used when a track path opens the statistics
/// view. The effective artist prefers a non-blank album artist.
pub fn query_stats_album_target_for_path(
    db: &Db,
    path: &str,
) -> Result<Option<(i64, String, String)>, rusqlite::Error> {
    let conn = db.conn();
    query_stats_album_target_for_path_in(conn, path)
}

fn query_stats_album_target_for_path_in(
    conn: &Connection,
    path: &str,
) -> Result<Option<(i64, String, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, album,
                CASE WHEN TRIM(album_artist) <> ''
                     THEN TRIM(album_artist)
                     ELSE TRIM(artist)
                END
         FROM tracks
         WHERE path = ?1",
        [path],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artist_albums_match_artist_and_album_artist_and_deduplicate() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,album_artist,added_at) VALUES
             ('/a','A','The Band','First','','0'),
             ('/b','B','Member','Second','The Band','0'),
             ('/c','C','the band',' first ','','0'),
             ('/d','D','The Band','','','0');",
        )
        .unwrap();
        assert_eq!(
            query_artist_albums(&db, "THE BAND").unwrap(),
            ["First", "Second"]
        );
    }

    #[test]
    fn artist_albums_exclude_missing_tracks() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO tracks (path,title,artist,album,added_at,missing_since) \
             VALUES ('/a','A','Artist','Gone',0,1)",
            [],
        )
        .unwrap();
        assert!(query_artist_albums(&db, "Artist").unwrap().is_empty());
    }

    #[test]
    fn stats_album_target_prefers_album_artist_and_falls_back_to_track_artist() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO tracks
                    (id,path,title,artist,album,album_artist,added_at)
                 VALUES
                    (7,'/album-artist','A','Track Artist','Album','  Album Artist  ',0),
                    (8,'/track-artist','B','  Track Artist  ','Album','',0);",
            )
            .unwrap();

        assert_eq!(
            query_stats_album_target_for_path(&db, "/album-artist").unwrap(),
            Some((7, "Album".into(), "Album Artist".into()))
        );
        assert_eq!(
            query_stats_album_target_for_path(&db, "/track-artist").unwrap(),
            Some((8, "Album".into(), "Track Artist".into()))
        );
        assert_eq!(
            query_stats_album_target_for_path(&db, "/missing").unwrap(),
            None
        );
    }
}
