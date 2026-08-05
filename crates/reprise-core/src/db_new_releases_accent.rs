//! Schema v54: drops `new_releases.fallback_accent`.
//!
//! The column was a per-release tint extracted from an artist's most-played
//! album cover, used to colour a release tile before its own cover finished
//! loading. Deriving the accent from cover art is gone; placeholder tiles now
//! take the effective accent from the app's own accent source at render time
//! and never read a stored colour. What was left behind wrote the constant
//! `#3584E4` into every fetched row and published it through the MCP catalog
//! as if it still carried meaning, so the column is removed rather than
//! quietly kept.
//!
//! The v12 `CREATE TABLE new_releases` step keeps the column, as does the v26
//! test fixture that reconstructs the pre-v26 table: shipped migrations are
//! immutable history and every database walks the same version sequence (see
//! `SCHEMA_V11`'s note on why a column drop gets its own version instead of
//! being retrofitted into the step that added it). This later step reclaims
//! the column on every database, fresh or existing.
//!
//! `ALTER TABLE ... DROP COLUMN` is the same tool v11 used to retire
//! `tracks.missing`, and it is safe here for the reason SQLite requires: no
//! index, partial-index predicate, view, or trigger mentions
//! `fallback_accent`, and the drop rewrites nothing else — the other
//! `new_releases` columns and all their rows survive untouched, which a
//! table rebuild would have put at risk for no gain.
//!
//! The `PRAGMA table_info` guard makes the step tolerate a table that already
//! lacks the column, so re-running it (or running it against a database
//! stamped back to an earlier version by a test) is a no-op rather than a
//! hard "no such column" failure.

use rusqlite::Connection;

pub(crate) fn migrate_v54(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 54 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if column_exists(&transaction, "new_releases", "fallback_accent")? {
        transaction.execute_batch("ALTER TABLE new_releases DROP COLUMN fallback_accent;")?;
    }
    transaction.pragma_update(None, "user_version", 54)?;
    transaction.commit()
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    columns.try_fold(false, |found, name| Ok(found || name? == column))
}

#[cfg(test)]
mod tests {
    use rusqlite::types::Value;

    use super::*;

    /// The `new_releases` shape as of v53, rebuilt by hand so the test can
    /// prove an *existing* user database — one that still carries the column
    /// and real rows in it — survives the drop with every other value intact.
    const V53_NEW_RELEASES: &str = r#"
DROP TABLE new_releases;
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
  fallback_accent    TEXT NOT NULL,
  first_seen         INTEGER,
  hidden_at          INTEGER,
  announce_url       TEXT,
  track_count        INTEGER CHECK (track_count IS NULL OR track_count >= 2)
);
CREATE INDEX idx_new_releases_artist ON new_releases(artist_mbid);
CREATE INDEX idx_new_releases_unseen ON new_releases(seen_at) WHERE seen_at IS NULL;
PRAGMA user_version = 53;
"#;

    fn columns(conn: &Connection) -> Vec<String> {
        conn.prepare("PRAGMA table_info(new_releases)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn v53_database_drops_the_accent_and_keeps_every_other_release_value() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(V53_NEW_RELEASES).unwrap();
        conn.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, seen_at, hidden, fallback_accent,
               first_seen, hidden_at, announce_url, track_count
             ) VALUES ('rg-1', 'Artist', 'artist-mbid', 'Title', 'Album',
                       '2026-01-01', 1000, 1200, 1, '#3584E4', 900, 1300,
                       'https://example.test/announce', 9)",
            [],
        )
        .unwrap();

        migrate_v54(&conn).unwrap();
        migrate_v54(&conn).unwrap(); // idempotent re-run must not fail

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 54);
        assert_eq!(
            columns(&conn),
            [
                "release_group_mbid",
                "artist_name",
                "artist_mbid",
                "title",
                "release_type",
                "first_release_date",
                "fetched_at",
                "seen_at",
                "hidden",
                "first_seen",
                "hidden_at",
                "announce_url",
                "track_count",
            ]
        );

        // `SELECT *` rather than a named column list: the point is that no
        // value shifted into a neighbour's slot, so the test must read the
        // table's own post-drop column order.
        let row = conn
            .query_row("SELECT * FROM new_releases", [], |row| {
                (0..13)
                    .map(|index| row.get::<_, rusqlite::types::Value>(index))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        assert_eq!(
            row,
            [
                Value::Text("rg-1".to_string()),
                Value::Text("Artist".to_string()),
                Value::Text("artist-mbid".to_string()),
                Value::Text("Title".to_string()),
                Value::Text("Album".to_string()),
                Value::Text("2026-01-01".to_string()),
                Value::Integer(1000),
                Value::Integer(1200),
                Value::Integer(1),
                Value::Integer(900),
                Value::Integer(1300),
                Value::Text("https://example.test/announce".to_string()),
                Value::Integer(9),
            ]
        );

        // The indexes the table carried before the drop must still be there:
        // SQLite recreates them as part of `DROP COLUMN`, and a lost partial
        // index would silently turn the unseen-badge query into a scan.
        let indexes = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'new_releases' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(indexes.iter().any(|name| name == "idx_new_releases_artist"));
        assert!(indexes.iter().any(|name| name == "idx_new_releases_unseen"));
    }

    #[test]
    fn fresh_database_has_no_accent_column() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
        assert!(!columns(&conn).iter().any(|c| c == "fallback_accent"));
    }
}
