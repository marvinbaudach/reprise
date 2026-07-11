use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn open(path: Option<&Path>) -> Result<Connection, DbError> {
    let conn = match path {
        Some(p) => {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            Connection::open(p)?
        }
        None => Connection::open_in_memory()?,
    };
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Cheap insurance for future concurrent writers (e.g. a scan worker
    // thread's own `Connection` writing while the UI thread reads): wait up
    // to 5s for a lock instead of failing immediately with `SQLITE_BUSY`.
    conn.pragma_update(None, "busy_timeout", 5000)?;
    Ok(conn)
}

const SCHEMA_V1: &str = r#"
CREATE TABLE tracks (
  id            INTEGER PRIMARY KEY,
  path          TEXT NOT NULL UNIQUE,
  title         TEXT NOT NULL DEFAULT '',
  artist        TEXT NOT NULL DEFAULT '',
  album         TEXT NOT NULL DEFAULT '',
  album_artist  TEXT NOT NULL DEFAULT '',
  year          INTEGER,
  track_no      INTEGER,
  genre         TEXT NOT NULL DEFAULT '',
  duration_ms   INTEGER NOT NULL DEFAULT 0,
  bitrate_kbps  INTEGER,
  rating        INTEGER NOT NULL DEFAULT 0,
  play_count    INTEGER NOT NULL DEFAULT 0,
  last_played_at INTEGER,
  added_at      INTEGER NOT NULL,
  file_mtime    INTEGER NOT NULL DEFAULT 0,
  missing       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tracks_artist ON tracks(artist);
CREATE INDEX idx_tracks_album  ON tracks(album);
CREATE TABLE import_errors (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL,
  reason      TEXT NOT NULL,
  occurred_at INTEGER NOT NULL
);
"#;

/// Schema v2 (Stage 2 Task 8 — scanner move detection): adds the filesystem
/// identity columns the scanner uses to recognize a relocated file. `dev`+
/// `inode` survive a same-filesystem `rename`(2); `file_size` is the
/// fallback fingerprint signal when the inode changes too (cross-filesystem
/// copy+delete). Nullable (`device`/`inode`) because pre-v2 rows have none
/// until their next scan; `file_size` is `NOT NULL DEFAULT 0` to match the
/// rest of the tag-derived columns' non-null convention.
const SCHEMA_V2: &str = r#"
ALTER TABLE tracks ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN device INTEGER;
ALTER TABLE tracks ADD COLUMN inode INTEGER;
CREATE INDEX idx_tracks_dev_inode ON tracks(device, inode);
"#;

/// Applies pending schema migrations in order, tracked via `PRAGMA
/// user_version`. Design choice: rather than branching "fresh DB gets the
/// latest schema in one shot, existing DB gets incremental ALTERs", every DB
/// — fresh or existing — walks the *same* sequence of version steps
/// (`SCHEMA_V1` then `SCHEMA_V2`'s `ALTER`s). This keeps there being exactly
/// one code path per version bump to test and reason about, at the cost of a
/// fresh install running through slightly more SQL than strictly necessary —
/// a one-time, sub-millisecond cost that's worth the simplicity.
pub fn migrate(conn: &Connection) -> Result<(), DbError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_tracks_table_and_is_idempotent() {
        let conn = open(None).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // second run must not fail
        let n: i64 = conn
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    /// Builds a v1 DB by hand (SCHEMA_V1 + `user_version = 1`, exactly what a
    /// pre-Task-8 install looks like on disk), inserts a row the way the old
    /// scanner would have, then migrates. The v2 columns must exist with
    /// their documented defaults, the pre-existing row's data must survive
    /// untouched, and a second `migrate` call must be a no-op (idempotent).
    #[test]
    fn migrate_v1_to_v2_adds_columns_preserves_data_and_is_idempotent() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, rating, play_count, added_at) \
             VALUES ('/x/a.flac', 'A Title', 'An Artist', 5, 7, 1000)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);

        let (title, rating, play_count, added_at, file_size, device, inode): (
            String,
            i64,
            i64,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
        ) = conn
            .query_row(
                "SELECT title, rating, play_count, added_at, file_size, device, inode \
                 FROM tracks WHERE path = '/x/a.flac'",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(title, "A Title");
        assert_eq!(rating, 5);
        assert_eq!(play_count, 7);
        assert_eq!(added_at, 1000);
        assert_eq!(file_size, 0); // NOT NULL DEFAULT 0 for a pre-v2 row
        assert_eq!(device, None);
        assert_eq!(inode, None);

        // Second migrate() call (e.g. next app launch) must not fail or
        // re-run the ALTERs (which would error: "duplicate column name").
        migrate(&conn).unwrap();
        let version_after_second_run: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version_after_second_run, 2);
    }

    #[test]
    fn open_sets_busy_timeout() {
        let conn = open(None).unwrap();
        let busy_timeout_ms: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy_timeout_ms, 5000);
    }
}
