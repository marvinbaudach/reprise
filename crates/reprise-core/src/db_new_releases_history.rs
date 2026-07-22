//! Schema migration adding release-history/retention columns to
//! `new_releases`.
//!
//! `first_seen` backfills from the existing `fetched_at` clock so retention
//! math (a later task) has a stable "when did we first learn about this
//! release" timestamp that does not move if the release is re-fetched.
//! `hidden_at` backfills to "now" for rows already hidden, so hidden releases
//! get a real hide timestamp instead of silently looking freshly hidden.
//! `announce_url` is new state with no prior data to backfill, so it stays
//! `NULL` for every existing row.

use rusqlite::Connection;

const SCHEMA_V26: &str = r#"
ALTER TABLE new_releases ADD COLUMN first_seen INTEGER;
ALTER TABLE new_releases ADD COLUMN hidden_at INTEGER;
ALTER TABLE new_releases ADD COLUMN announce_url TEXT;
UPDATE new_releases SET first_seen = fetched_at WHERE first_seen IS NULL;
UPDATE new_releases SET hidden_at = strftime('%s', 'now')
  WHERE hidden = 1 AND hidden_at IS NULL;
"#;

pub(crate) fn migrate_v26(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 26 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V26)?;
    transaction.pragma_update(None, "user_version", 26)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRE_V26_NEW_RELEASES: &str = r#"
CREATE TABLE new_releases (
  release_group_mbid TEXT PRIMARY KEY,
  artist_name        TEXT NOT NULL,
  artist_mbid        TEXT NOT NULL,
  title              TEXT NOT NULL,
  release_type       TEXT NOT NULL,
  first_release_date TEXT NOT NULL,
  fetched_at         INTEGER NOT NULL,
  seen_at            INTEGER,
  hidden             INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
  fallback_accent    TEXT NOT NULL
);
PRAGMA user_version = 25;
"#;

    #[test]
    fn migrate_v25_to_v26_adds_history_columns_and_backfills_them() {
        let conn = crate::db::open(None).unwrap();
        conn.execute_batch(PRE_V26_NEW_RELEASES).unwrap();
        conn.execute(
            "INSERT INTO new_releases
               (release_group_mbid, artist_name, artist_mbid, title, release_type,
                first_release_date, fetched_at, seen_at, hidden, fallback_accent)
             VALUES ('rg-hidden', 'Artist', 'mbid-1', 'Title', 'album',
                     '2024-01-01', 1000, NULL, 1, '#000000')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO new_releases
               (release_group_mbid, artist_name, artist_mbid, title, release_type,
                first_release_date, fetched_at, seen_at, hidden, fallback_accent)
             VALUES ('rg-visible', 'Artist', 'mbid-2', 'Title 2', 'album',
                     '2024-02-01', 2000, NULL, 0, '#111111')",
            [],
        )
        .unwrap();

        migrate_v26(&conn).unwrap();
        migrate_v26(&conn).unwrap(); // idempotent re-run must not fail

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 26);

        let columns = conn
            .prepare("PRAGMA table_info(new_releases)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for column in ["first_seen", "hidden_at", "announce_url"] {
            assert!(
                columns.iter().any(|c| c == column),
                "missing column {column}"
            );
        }

        let (first_seen, hidden_at, announce_url): (i64, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT first_seen, hidden_at, announce_url FROM new_releases \
                 WHERE release_group_mbid = 'rg-hidden'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(first_seen, 1000);
        assert!(hidden_at.is_some());
        assert!(announce_url.is_none());

        let (first_seen, hidden_at, announce_url): (i64, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT first_seen, hidden_at, announce_url FROM new_releases \
                 WHERE release_group_mbid = 'rg-visible'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(first_seen, 2000);
        assert!(hidden_at.is_none());
        assert!(announce_url.is_none());
    }
}
