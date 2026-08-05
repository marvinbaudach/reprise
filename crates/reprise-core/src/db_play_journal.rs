//! Schema storage for the Android play-count journal's applied high-water mark.
//!
//! Pending entries deliberately live outside SQLite so a scanner transaction
//! cannot prevent them from becoming durable. Only the one sequence already
//! applied belongs here: keeping it beside the play-count update lets Core
//! commit both facts atomically without exposing a SQL connection to Android.

use rusqlite::Connection;

const SCHEMA_V54: &str = r#"
CREATE TABLE IF NOT EXISTS android_play_count_journal_state (
  singleton        INTEGER PRIMARY KEY CHECK (singleton = 1),
  applied_sequence INTEGER NOT NULL CHECK (applied_sequence >= 0)
);
INSERT OR IGNORE INTO android_play_count_journal_state (singleton, applied_sequence)
VALUES (1, 0);
"#;

pub(crate) fn migrate_v54(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 54 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V54)?;
    transaction.pragma_update(None, "user_version", 54)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::migrate_v54;

    #[test]
    fn v53_upgrade_seeds_one_zero_high_water_and_is_idempotent() {
        for table_already_exists in [false, true] {
            let conn = crate::db::open(None).unwrap();
            crate::db::migrate_connection(&conn).unwrap();
            if !table_already_exists {
                conn.execute("DROP TABLE android_play_count_journal_state", [])
                    .unwrap();
            }
            conn.pragma_update(None, "user_version", 53).unwrap();

            migrate_v54(&conn).unwrap();
            migrate_v54(&conn).unwrap();

            let version: i64 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
            let states: Vec<i64> = conn
                .prepare("SELECT applied_sequence FROM android_play_count_journal_state")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(version, 54);
            assert_eq!(states, vec![0]);
        }
    }
}
