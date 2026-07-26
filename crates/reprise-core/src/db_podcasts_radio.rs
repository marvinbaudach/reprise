//! Schema migration for podcast subscriptions, episodes, and radio favorites.

use rusqlite::Connection;

const SCHEMA_V32: &str = r#"
CREATE TABLE IF NOT EXISTS podcast_subscriptions (
  id              INTEGER PRIMARY KEY,
  kind            TEXT NOT NULL,
  feed_url        TEXT NOT NULL UNIQUE,
  title           TEXT NOT NULL,
  author          TEXT,
  image_url       TEXT,
  etag            TEXT,
  last_modified   TEXT,
  last_fetch_at   INTEGER,
  last_outcome    TEXT,
  auto_download   INTEGER NOT NULL DEFAULT 0,
  added_at        INTEGER NOT NULL,
  removed_at      INTEGER
);

CREATE TABLE IF NOT EXISTS podcast_episodes (
  id              INTEGER PRIMARY KEY,
  subscription_id INTEGER NOT NULL
                    REFERENCES podcast_subscriptions(id) ON DELETE CASCADE,
  guid            TEXT NOT NULL,
  title           TEXT NOT NULL,
  audio_url       TEXT NOT NULL,
  page_url        TEXT,
  published_at    INTEGER,
  duration_secs   INTEGER,
  downloaded_path TEXT,
  played_at       INTEGER,
  position_ms     INTEGER NOT NULL DEFAULT 0,
  first_seen_at   INTEGER NOT NULL,
  UNIQUE(subscription_id, guid)
);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_sub
  ON podcast_episodes(subscription_id);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_pub
  ON podcast_episodes(published_at);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_unplayed
  ON podcast_episodes(played_at) WHERE played_at IS NULL;

CREATE TABLE IF NOT EXISTS radio_stations (
  id              INTEGER PRIMARY KEY,
  uuid            TEXT UNIQUE,
  name            TEXT NOT NULL,
  stream_url      TEXT NOT NULL UNIQUE,
  homepage        TEXT,
  favicon_url     TEXT,
  genre           TEXT,
  codec           TEXT,
  bitrate_kbps    INTEGER,
  country_code    TEXT,
  votes           INTEGER,
  added_at        INTEGER NOT NULL,
  removed_at      INTEGER
);
"#;

const SCHEMA_V33: &str = r#"
CREATE TABLE IF NOT EXISTS podcast_subscription_baselines (
  subscription_id INTEGER NOT NULL
                  REFERENCES podcast_subscriptions(id) ON DELETE CASCADE,
  guid            TEXT NOT NULL,
  PRIMARY KEY(subscription_id, guid)
);
"#;

pub(crate) fn migrate_v32(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 32 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V32)?;
    transaction.pragma_update(None, "user_version", 32)?;
    transaction.commit()
}

pub(crate) fn migrate_v33(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 33 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V33)?;
    transaction.pragma_update(None, "user_version", 33)?;
    transaction.commit()
}

#[cfg(test)]
#[path = "db_podcasts_radio_migration_tests.rs"]
mod tests;
