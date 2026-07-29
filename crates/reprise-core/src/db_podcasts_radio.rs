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

const SCHEMA_V34: &str = r#"
CREATE TABLE IF NOT EXISTS podcast_episode_dismissals (
  subscription_id INTEGER NOT NULL
                  REFERENCES podcast_subscriptions(id) ON DELETE CASCADE,
  guid            TEXT NOT NULL,
  removed_at      INTEGER NOT NULL,
  PRIMARY KEY(subscription_id, guid)
);

DROP INDEX IF EXISTS idx_podcast_episodes_unplayed;
CREATE INDEX idx_podcast_episodes_unplayed
  ON podcast_episodes(played_at)
  WHERE played_at IS NULL AND removed_at IS NULL;
"#;

const SCHEMA_V41: &str = r#"
CREATE TABLE IF NOT EXISTS podcast_subscription_devices (
  subscription_id INTEGER NOT NULL
                  REFERENCES podcast_subscriptions(id) ON DELETE CASCADE,
  device_id       TEXT NOT NULL CHECK(length(trim(device_id)) > 0),
  PRIMARY KEY(subscription_id, device_id)
);
CREATE INDEX IF NOT EXISTS idx_podcast_subscription_devices_device
  ON podcast_subscription_devices(device_id);
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

pub(crate) fn migrate_v34(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_removed_at = {
        let mut statement = conn.prepare("PRAGMA table_info(podcast_episodes)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        columns.into_iter().any(|column| column == "removed_at")
    };
    let has_dismissals: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_schema
           WHERE type = 'table' AND name = 'podcast_episode_dismissals'
         )",
        [],
        |row| row.get(0),
    )?;
    if version >= 34 && has_removed_at && has_dismissals {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if !has_removed_at {
        transaction.execute(
            "ALTER TABLE podcast_episodes ADD COLUMN removed_at INTEGER",
            [],
        )?;
    }
    transaction.execute_batch(SCHEMA_V34)?;
    transaction.pragma_update(None, "user_version", version.max(34))?;
    transaction.commit()
}

pub(crate) fn migrate_v40(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 40 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if !has_column(&transaction, "podcast_subscriptions", "sync_to_phone")? {
        transaction.execute(
            "ALTER TABLE podcast_subscriptions
             ADD COLUMN sync_to_phone INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !has_column(&transaction, "podcast_episodes", "downloaded_bytes")? {
        transaction.execute(
            "ALTER TABLE podcast_episodes ADD COLUMN downloaded_bytes INTEGER",
            [],
        )?;
    }
    transaction.pragma_update(None, "user_version", 40)?;
    transaction.commit()
}

pub(crate) fn migrate_v41(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 41 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V41)?;
    transaction.pragma_update(None, "user_version", 41)?;
    transaction.commit()
}

// `MTP-40`: persistent "sync to phone" intent for a single episode,
// independent of whether it has been downloaded yet (design 7f). See
// `podcasts::wanted_on_device` for the pure transition this column backs.
pub(crate) fn migrate_v43(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_wanted = has_column(conn, "podcast_episodes", "wanted_on_device")?;
    if version >= 43 && has_wanted {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if !has_wanted {
        transaction.execute(
            "ALTER TABLE podcast_episodes
             ADD COLUMN wanted_on_device INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    transaction.pragma_update(None, "user_version", version.max(43))?;
    transaction.commit()
}

fn has_column(conn: &Connection, table: &str, expected: &str) -> Result<bool, rusqlite::Error> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|column| column == expected))
}

#[cfg(test)]
#[path = "db_podcasts_radio_migration_tests.rs"]
mod tests;
