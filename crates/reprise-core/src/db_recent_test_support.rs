//! Test support for rewinding fully migrated fixtures to an older schema.

use rusqlite::Connection;

pub(crate) fn remove_v73_v74_columns(conn: &Connection) {
    conn.execute_batch(
        "ALTER TABLE concert_events DROP COLUMN ticket_availability;
         ALTER TABLE new_releases DROP COLUMN notified_released_at;",
    )
    .unwrap();
}
