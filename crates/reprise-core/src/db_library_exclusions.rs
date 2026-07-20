//! Schema migration for persistent, identity-based library exclusions.

use rusqlite::Connection;

const SCHEMA_V24: &str = r#"
CREATE TABLE library_exclusions (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL,
  device      INTEGER,
  inode       INTEGER,
  file_size   INTEGER NOT NULL DEFAULT 0,
  file_mtime  INTEGER NOT NULL DEFAULT 0,
  excluded_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_library_exclusions_path
  ON library_exclusions(path)
  WHERE device IS NULL OR inode IS NULL;
CREATE UNIQUE INDEX idx_library_exclusions_identity
  ON library_exclusions(device, inode)
  WHERE device IS NOT NULL AND inode IS NOT NULL;
"#;

pub(crate) fn migrate_v24(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 24 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V24)?;
    transaction.pragma_update(None, "user_version", 24)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    #[test]
    fn browse_7_v23_upgrade_adds_idempotent_exclusion_schema() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "DROP TABLE library_exclusions;
             PRAGMA user_version = 23;",
        )
        .unwrap();

        super::migrate_v24(&conn).unwrap();
        super::migrate_v24(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 24);
        let table: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type='table' AND name='library_exclusions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table, 1);
    }
}
