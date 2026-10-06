//! Schema migration for the index that serves track-first lookups in
//! `playlist_tracks`, whose primary key leads with `playlist_id`.

use rusqlite::Connection;

pub(crate) fn migrate_v88(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 88 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track
         ON playlist_tracks(track_id);",
    )?;
    transaction.pragma_update(None, "user_version", 88)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX_NAME: &str = "idx_playlist_tracks_track";

    fn open_without_index_at(version: i64) -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(&format!("DROP INDEX IF EXISTS {INDEX_NAME}"))
            .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        conn
    }

    fn index_count(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [INDEX_NAME],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    fn schema_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA schema_version", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn v88_creates_the_index_and_bumps_the_schema_version() {
        let conn = open_without_index_at(87);
        assert_eq!(index_count(&conn), 0);

        migrate_v88(&conn).unwrap();

        assert_eq!(index_count(&conn), 1);
        assert_eq!(user_version(&conn), 88);
    }

    #[test]
    fn v88_is_a_no_op_on_a_second_run() {
        let conn = open_without_index_at(87);
        migrate_v88(&conn).unwrap();
        let schema_after_first_run = schema_version(&conn);

        migrate_v88(&conn).unwrap();

        assert_eq!(index_count(&conn), 1);
        assert_eq!(user_version(&conn), 88);
        assert_eq!(schema_version(&conn), schema_after_first_run);
    }

    #[test]
    fn v88_serves_the_delete_time_lookup_from_the_index() {
        let conn = open_without_index_at(87);
        migrate_v88(&conn).unwrap();

        let details = conn
            .prepare(
                "EXPLAIN QUERY PLAN SELECT playlist_id FROM playlist_tracks WHERE track_id = ?1",
            )
            .unwrap()
            .query_map([1_i64], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(
            details.iter().any(|detail| detail.contains(INDEX_NAME)),
            "query plan did not use {INDEX_NAME}: {details:?}"
        );
    }

    #[test]
    fn a_fresh_database_has_the_index() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();

        assert_eq!(index_count(&conn), 1);
        assert_eq!(user_version(&conn), crate::db::SUPPORTED_SCHEMA_VERSION);
    }

    #[test]
    fn migration_chain_upgrades_v87_to_the_current_schema() {
        let conn = open_without_index_at(87);

        crate::db::migrate_connection(&conn).unwrap();

        assert_eq!(index_count(&conn), 1);
        assert_eq!(user_version(&conn), crate::db::SUPPORTED_SCHEMA_VERSION);
    }
}
