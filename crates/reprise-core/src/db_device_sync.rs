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
CREATE TABLE device_files_v36 (
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

INSERT INTO device_files_v36 (
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
ALTER TABLE device_files_v36 RENAME TO device_files;
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

const ADD_TRANSFER_PROFILE: &str = r#"
ALTER TABLE device_settings
  ADD COLUMN transfer_profile TEXT NOT NULL DEFAULT 'opus_160'
  CHECK (transfer_profile IN ('opus_160', 'mp3_256', 'original'));
"#;

const PRESERVE_EXISTING_MP3_BEHAVIOR: &str =
    "UPDATE device_settings SET transfer_profile = 'mp3_256';";

const ADD_PLAYLIST_LAST_SYNC: &str = r#"
ALTER TABLE device_playlists
  ADD COLUMN last_synced_at INTEGER
  CHECK (last_synced_at IS NULL OR last_synced_at >= 0);
"#;

// `MTP-18`: three named, per-device sync targets replacing the single
// implicit managed folder from `78e379fd`. See
// `device_sync::targets` for the pure model this table backs — `kind` is
// one of `SyncTargetKind::storage_value()`, `storage_id` is an MTP
// `StorageID` (not a path component, and never a persisted object handle —
// handles are not stable across reconnects), `path` is the device-relative
// target folder, and `cap_bytes` is an optional per-target size cap.
const CREATE_DEVICE_SYNC_TARGETS: &str = r#"
CREATE TABLE IF NOT EXISTS device_sync_targets (
  device_serial TEXT NOT NULL,
  kind          TEXT NOT NULL
                  CHECK (kind IN ('playlists', 'youtube_audio', 'podcast_episodes')),
  storage_id    INTEGER,
  path          TEXT NOT NULL CHECK (length(trim(path)) > 0),
  enabled       INTEGER NOT NULL DEFAULT 1,
  cap_bytes     INTEGER CHECK (cap_bytes IS NULL OR cap_bytes >= 0),
  PRIMARY KEY (device_serial, kind)
);
"#;

pub(crate) fn migrate_v36(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 36 {
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
    transaction.pragma_update(None, "user_version", 36)?;
    transaction.commit()
}

pub(crate) fn migrate_v37(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 37 {
        return Ok(());
    }
    let has_transfer_profile = has_column(conn, "device_settings", "transfer_profile")?;
    let transaction = conn.unchecked_transaction()?;
    if !has_transfer_profile {
        transaction.execute_batch(ADD_TRANSFER_PROFILE)?;
        transaction.execute_batch(PRESERVE_EXISTING_MP3_BEHAVIOR)?;
    }
    transaction.pragma_update(None, "user_version", 37)?;
    transaction.commit()
}

pub(crate) fn migrate_v38(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_last_sync = has_column(conn, "device_playlists", "last_synced_at")?;
    if version >= 38 && has_last_sync {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if !has_last_sync {
        transaction.execute_batch(ADD_PLAYLIST_LAST_SYNC)?;
    }
    transaction.pragma_update(None, "user_version", version.max(38))?;
    transaction.commit()
}

pub(crate) fn migrate_v42(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 42 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(CREATE_DEVICE_SYNC_TARGETS)?;
    transaction.pragma_update(None, "user_version", 42)?;
    transaction.commit()
}

// Design 7a/7e (`docs/plans/podcasts-youtube-radio-turn6.md` §3b): the
// device view's "Sync automatically when this phone connects" switch. Like
// `remove_deleted`, this is a genuinely per-device choice (not one of the
// global sync rules 7b's Preferences block owns), so it lives beside it on
// `device_settings` rather than on `device_sync_targets`.
const ADD_SYNC_AUTOMATICALLY: &str = r#"
ALTER TABLE device_settings
  ADD COLUMN sync_automatically INTEGER NOT NULL DEFAULT 1;
"#;

pub(crate) fn migrate_v44(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_sync_automatically = has_column(conn, "device_settings", "sync_automatically")?;
    if version >= 44 && has_sync_automatically {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if !has_sync_automatically {
        transaction.execute_batch(ADD_SYNC_AUTOMATICALLY)?;
    }
    transaction.pragma_update(None, "user_version", version.max(44))?;
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
