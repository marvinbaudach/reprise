//! Schema migration for official release track counts.

use rusqlite::Connection;

const SCHEMA_V36: &str = r#"
ALTER TABLE new_releases ADD COLUMN track_count INTEGER
  CHECK (track_count IS NULL OR track_count >= 2);
"#;

pub(crate) fn migrate_v36(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 36 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V36)?;
    transaction.pragma_update(None, "user_version", 36)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dg_2_v36_adds_constrained_release_track_counts() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "ALTER TABLE new_releases DROP COLUMN track_count;
             PRAGMA user_version = 35;",
        )
        .unwrap();

        migrate_v36(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 36);

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

        migrate_v36(&conn).unwrap();
    }
}
