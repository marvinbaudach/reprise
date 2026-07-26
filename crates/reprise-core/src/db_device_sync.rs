//! Schema migration for safe playlist mirroring to managed devices.

use rusqlite::Connection;

const ADD_MP3_QUALITY: &str = r#"
ALTER TABLE device_settings
  ADD COLUMN mp3_quality INTEGER NOT NULL DEFAULT 256
  CHECK (mp3_quality IN (128, 192, 256, 320));
"#;

const NORMALIZE_LEGACY_SELECTION: &str = r#"
UPDATE device_settings
SET selection_json = '[]'
WHERE selection_json = '"entire_library"';
"#;

const MIGRATE_DEVICE_FILES: &str = r#"
CREATE TABLE device_files_v34 (
  device_serial       TEXT NOT NULL,
  track_id            INTEGER NOT NULL,
  source_path         TEXT NOT NULL,
  source_size         INTEGER NOT NULL CHECK (source_size >= 0),
  source_mtime        INTEGER NOT NULL,
  device_path         TEXT NOT NULL,
  device_size         INTEGER NOT NULL CHECK (device_size >= 0),
  profile_fingerprint TEXT NOT NULL CHECK (profile_fingerprint <> ''),
  pinned              INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (device_serial, track_id)
);

INSERT INTO device_files_v34 (
  device_serial,
  track_id,
  source_path,
  source_size,
  source_mtime,
  device_path,
  device_size,
  profile_fingerprint,
  pinned
)
SELECT
  files.device_serial,
  files.track_id,
  COALESCE(tracks.path, ''),
  MAX(COALESCE(tracks.file_size, 0), 0),
  files.mtime,
  files.device_path,
  MAX(files.size, 0),
  'legacy-opus-v1',
  files.pinned
FROM device_files AS files
LEFT JOIN tracks ON tracks.id = files.track_id;

DROP TABLE device_files;
ALTER TABLE device_files_v34 RENAME TO device_files;
CREATE INDEX idx_device_files_serial ON device_files(device_serial);
"#;

const CREATE_DEVICE_PLAYLISTS: &str = r#"
CREATE TABLE IF NOT EXISTS device_playlists (
  device_serial TEXT NOT NULL,
  source_kind   TEXT NOT NULL CHECK (source_kind IN ('playlist', 'smart')),
  source_id     INTEGER NOT NULL CHECK (source_id > 0),
  source_name   TEXT NOT NULL,
  device_path   TEXT NOT NULL,
  PRIMARY KEY (device_serial, source_kind, source_id),
  UNIQUE (device_serial, device_path)
);
CREATE INDEX IF NOT EXISTS idx_device_playlists_serial ON device_playlists(device_serial);
"#;

pub(crate) fn migrate_v34(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 34 {
        return Ok(());
    }
    let has_mp3_quality = has_column(conn, "device_settings", "mp3_quality")?;
    let has_explicit_inventory = has_column(conn, "device_files", "profile_fingerprint")?;
    let transaction = conn.unchecked_transaction()?;
    if !has_mp3_quality {
        transaction.execute_batch(ADD_MP3_QUALITY)?;
    }
    transaction.execute_batch(NORMALIZE_LEGACY_SELECTION)?;
    if !has_explicit_inventory {
        transaction.execute_batch(MIGRATE_DEVICE_FILES)?;
    }
    transaction.execute_batch(CREATE_DEVICE_PLAYLISTS)?;
    transaction.pragma_update(None, "user_version", 34)?;
    transaction.commit()
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2
         )",
        [table, column],
        |row| row.get(0),
    )
}
