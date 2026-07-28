//! Schema for the device-sync log (MTP-20).

use rusqlite::Connection;

const CREATE_SYNC_LOG: &str = "\
CREATE TABLE IF NOT EXISTS sync_runs (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  device_serial     TEXT NOT NULL,
  device_name       TEXT NOT NULL,
  transfer_profile  TEXT NOT NULL,
  started_at        INTEGER NOT NULL,
  finished_at       INTEGER,
  outcome           TEXT NOT NULL
    CHECK (outcome IN ('running','completed','cancelled','failed','interrupted')),
  planned           INTEGER NOT NULL DEFAULT 0 CHECK (planned >= 0),
  copied            INTEGER NOT NULL DEFAULT 0 CHECK (copied >= 0),
  skipped           INTEGER NOT NULL DEFAULT 0 CHECK (skipped >= 0),
  deleted           INTEGER NOT NULL DEFAULT 0 CHECK (deleted >= 0),
  failed            INTEGER NOT NULL DEFAULT 0 CHECK (failed >= 0),
  bytes_copied      INTEGER NOT NULL DEFAULT 0 CHECK (bytes_copied >= 0),
  detail            TEXT
);
CREATE INDEX IF NOT EXISTS idx_sync_runs_started ON sync_runs(started_at DESC, id DESC);
CREATE TABLE IF NOT EXISTS sync_events (
  run_id       INTEGER NOT NULL,
  kind         TEXT NOT NULL
    CHECK (kind IN ('skipped','failed','deleted','conversion_fallback','playlist_write_failed')),
  track_id     INTEGER,
  device_path  TEXT NOT NULL,
  detail       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sync_events_run ON sync_events(run_id);";

pub(crate) fn migrate_v40(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 40 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(CREATE_SYNC_LOG)?;
    transaction.pragma_update(None, "user_version", 40)?;
    transaction.commit()
}
