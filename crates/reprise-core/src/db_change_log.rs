//! Schema migration for cross-process change notifications.

use rusqlite::Connection;

const SCHEMA_V26: &str = r#"
CREATE TABLE change_log (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  entity    TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  op        TEXT NOT NULL,
  writer    INTEGER NOT NULL,
  at        INTEGER NOT NULL
);
CREATE INDEX idx_change_log_at ON change_log(at);
"#;

pub(crate) fn migrate_v26(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 26 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V26)?;
    transaction.pragma_update(None, "user_version", 26)?;
    transaction.commit()
}
