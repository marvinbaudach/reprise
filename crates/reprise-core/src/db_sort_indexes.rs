//! Schema migration for indexes serving the library's default sort.

use rusqlite::Connection;

pub(crate) fn migrate_v81(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 81 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
         ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
         WHERE missing_since IS NULL AND removed_at IS NULL;",
    )?;
    transaction.pragma_update(None, "user_version", 81)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX_NAME: &str = "idx_tracks_present_artist_order";

    fn open_at_v80() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(&format!("DROP INDEX IF EXISTS {INDEX_NAME}"))
            .unwrap();
        conn.pragma_update(None, "user_version", 80).unwrap();
        conn
    }

    fn seed_tracks(conn: &Connection) {
        let transaction = conn.unchecked_transaction().unwrap();
        for id in 1..=500_i64 {
            transaction
                .execute(
                    "INSERT INTO tracks
                     (id, path, title, artist, album, year, track_no, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                    rusqlite::params![
                        id,
                        format!("/music/{id}.flac"),
                        format!("Track {id}"),
                        format!("Artist {}", id % 25),
                        format!("Album {}", id % 50),
                        1980 + id % 40,
                        id % 20,
                    ],
                )
                .unwrap();
        }
        transaction.commit().unwrap();
        conn.execute_batch("ANALYZE").unwrap();
    }

    #[test]
    fn v81_serves_the_default_artist_sort_from_an_index() {
        let conn = open_at_v80();
        seed_tracks(&conn);
        migrate_v81(&conn).unwrap();
        conn.execute_batch("ANALYZE").unwrap();

        let query = crate::queries::build_track_query("artist", "ASC", false);
        let details = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
            .unwrap()
            .query_map(rusqlite::params![500, 0], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(
            details.iter().any(|detail| detail.contains(INDEX_NAME)),
            "query plan did not use {INDEX_NAME}: {details:?}"
        );
        assert!(
            details
                .iter()
                .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")),
            "query plan still sorts through a temporary B-tree: {details:?}"
        );
    }

    #[test]
    fn v81_is_idempotent_and_bumps_the_schema_version() {
        let conn = open_at_v80();

        migrate_v81(&conn).unwrap();
        migrate_v81(&conn).unwrap();

        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            81
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [INDEX_NAME],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
    }
}
