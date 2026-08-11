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

const SCHEMA_V70: &str = r#"
ALTER TABLE new_releases
  ADD COLUMN hidden_by_deleted_memory INTEGER NOT NULL DEFAULT 0;
"#;

const INDEX_V70: &str = r#"
CREATE INDEX IF NOT EXISTS idx_new_releases_deleted_memory_hidden
ON new_releases(release_group_mbid)
WHERE hidden_by_deleted_memory = 1;
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

pub(crate) fn migrate_v70(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_memory_owner = column_exists(conn, "new_releases", "hidden_by_deleted_memory")?;
    let has_memory_index = index_exists(conn, "idx_new_releases_deleted_memory_hidden")?;
    if version >= 70 && has_memory_owner && has_memory_index {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    if !has_memory_owner {
        transaction.execute_batch(SCHEMA_V70)?;
    }
    transaction.execute_batch(INDEX_V70)?;
    transaction.pragma_update(None, "user_version", version.max(70))?;
    transaction.commit()
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for candidate in columns {
        if candidate? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn index_exists(conn: &Connection, index: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
         )",
        [index],
        |row| row.get(0),
    )
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
        conn.pragma_update(None, "user_version", 68).unwrap();
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

    #[test]
    fn migration_v70_records_memory_owned_hiding_without_claiming_existing_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "DROP INDEX idx_new_releases_deleted_memory_hidden;
             ALTER TABLE new_releases DROP COLUMN hidden_by_deleted_memory;
             PRAGMA user_version = 69;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, hidden
             ) VALUES ('manual', 'Artist', 'artist-id', 'Album', 'Album',
                       '2026-08-01', 1, 1)",
            [],
        )
        .unwrap();

        migrate_v70(&conn).unwrap();
        migrate_v70(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let memory_owned: bool = conn
            .query_row(
                "SELECT hidden_by_deleted_memory FROM new_releases
                 WHERE release_group_mbid = 'manual'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 70);
        assert!(!memory_owned);
        assert!(index_exists(&conn, "idx_new_releases_deleted_memory_hidden").unwrap());
    }
}
