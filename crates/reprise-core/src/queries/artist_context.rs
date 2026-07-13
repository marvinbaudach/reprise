use rusqlite::Connection;

/// Returns the distinct, non-empty local albums credited either directly to
/// `artist` or through the album-artist field. Missing tracks are ignored.
pub fn query_artist_albums(
    conn: &Connection,
    artist: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT MIN(TRIM(album))
         FROM tracks
         WHERE missing = 0
           AND TRIM(album) <> ''
           AND (artist = ?1 COLLATE NOCASE OR album_artist = ?1 COLLATE NOCASE)
         GROUP BY LOWER(TRIM(album))
         ORDER BY LOWER(TRIM(album)) ASC",
    )?;
    let albums = statement
        .query_map([artist], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(albums)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artist_albums_match_artist_and_album_artist_and_deduplicate() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,album_artist,added_at) VALUES
             ('/a','A','The Band','First','','0'),
             ('/b','B','Member','Second','The Band','0'),
             ('/c','C','the band',' first ','','0'),
             ('/d','D','The Band','','','0');",
        )
        .unwrap();
        assert_eq!(
            query_artist_albums(&conn, "THE BAND").unwrap(),
            ["First", "Second"]
        );
    }

    #[test]
    fn artist_albums_exclude_missing_tracks() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path,title,artist,album,added_at,missing) VALUES ('/a','A','Artist','Gone',0,1)",
            [],
        )
        .unwrap();
        assert!(query_artist_albums(&conn, "Artist").unwrap().is_empty());
    }
}
