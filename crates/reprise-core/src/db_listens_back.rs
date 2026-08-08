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
const DROP_RATINGS_BACK: &str = "ALTER TABLE device_settings DROP COLUMN ratings_back";
const ADD_LISTENS_APPLIED: &str = "ALTER TABLE sync_runs ADD COLUMN listens_applied \
                                  INTEGER NOT NULL DEFAULT 0 CHECK (listens_applied >= 0)";
const ADD_RATINGS_APPLIED: &str = "ALTER TABLE sync_runs ADD COLUMN ratings_applied \
                                  INTEGER NOT NULL DEFAULT 0 CHECK (ratings_applied >= 0)";

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

pub(crate) fn migrate_v65(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 65 {
        return Ok(());
    }
    let has_ratings_back = has_column(conn, "device_settings", "ratings_back")?;
    let has_listens_applied = has_column(conn, "sync_runs", "listens_applied")?;
    let has_ratings_applied = has_column(conn, "sync_runs", "ratings_applied")?;
    let transaction = conn.unchecked_transaction()?;
    if has_ratings_back {
        transaction.execute(DROP_RATINGS_BACK, [])?;
    }
    if !has_listens_applied {
        transaction.execute(ADD_LISTENS_APPLIED, [])?;
    }
    if !has_ratings_applied {
        transaction.execute(ADD_RATINGS_APPLIED, [])?;
    }
    transaction.pragma_update(None, "user_version", 65)?;
    transaction.commit()
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2
         )",
        rusqlite::params![table, column],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn v64_upgrade_retires_the_hidden_rating_switch_and_adds_return_counts() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        if !super::has_column(&conn, "device_settings", "ratings_back").unwrap() {
            conn.execute(
                "ALTER TABLE device_settings ADD COLUMN ratings_back \
                 INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .unwrap();
        }
        if super::has_column(&conn, "sync_runs", "listens_applied").unwrap() {
            conn.execute("ALTER TABLE sync_runs DROP COLUMN listens_applied", [])
                .unwrap();
        }
        if super::has_column(&conn, "sync_runs", "ratings_applied").unwrap() {
            conn.execute("ALTER TABLE sync_runs DROP COLUMN ratings_applied", [])
                .unwrap();
        }
        conn.pragma_update(None, "user_version", 64).unwrap();
        conn.execute(
            "INSERT INTO device_settings (device_serial, device_name, ratings_back)
             VALUES ('phone', 'Train phone', 1)",
            [],
        )
        .unwrap();

        super::migrate_v65(&conn).unwrap();
        super::migrate_v65(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let settings_columns = conn
            .prepare("SELECT name FROM pragma_table_info('device_settings') ORDER BY cid")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let run_columns = conn
            .prepare("SELECT name FROM pragma_table_info('sync_runs') ORDER BY cid")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let device_name: String = conn
            .query_row(
                "SELECT device_name FROM device_settings WHERE device_serial = 'phone'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 65);
        assert!(!settings_columns.iter().any(|name| name == "ratings_back"));
        assert!(run_columns.iter().any(|name| name == "listens_applied"));
        assert!(run_columns.iter().any(|name| name == "ratings_applied"));
        assert_eq!(device_name, "Train phone");
    }

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
