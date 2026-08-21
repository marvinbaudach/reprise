//! Schema migration that removes resume positions outside the supported scope.

use rusqlite::Connection;

pub(crate) fn migrate_v78(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 78 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE podcast_episodes
            SET position_ms = 0
          WHERE position_ms <> 0
            AND subscription_id IN (
                  SELECT id FROM podcast_subscriptions WHERE kind = 'youtube'
                )",
        [],
    )?;
    transaction.execute(
        "UPDATE podcast_episodes
            SET position_ms = 0
          WHERE position_ms <> 0
            AND duration_secs IS NOT NULL
            AND duration_secs < ?1",
        [crate::podcasts::resume_rules::MIN_RESUME_DURATION_SECS],
    )?;
    transaction.pragma_update(None, "user_version", 78)?;
    transaction.commit()
}

#[cfg(test)]
#[path = "db_podcast_resume_scope_migration_tests.rs"]
mod tests;
