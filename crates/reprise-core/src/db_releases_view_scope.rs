//! Schema v62: invalidates the per-artist Releases fetch cache after singles
//! become durable catalog data and removes their retired preference.

use rusqlite::Connection;

const SCHEMA_V62: &str = r#"
UPDATE artist_news_fetch SET last_attempt_at = 0;
DELETE FROM settings WHERE key = 'module.new_releases.include_singles';
"#;

/// The v30 ledger deliberately backfilled past attempts to avoid fetching an
/// entire library after that migration. This reset is different: the cached
/// answer for every artist is now incomplete because released singles used to
/// be discarded. Preserving each `artist_mbid` avoids repeating artist-search
/// requests, while the existing thirty-artist rotation keeps the refetch load
/// bounded.
pub(crate) fn migrate_v62(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 62 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V62)?;
    transaction.pragma_update(None, "user_version", 62)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v62_resets_release_fetches_preserves_identity_and_drops_the_dead_setting() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO artist_news_fetch (
               artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found
             ) VALUES ('artist', 'artist-mbid', 1234, 'ok', 7)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value)
             VALUES ('module.new_releases.include_singles', 'true')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 61).unwrap();

        migrate_v62(&conn).unwrap();
        migrate_v62(&conn).unwrap();

        let (artist_mbid, last_attempt_at): (Option<String>, i64) = conn
            .query_row(
                "SELECT artist_mbid, last_attempt_at
                 FROM artist_news_fetch WHERE artist_key = 'artist'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let setting_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings
                 WHERE key = 'module.new_releases.include_singles'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();

        assert_eq!(artist_mbid.as_deref(), Some("artist-mbid"));
        assert_eq!(last_attempt_at, 0);
        assert_eq!(setting_count, 0);
        assert_eq!(version, 62);
    }
}
