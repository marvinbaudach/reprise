//! Schema migration that clears video thumbnails out of YouTube channel images.
//!
//! Until 2026-08-18 a YouTube subscription could only ever hold an *episode*
//! picture in `image_url`: the add dialog persisted the preview thumbnail of a
//! search hit (search hits are videos), and the refresh projection hard-coded
//! `None`, so nothing ever replaced it. Measured on the live database that day:
//! seven of nine channels carried an `i.ytimg.com/vi/…` URL, two carried NULL.
//!
//! With the projection fixed, a refresh now writes the real avatar — but only
//! when the subscription is due, which is `DEFAULT_REFRESH_HOURS` (6) plus up
//! to an hour of jitter away. This migration drops the wrong value and clears
//! `last_fetch_at`, which makes those subscriptions due immediately (`refresh`
//! treats a missing timestamp as due), so the next refresh pass repairs them
//! instead of leaving a stale picture on screen for most of a day.

use rusqlite::Connection;

pub(crate) fn migrate_v77(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 77 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE podcast_subscriptions
            SET image_url = NULL,
                last_fetch_at = NULL
          WHERE kind = 'youtube'
            AND (image_url IS NULL OR image_url LIKE '%i.ytimg.com/vi/%')",
        [],
    )?;
    transaction.pragma_update(None, "user_version", 77)?;
    transaction.commit()
}

#[cfg(test)]
#[path = "db_podcast_channel_image_migration_tests.rs"]
mod tests;
