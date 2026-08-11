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

// Historical shape only (`MTP-38`): three named, per-device sync targets
// replacing the single implicit managed folder from `78e379fd`. It is still
// created verbatim so a fresh database walks the same path an existing one
// did; `migrate_v68` then reduces it to the one row per device that
// `MTP-54` keeps. Nothing in the code reads `kind` or `cap_bytes` any more —
// the enum behind `kind` and the per-target size cap are both gone. In the
// surviving columns, `storage_id` is an MTP `StorageID` (not a path
// component, and never a persisted object handle — handles are not stable
// across reconnects) and `path` is the device-relative target folder.
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

// Design 7a/7e (`docs/plans/podcasts-youtube-radio-turn6.md` §3b, §8a
// `E-6`): the device view's "Sync automatically when this phone connects"
// switch. Like `remove_deleted`, this is a per-device choice, so it lives
// beside it on `device_settings` rather than on `device_sync_targets`.
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

// Historical shape only (design 7f, `MTP-43`): "Download missing files before
// syncing", per-device beside `sync_automatically`/`remove_deleted`. Its only
// reader was `device_sync::preparation::plan_preparation`, which decided the
// offline/metered overrides; that module is gone with the two-step
// preparation flow, so `migrate_v68` drops the column again.
const ADD_PREPARE_BEFORE_SYNC: &str = r#"
ALTER TABLE device_settings
  ADD COLUMN prepare_before_sync INTEGER NOT NULL DEFAULT 1;
"#;

pub(crate) fn migrate_v46(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 68 {
        return Ok(());
    }
    let has_prepare_before_sync = has_column(conn, "device_settings", "prepare_before_sync")?;
    if version >= 46 && has_prepare_before_sync {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if !has_prepare_before_sync {
        transaction.execute_batch(ADD_PREPARE_BEFORE_SYNC)?;
    }
    transaction.pragma_update(None, "user_version", version.max(46))?;
    transaction.commit()
}

/// Drops the subscription-fed phone targets while preserving the playlists
/// target exactly as configured. Files already on the device are never
/// touched; only local inventory rows under the retired target paths leave the
/// database so Reprise no longer claims to manage them.
///
/// It also drops three columns whose last reader left with those targets:
/// `podcast_subscriptions.sync_to_phone` (`MTP-40`),
/// `device_settings.prepare_before_sync` (`MTP-43`) and
/// `podcast_subscriptions.latest_per_channel` (`MTP-36`). Each drop is
/// guarded by its own column the way the sibling migrations guard their
/// additions, so a database that already ran an earlier build of v68 — the
/// version has never shipped — is repaired on the next start instead of
/// keeping the columns forever behind a satisfied version check.
pub(crate) fn migrate_v68(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let subscriptions_sync_to_phone = has_column(conn, "podcast_subscriptions", "sync_to_phone")?;
    let settings_prepare_before_sync = has_column(conn, "device_settings", "prepare_before_sync")?;
    let subscriptions_latest_per_channel =
        has_column(conn, "podcast_subscriptions", "latest_per_channel")?;
    if version >= 68
        && !subscriptions_sync_to_phone
        && !settings_prepare_before_sync
        && !subscriptions_latest_per_channel
    {
        return Ok(());
    }
    let target_has_kind = has_column(conn, "device_sync_targets", "kind")?;
    let inventory_has_path = has_column(conn, "device_files", "device_path")?;
    let episodes_have_wanted = has_column(conn, "podcast_episodes", "wanted_on_device")?;
    let transaction = conn.unchecked_transaction()?;
    if target_has_kind {
        if inventory_has_path {
            transaction.execute_batch(
                r#"
        DELETE FROM device_files
        WHERE EXISTS (
          SELECT 1
          FROM device_sync_targets AS target
          WHERE target.device_serial = device_files.device_serial
            AND target.kind IN ('youtube_audio', 'podcast_episodes')
            AND (
              ltrim(device_files.device_path, '/') = ltrim(target.path, '/')
              OR substr(
                   ltrim(device_files.device_path, '/'),
                   1,
                   length(ltrim(target.path, '/')) + 1
                 ) = ltrim(target.path, '/') || '/'
            )
        );
        "#,
            )?;
        }

        transaction.execute_batch(
            r#"
        DELETE FROM device_sync_targets
        WHERE kind IN ('youtube_audio', 'podcast_episodes');

        CREATE TABLE device_sync_targets_v68 (
          device_serial TEXT PRIMARY KEY,
          storage_id    INTEGER,
          path          TEXT NOT NULL CHECK (length(trim(path)) > 0),
          enabled       INTEGER NOT NULL DEFAULT 1
        );
        INSERT INTO device_sync_targets_v68 (device_serial, storage_id, path, enabled)
        SELECT device_serial, storage_id, path, enabled
        FROM device_sync_targets
        WHERE kind = 'playlists';
        DROP TABLE device_sync_targets;
        ALTER TABLE device_sync_targets_v68 RENAME TO device_sync_targets;
        "#,
        )?;
    }
    transaction.execute_batch("DROP TABLE IF EXISTS podcast_subscription_devices;")?;
    if episodes_have_wanted {
        transaction.execute_batch("ALTER TABLE podcast_episodes DROP COLUMN wanted_on_device;")?;
    }
    if subscriptions_sync_to_phone {
        transaction
            .execute_batch("ALTER TABLE podcast_subscriptions DROP COLUMN sync_to_phone;")?;
    }
    if settings_prepare_before_sync {
        transaction
            .execute_batch("ALTER TABLE device_settings DROP COLUMN prepare_before_sync;")?;
    }
    if subscriptions_latest_per_channel {
        transaction
            .execute_batch("ALTER TABLE podcast_subscriptions DROP COLUMN latest_per_channel;")?;
    }
    transaction.pragma_update(None, "user_version", 68)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn open_v67_device_sync_shape() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE device_sync_targets (
               device_serial TEXT NOT NULL,
               kind TEXT NOT NULL,
               storage_id INTEGER,
               path TEXT NOT NULL,
               enabled INTEGER NOT NULL DEFAULT 1,
               cap_bytes INTEGER,
               PRIMARY KEY (device_serial, kind)
             );
             CREATE TABLE podcast_subscription_devices (
               subscription_id INTEGER NOT NULL,
               device_id TEXT NOT NULL
             );
             CREATE TABLE podcast_episodes (
               id INTEGER PRIMARY KEY,
               title TEXT NOT NULL,
               wanted_on_device INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE podcast_subscriptions (
               id INTEGER PRIMARY KEY,
               title TEXT NOT NULL,
               keep_downloaded INTEGER,
               sync_to_phone INTEGER NOT NULL DEFAULT 0,
               latest_per_channel INTEGER
             );
             CREATE TABLE device_settings (
               device_serial TEXT PRIMARY KEY,
               device_name TEXT NOT NULL,
               prepare_before_sync INTEGER NOT NULL DEFAULT 1
             );
             CREATE TABLE device_files (
               device_serial TEXT NOT NULL,
               track_id INTEGER NOT NULL,
               device_path TEXT NOT NULL,
               PRIMARY KEY (device_serial, track_id)
             );
             PRAGMA user_version = 67;",
        )
        .unwrap();
        conn
    }

    #[test]
    fn migration_v67_to_v68_keeps_only_the_playlist_target_and_its_inventory() {
        let conn = open_v67_device_sync_shape();
        conn.execute_batch(
            "INSERT INTO device_sync_targets VALUES
               ('pixel', 'playlists', 65537, '/Music/My Reprise', 1, NULL),
               ('pixel', 'youtube_audio', 65537, '/Media/Channels', 1, 1000),
               ('pixel', 'podcast_episodes', 65538, '/Podcasts/Reprise', 1, 2000);
             INSERT INTO podcast_subscription_devices VALUES (1, 'pixel');
             INSERT INTO podcast_episodes VALUES (1, 'Episode', 1);
             INSERT INTO device_files VALUES
               ('pixel', 1, 'Music/My Reprise/Artist/Track.opus'),
               ('pixel', 2, '/Media/Channels/Channel/Video.opus'),
               ('pixel', 3, 'Podcasts/Reprise/Show/Episode.mp3'),
               ('pixel', 4, 'Podcasts/Unmanaged/Keep.mp3');",
        )
        .unwrap();

        migrate_v68(&conn).unwrap();

        let target: (Option<i64>, String, bool) = conn
            .query_row(
                "SELECT storage_id, path, enabled FROM device_sync_targets",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(target, (Some(65537), "/Music/My Reprise".into(), true));
        let target_columns = conn
            .prepare("PRAGMA table_info(device_sync_targets)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            target_columns,
            ["device_serial", "storage_id", "path", "enabled"]
        );
        assert!(!has_column(&conn, "podcast_episodes", "wanted_on_device").unwrap());
        let subscription_devices_exist: bool = conn
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master
                   WHERE type = 'table' AND name = 'podcast_subscription_devices'
                 )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!subscription_devices_exist);
        let paths = conn
            .prepare("SELECT device_path FROM device_files ORDER BY track_id")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            paths,
            [
                "Music/My Reprise/Artist/Track.opus",
                "Podcasts/Unmanaged/Keep.mp3"
            ]
        );
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 68);
    }

    #[test]
    fn migration_v67_to_v68_drops_the_columns_that_lost_their_last_reader() {
        let conn = open_v67_device_sync_shape();
        conn.execute_batch(
            "INSERT INTO podcast_subscriptions VALUES (1, 'Show', 5, 1, 3);
             INSERT INTO device_settings VALUES ('pixel', 'Pixel', 1);",
        )
        .unwrap();

        migrate_v68(&conn).unwrap();

        assert!(!has_column(&conn, "podcast_subscriptions", "sync_to_phone").unwrap());
        assert!(!has_column(&conn, "device_settings", "prepare_before_sync").unwrap());
        assert!(!has_column(&conn, "podcast_subscriptions", "latest_per_channel").unwrap());
        let surviving: (String, Option<i64>) = conn
            .query_row(
                "SELECT title, keep_downloaded FROM podcast_subscriptions WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            surviving,
            ("Show".into(), Some(5)),
            "the desktop's own per-channel settings must survive untouched — \
             only the phone-sync columns go"
        );
        let device_name: String = conn
            .query_row("SELECT device_name FROM device_settings", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(device_name, "Pixel");
    }

    #[test]
    fn migration_v68_repairs_a_database_that_already_ran_an_earlier_v68() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE device_sync_targets (
               device_serial TEXT PRIMARY KEY,
               storage_id INTEGER,
               path TEXT NOT NULL,
               enabled INTEGER NOT NULL DEFAULT 1
             );
             INSERT INTO device_sync_targets VALUES
               ('pixel', 65537, '/Music/Reprise', 1);
             CREATE TABLE podcast_subscriptions (
               id INTEGER PRIMARY KEY,
               title TEXT NOT NULL,
               sync_to_phone INTEGER NOT NULL DEFAULT 0,
               latest_per_channel INTEGER
             );
             CREATE TABLE device_settings (
               device_serial TEXT PRIMARY KEY,
               prepare_before_sync INTEGER NOT NULL DEFAULT 1
             );
             PRAGMA user_version = 68;",
        )
        .unwrap();

        migrate_v68(&conn).unwrap();

        assert!(
            !has_column(&conn, "podcast_subscriptions", "sync_to_phone").unwrap(),
            "v68 has never shipped, so a database that ran an earlier build of \
             it must still lose the columns rather than keep them forever \
             behind a satisfied version check"
        );
        assert!(!has_column(&conn, "device_settings", "prepare_before_sync").unwrap());
        assert!(!has_column(&conn, "podcast_subscriptions", "latest_per_channel").unwrap());
        let target: (Option<i64>, String, bool) = conn
            .query_row(
                "SELECT storage_id, path, enabled FROM device_sync_targets",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            target,
            (Some(65537), "/Music/Reprise".into(), true),
            "re-entering v68 must not disturb the playlists target it already settled"
        );
    }

    #[test]
    fn migration_v68_is_a_no_op_when_the_database_is_already_current() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE device_sync_targets (
               device_serial TEXT PRIMARY KEY,
               storage_id INTEGER,
               path TEXT NOT NULL,
               enabled INTEGER NOT NULL DEFAULT 1
             );
             INSERT INTO device_sync_targets VALUES
               ('pixel', 65537, '/Music/Reprise', 1);
             PRAGMA user_version = 68;",
        )
        .unwrap();

        migrate_v68(&conn).unwrap();

        let target: (Option<i64>, String, bool) = conn
            .query_row(
                "SELECT storage_id, path, enabled FROM device_sync_targets",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(target, (Some(65537), "/Music/Reprise".into(), true));
    }
}
