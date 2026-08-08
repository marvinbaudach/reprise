//! Schema storage for phone-to-desktop listening reports.

use rusqlite::Connection;

const ADD_RATED_AT: &str = "ALTER TABLE tracks ADD COLUMN rated_at INTEGER";
const CREATE_REPORT_STATE: &str = r#"
CREATE TABLE IF NOT EXISTS device_listen_report_state (
  device_serial    TEXT PRIMARY KEY,
  applied_sequence BLOB NOT NULL
                   CHECK (typeof(applied_sequence) = 'blob' AND length(applied_sequence) = 8)
);
"#;

pub(crate) fn migrate_v63(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 63 {
        return Ok(());
    }
    let has_rated_at = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'rated_at'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    let transaction = conn.unchecked_transaction()?;
    if !has_rated_at {
        transaction.execute(ADD_RATED_AT, [])?;
    }
    transaction.execute_batch(CREATE_REPORT_STATE)?;
    transaction.pragma_update(None, "user_version", 63)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    #[test]
    fn v62_upgrade_timestamps_future_ratings_and_starts_each_device_unacknowledged() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "DROP TABLE device_listen_report_state;
             ALTER TABLE tracks DROP COLUMN rated_at;
             PRAGMA user_version = 62;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, rating, added_at) \
             VALUES (1, '/music/old.flac', 'Old', 'Artist', 4, 1)",
            [],
        )
        .unwrap();

        super::migrate_v63(&conn).unwrap();
        super::migrate_v63(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let rated_at: Option<i64> = conn
            .query_row("SELECT rated_at FROM tracks WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let states: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM device_listen_report_state",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let sequence_type: String = conn
            .query_row(
                "SELECT type FROM pragma_table_info('device_listen_report_state')
                  WHERE name = 'applied_sequence'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 63);
        assert_eq!(rated_at, None, "an old rating has no knowable timestamp");
        assert_eq!(states, 0, "each device starts without an acknowledgement");
        assert_eq!(
            sequence_type, "BLOB",
            "SQLite INTEGER cannot hold every u64"
        );
    }
}
