//! Schema migration for indexes serving the library's default sort.

use rusqlite::Connection;

pub(crate) fn migrate_v82(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 82 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
         ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
         WHERE missing_since IS NULL AND removed_at IS NULL;",
    )?;
    transaction.pragma_update(None, "user_version", 82)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX_NAME: &str = "idx_tracks_present_artist_order";

    fn open_without_sort_index_at(version: i64) -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(&format!("DROP INDEX IF EXISTS {INDEX_NAME}"))
            .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
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

    fn replace_sync_events_with_v80_schema(conn: &Connection) {
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_sync_events_run;
             DROP TABLE sync_events;
             CREATE TABLE sync_events (
               run_id       INTEGER NOT NULL,
               kind         TEXT NOT NULL
                 CHECK (kind IN (
                   'skipped','failed','deleted',
                   'conversion_fallback','playlist_write_failed'
                 )),
               track_id     INTEGER,
               device_path  TEXT NOT NULL,
               detail       TEXT NOT NULL
             );
             CREATE INDEX idx_sync_events_run ON sync_events(run_id);
             INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
             VALUES (1, 'failed', 1666, 'Artist/Album/Track.opus', 'copy failed');",
        )
        .unwrap();
    }

    fn schema_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA schema_version", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn v82_serves_the_default_artist_sort_from_an_index() {
        let conn = open_without_sort_index_at(81);
        seed_tracks(&conn);
        migrate_v82(&conn).unwrap();
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
    fn v82_is_idempotent_and_bumps_the_schema_version() {
        let conn = open_without_sort_index_at(81);

        migrate_v82(&conn).unwrap();
        migrate_v82(&conn).unwrap();

        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            82
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

    #[test]
    fn migration_chain_upgrades_v80_through_sync_log_and_sort_index_to_v83() {
        let conn = open_without_sort_index_at(80);
        replace_sync_events_with_v80_schema(&conn);

        crate::db::migrate_connection(&conn).unwrap();

        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            83
        );
        assert_eq!(
            conn.query_row("SELECT kind FROM sync_events", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
            "failed"
        );
        conn.execute(
            "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
             VALUES (1, 'analysis_failed', 1667, 'Track.reprise-analysis', 'copy failed')",
            [],
        )
        .unwrap();
        assert!(conn
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
                 )",
                [INDEX_NAME],
                |row| row.get::<_, bool>(0),
            )
            .unwrap());

        let schema_version_after_first_run = schema_version(&conn);
        crate::db::migrate_connection(&conn).unwrap();
        assert_eq!(schema_version(&conn), schema_version_after_first_run);
    }

    #[test]
    fn migration_chain_upgrades_v81_through_the_sort_index_to_v83() {
        let conn = open_without_sort_index_at(81);

        crate::db::migrate_connection(&conn).unwrap();

        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            83
        );
        assert!(conn
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
                 )",
                [INDEX_NAME],
                |row| row.get::<_, bool>(0),
            )
            .unwrap());
    }
}
