//! Schema migration for cached concerts and their per-artist fetch ledger.

use rusqlite::Connection;

const SCHEMA_V31: &str = r#"
CREATE TABLE IF NOT EXISTS concert_artists (
  artist_key      TEXT PRIMARY KEY,
  artist_name     TEXT NOT NULL,
  artist_mbid     TEXT,
  provider        TEXT,
  provider_id     TEXT,
  mbid_verified   INTEGER NOT NULL DEFAULT 0,
  is_similar      INTEGER NOT NULL DEFAULT 0,
  similar_to      TEXT,
  last_attempt_at INTEGER,
  last_outcome    TEXT,
  events_found    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS concert_events (
  id            INTEGER PRIMARY KEY,
  artist_key    TEXT NOT NULL,
  artist_name   TEXT NOT NULL,
  starts_at     TEXT NOT NULL,
  date_key      TEXT NOT NULL,
  venue         TEXT NOT NULL,
  city          TEXT NOT NULL,
  region        TEXT,
  country       TEXT,
  latitude      REAL,
  longitude     REAL,
  ticket_url    TEXT,
  ticket_source TEXT,
  event_url     TEXT,
  provider      TEXT NOT NULL,
  is_similar    INTEGER NOT NULL DEFAULT 0,
  similar_to    TEXT,
  fetched_at    INTEGER NOT NULL,
  seen_at       INTEGER,
  dedupe_key    TEXT NOT NULL UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_concert_events_date
  ON concert_events(date_key);
CREATE INDEX IF NOT EXISTS idx_concert_events_artist
  ON concert_events(artist_key);
"#;

pub(crate) fn migrate_v31(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 31 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V31)?;
    transaction.pragma_update(None, "user_version", 31)?;
    transaction.commit()
}

#[cfg(test)]
#[path = "db_concerts_migration_tests.rs"]
mod tests;
