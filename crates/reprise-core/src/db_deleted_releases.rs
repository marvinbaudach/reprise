//! Schema migration for durable deliberate-release deletion memory.

use rusqlite::Connection;

const SCHEMA_V69: &str = r#"
CREATE TABLE IF NOT EXISTS deleted_releases (
  artist_key TEXT NOT NULL,
  title_key  TEXT NOT NULL,
  scope      TEXT NOT NULL,
  deleted_at INTEGER NOT NULL,
  PRIMARY KEY (artist_key, title_key, scope)
);
"#;

pub(crate) fn migrate_v69(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 69 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V69)?;
    transaction.pragma_update(None, "user_version", 69)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_v69_creates_an_empty_deleted_releases_table_idempotently() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "DROP TABLE deleted_releases;
             PRAGMA user_version = 68;",
        )
        .unwrap();

        migrate_v69(&conn).unwrap();
        migrate_v69(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(deleted_releases)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM deleted_releases", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(version, 69);
        assert_eq!(columns, ["artist_key", "title_key", "scope", "deleted_at"]);
        assert_eq!(rows, 0);
    }
}
