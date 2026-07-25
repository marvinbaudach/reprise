//! Schema migration adding the per-artist New Releases fetch ledger.
//!
//! Before this table, freshness was judged by `MAX(fetched_at)` over
//! `new_releases` rows — which only exist for artists that actually had
//! news. An artist with nothing to report never got a cache entry and was
//! therefore re-fetched on every single run. The ledger records the
//! *attempt*, not the outcome, so "checked, found nothing" is finally
//! distinguishable from "never checked".
//!
//! The key is the normalized artist name rather than the MBID: artists
//! without a resolved MBID are exactly the ones that need tracking, and the
//! name is what `artists_for_fetch` already groups by.
//!
//! Backfill seeds the ledger from existing `new_releases` rows so an upgrade
//! does not re-fetch the whole library at once. Artists with no rows there
//! deliberately get no entry — they count as "never checked" and are picked
//! up first by the rotation, which is the intended behaviour.

use rusqlite::Connection;

const SCHEMA_V30: &str = r#"
CREATE TABLE IF NOT EXISTS artist_news_fetch (
  artist_key      TEXT PRIMARY KEY,
  artist_mbid     TEXT,
  last_attempt_at INTEGER NOT NULL,
  last_outcome    TEXT NOT NULL,
  releases_found  INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO artist_news_fetch
  (artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found)
SELECT lower(trim(artist_name)), MAX(artist_mbid), MAX(fetched_at), 'ok', COUNT(*)
FROM new_releases
GROUP BY lower(trim(artist_name));
"#;

pub(crate) fn migrate_v30(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 30 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V30)?;
    transaction.pragma_update(None, "user_version", 30)?;
    transaction.commit()
}

#[cfg(test)]
#[path = "db_artist_news_fetch_migration_tests.rs"]
mod tests;
