//! Schema migration for cached concerts and their per-artist fetch ledger.

use std::collections::HashMap;

use rusqlite::{params, types::Type, Connection};

use crate::concerts::{dedupe_key, ProviderKind};

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

pub(crate) fn migrate_v73(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 73 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "ALTER TABLE concert_events
         ADD COLUMN ticket_availability TEXT NOT NULL DEFAULT 'unknown';",
    )?;
    transaction.pragma_update(None, "user_version", 73)?;
    transaction.commit()
}

const SCHEMA_V75: &str = r#"
DELETE FROM settings WHERE key = 'ui.column_layout.concerts';
"#;

pub(crate) fn migrate_v75(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 75 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V75)?;
    transaction.pragma_update(None, "user_version", 75)?;
    transaction.commit()
}

#[derive(Debug)]
struct StoredListing {
    id: i64,
    dedupe_key: String,
    provider: ProviderKind,
    ticket_url: Option<String>,
}

pub(crate) fn migrate_v76(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 76 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    let listings = {
        let mut statement = transaction.prepare(
            "SELECT id, artist_key, date_key, city, provider, ticket_url
               FROM concert_events
              ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            let provider: String = row.get(4)?;
            Ok(StoredListing {
                id: row.get(0)?,
                dedupe_key: dedupe_key(
                    row.get_ref(1)?.as_str()?,
                    row.get_ref(2)?.as_str()?,
                    row.get_ref(3)?.as_str()?,
                ),
                provider: parse_provider(4, &provider)?,
                ticket_url: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut winners: Vec<StoredListing> = Vec::with_capacity(listings.len());
    let mut positions: HashMap<String, usize> = HashMap::with_capacity(listings.len());
    let mut loser_ids = Vec::new();
    for listing in listings {
        if let Some(&position) = positions.get(&listing.dedupe_key) {
            let existing = &winners[position];
            if ProviderKind::listing_winner_is_incoming(
                existing.provider,
                existing.ticket_url.as_deref(),
                listing.provider,
                listing.ticket_url.as_deref(),
            ) {
                loser_ids.push(existing.id);
                winners[position] = listing;
            } else {
                loser_ids.push(listing.id);
            }
        } else {
            positions.insert(listing.dedupe_key.clone(), winners.len());
            winners.push(listing);
        }
    }

    for id in loser_ids {
        transaction.execute("DELETE FROM concert_events WHERE id = ?1", [id])?;
    }
    for listing in &winners {
        transaction.execute(
            "UPDATE concert_events SET dedupe_key = ?1 WHERE id = ?2",
            params![format!("\0reprise-v76-{}", listing.id), listing.id],
        )?;
    }
    for listing in winners {
        transaction.execute(
            "UPDATE concert_events SET dedupe_key = ?1 WHERE id = ?2",
            params![listing.dedupe_key, listing.id],
        )?;
    }
    transaction.pragma_update(None, "user_version", 76)?;
    transaction.commit()
}

fn parse_provider(column: usize, value: &str) -> Result<ProviderKind, rusqlite::Error> {
    match value {
        "bandsintown" => Ok(ProviderKind::Bandsintown),
        "ticketmaster" => Ok(ProviderKind::Ticketmaster),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            Type::Text,
            format!("unknown concert provider: {value}").into(),
        )),
    }
}

#[cfg(test)]
#[path = "db_concerts_migration_tests.rs"]
mod tests;
