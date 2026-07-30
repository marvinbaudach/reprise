//! Schema migration for official release track counts.

use rusqlite::Connection;

const SCHEMA_V39: &str = r#"
ALTER TABLE new_releases ADD COLUMN track_count INTEGER
  CHECK (track_count IS NULL OR track_count >= 2);
"#;

pub(crate) fn migrate_v39(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 39 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V39)?;
    transaction.pragma_update(None, "user_version", 39)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dg_2_v39_adds_constrained_release_track_counts_after_device_sync_schema() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "ALTER TABLE new_releases DROP COLUMN track_count;
             PRAGMA user_version = 38;",
        )
        .unwrap();

        migrate_v39(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 39);

        let columns = conn
            .prepare("PRAGMA table_info(new_releases)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "track_count"));

        let invalid = conn.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent, track_count
             ) VALUES ('single-sized', 'Artist', 'artist', 'Album', 'Album',
                       '2020-01-01', 1, '#123456', 1)",
            [],
        );
        assert!(
            invalid.is_err(),
            "one-track release variants cannot prove album or EP ownership"
        );

        let device_columns = conn
            .prepare("PRAGMA table_info(device_playlists)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            device_columns
                .iter()
                .any(|column| column == "last_synced_at"),
            "the v39 migration must preserve the Android sync v38 schema"
        );

        migrate_v39(&conn).unwrap();
    }
}
