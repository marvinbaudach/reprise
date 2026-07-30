//! Durable per-device synchronization preferences and managed-file inventory.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::view_source::ViewSource;

use super::{Mp3Quality, TransferProfile};

pub const SUPPORTED_OPUS_BITRATES: [u32; 7] = [0, 64, 96, 128, 160, 192, 256];

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SelectionSource {
    Playlist(i64),
    Smart(i64),
}

impl SelectionSource {
    fn encode(&self) -> String {
        match self {
            Self::Playlist(id) => format!("playlist:{id}"),
            Self::Smart(id) => format!("smart:{id}"),
        }
    }

    fn decode(value: &str) -> Option<Self> {
        let (kind, id) = value.split_once(':')?;
        let id = id.parse::<i64>().ok().filter(|id| *id > 0)?;
        match kind {
            "playlist" => Some(Self::Playlist(id)),
            "smart" => Some(Self::Smart(id)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceSelection {
    EntireLibrary,
    Sources(Vec<SelectionSource>),
}

impl Default for DeviceSelection {
    fn default() -> Self {
        Self::Sources(Vec::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSettings {
    pub device_serial: String,
    pub device_name: String,
    pub selection: DeviceSelection,
    pub profile: TransferProfile,
    pub opus_bitrate: u32,
    pub ratings_back: bool,
    pub remove_deleted: bool,
    /// "Sync automatically when this phone connects" (design 7a). A
    /// per-device choice, not one of 7b's global sync rules — see the
    /// module doc on `db_device_sync::migrate_v44`.
    pub sync_automatically: bool,
    /// "Download missing files before syncing" (design 7f, `MTP-43`). The
    /// device's own input to `preparation::plan_preparation`'s precedence —
    /// offline and metered are decided there from live facts, never by
    /// mutating this stored value, so the switch always reflects what the
    /// user actually chose. Defaults to `true` (`db_device_sync::migrate_v46`).
    pub prepare_before_sync: bool,
}

impl DeviceSettings {
    /// Session-only defaults for a device whose platform cannot supply a
    /// stable identity. Callers may mutate this value while the cable is
    /// connected, but must never pass it to [`save_settings`].
    #[must_use]
    pub fn transient(serial: &str, name: &str) -> Self {
        Self {
            device_serial: serial.to_string(),
            device_name: name.to_string(),
            selection: DeviceSelection::default(),
            profile: TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            // An unrememberable device must not silently auto-start on every
            // replug as though the app remembered a user choice.
            sync_automatically: false,
            prepare_before_sync: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceFileRecord {
    pub device_serial: String,
    pub track_id: i64,
    pub source_path: String,
    pub source_size: u64,
    pub source_mtime: i64,
    pub device_path: String,
    pub device_size: u64,
    pub profile_fingerprint: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePlaylistRecord {
    pub device_serial: String,
    pub source: SelectionSource,
    pub source_name: String,
    pub device_path: String,
    pub last_synced_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RememberedDevice {
    pub stable_id: String,
    pub local_name: String,
    pub last_verified_at: Option<i64>,
    pub size_on_device_bytes: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceSettingsError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid synchronization selection: {0}")]
    Selection(#[from] serde_json::Error),
    #[error("unsupported Opus bitrate: {0}")]
    UnsupportedBitrate(u32),
    #[error("device file size is too large for SQLite: {0}")]
    FileTooLarge(u64),
    #[error("device name must not be empty")]
    EmptyDeviceName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyDeviceRekey {
    NoLegacyRow,
    AmbiguousLegacyRows,
    StableKeyAlreadyExists,
    Rekeyed,
}

const ADD_LAST_VERIFIED_AT: &str =
    "ALTER TABLE device_settings ADD COLUMN last_verified_at INTEGER";
const ADD_SIZE_ON_DEVICE: &str = "ALTER TABLE device_settings ADD COLUMN size_on_device INTEGER";

fn ensure_remembered_device_columns(db: &crate::db::Db) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    for (column, statement) in [
        ("last_verified_at", ADD_LAST_VERIFIED_AT),
        ("size_on_device", ADD_SIZE_ON_DEVICE),
    ] {
        let exists = conn.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM pragma_table_info('device_settings') WHERE name = ?1
             )",
            [column],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            conn.execute(statement, [])?;
        }
    }
    Ok(())
}

pub fn list_remembered_devices(
    db: &crate::db::Db,
) -> Result<Vec<RememberedDevice>, DeviceSettingsError> {
    ensure_remembered_device_columns(db)?;
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT device_serial, device_name, last_verified_at, size_on_device
           FROM device_settings
          WHERE device_serial NOT LIKE 'mtp://%'
          ORDER BY device_name COLLATE NOCASE, device_serial",
    )?;
    let rows = statement.query_map([], |row| {
        let size = row.get::<_, Option<i64>>(3)?;
        Ok(RememberedDevice {
            stable_id: row.get(0)?,
            local_name: row.get(1)?,
            last_verified_at: row.get(2)?,
            size_on_device_bytes: size.map(|value| value.max(0) as u64),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn record_device_verification(
    db: &crate::db::Db,
    stable_id: &str,
    verified_at: i64,
    size_on_device_bytes: u64,
) -> Result<(), DeviceSettingsError> {
    ensure_remembered_device_columns(db)?;
    let size = sqlite_size(size_on_device_bytes)?;
    db.conn().execute(
        "UPDATE device_settings
            SET last_verified_at = ?2, size_on_device = ?3
          WHERE device_serial = ?1",
        params![stable_id, verified_at, size],
    )?;
    Ok(())
}

pub fn rename_device(
    db: &crate::db::Db,
    stable_id: &str,
    local_name: &str,
) -> Result<(), DeviceSettingsError> {
    let local_name = local_name.trim();
    if local_name.is_empty() {
        return Err(DeviceSettingsError::EmptyDeviceName);
    }
    db.conn().execute(
        "UPDATE device_settings SET device_name = ?2 WHERE device_serial = ?1",
        params![stable_id, local_name],
    )?;
    Ok(())
}

pub fn forget_device(db: &crate::db::Db, stable_id: &str) -> Result<(), DeviceSettingsError> {
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "DELETE FROM sync_events
          WHERE run_id IN (SELECT id FROM sync_runs WHERE device_serial = ?1)",
        [stable_id],
    )?;
    for (table, column) in [
        ("device_files", "device_serial"),
        ("device_playlists", "device_serial"),
        ("device_sync_targets", "device_serial"),
        ("sync_runs", "device_serial"),
        ("podcast_subscription_devices", "device_id"),
        ("device_settings", "device_serial"),
    ] {
        transaction.execute(
            &format!("DELETE FROM {table} WHERE {column} = ?1"),
            [stable_id],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

/// Moves every persisted device-owned row off a volatile legacy MTP URI once
/// the same connection exposes a stable identity. A pre-existing stable row
/// wins without merging guesses; the transaction then leaves the legacy row
/// intact for an explicit later decision.
pub fn rekey_legacy_device(
    db: &crate::db::Db,
    legacy_uri: &str,
    stable_id: &str,
) -> Result<LegacyDeviceRekey, rusqlite::Error> {
    let conn = db.conn();
    let exact_legacy_exists = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM device_settings WHERE device_serial = ?1
         )",
        [legacy_uri],
        |row| row.get::<_, bool>(0),
    )?;
    let legacy_key = if exact_legacy_exists {
        legacy_uri.to_string()
    } else {
        let mut statement = conn.prepare(
            "SELECT device_serial
               FROM device_settings
              WHERE device_serial LIKE 'mtp://%'
              ORDER BY device_serial
              LIMIT 2",
        )?;
        let candidates = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        match candidates.as_slice() {
            [] => return Ok(LegacyDeviceRekey::NoLegacyRow),
            [only] => only.clone(),
            _ => return Ok(LegacyDeviceRekey::AmbiguousLegacyRows),
        }
    };
    let stable_exists = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM device_settings WHERE device_serial = ?1
         )",
        [stable_id],
        |row| row.get::<_, bool>(0),
    )?;
    if stable_exists {
        return Ok(LegacyDeviceRekey::StableKeyAlreadyExists);
    }

    let transaction = conn.unchecked_transaction()?;
    for (table, column) in [
        ("device_files", "device_serial"),
        ("device_playlists", "device_serial"),
        ("device_sync_targets", "device_serial"),
        ("sync_runs", "device_serial"),
        ("podcast_subscription_devices", "device_id"),
        ("device_settings", "device_serial"),
    ] {
        transaction.execute(
            &format!("UPDATE {table} SET {column} = ?2 WHERE {column} = ?1"),
            params![legacy_key, stable_id],
        )?;
    }
    transaction.commit()?;
    Ok(LegacyDeviceRekey::Rekeyed)
}

pub fn load_or_create_settings(
    db: &crate::db::Db,
    serial: &str,
    name: &str,
) -> Result<DeviceSettings, DeviceSettingsError> {
    ensure_remembered_device_columns(db)?;
    let conn = db.conn();
    let existing = conn
        .query_row(
            "SELECT device_name, selection_json, mp3_quality, transfer_profile, opus_bitrate, \
                    ratings_back, remove_deleted, sync_automatically, prepare_before_sync \
             FROM device_settings WHERE device_serial = ?1",
            [serial],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, bool>(8)?,
                ))
            },
        )
        .optional()?;
    if let Some((
        device_name,
        selection,
        _mp3_quality,
        transfer_profile,
        bitrate,
        _ratings_back,
        remove_deleted,
        sync_automatically,
        prepare_before_sync,
    )) = existing
    {
        let bitrate = u32::try_from(bitrate).unwrap_or(0);
        return Ok(DeviceSettings {
            device_serial: serial.to_string(),
            device_name,
            selection: decode_selection(&selection)?,
            profile: TransferProfile::from_storage_value(&transfer_profile).unwrap_or_default(),
            opus_bitrate: normalized_bitrate(bitrate),
            ratings_back: false,
            remove_deleted,
            sync_automatically,
            prepare_before_sync,
        });
    }

    conn.execute(
        "INSERT INTO device_settings (device_serial, device_name) VALUES (?1, ?2)",
        params![serial, name],
    )?;
    let mut settings = DeviceSettings::transient(serial, name);
    settings.sync_automatically = true;
    Ok(settings)
}

pub fn save_settings(
    db: &crate::db::Db,
    settings: &DeviceSettings,
) -> Result<(), DeviceSettingsError> {
    let conn = db.conn();
    if !SUPPORTED_OPUS_BITRATES.contains(&settings.opus_bitrate) {
        return Err(DeviceSettingsError::UnsupportedBitrate(
            settings.opus_bitrate,
        ));
    }
    let selection = encode_selection(&settings.selection)?;
    let mp3_quality = match settings.profile {
        TransferProfile::Mp3(quality) => quality,
        TransferProfile::Opus160 | TransferProfile::Original => Mp3Quality::default(),
    };
    conn.execute(
        "INSERT INTO device_settings \
         (device_serial, device_name, selection_json, mp3_quality, transfer_profile, \
          opus_bitrate, ratings_back, remove_deleted, sync_automatically, prepare_before_sync) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9) \
         ON CONFLICT(device_serial) DO UPDATE SET \
           device_name = excluded.device_name, \
           selection_json = excluded.selection_json, \
           mp3_quality = excluded.mp3_quality, \
           transfer_profile = excluded.transfer_profile, \
           opus_bitrate = excluded.opus_bitrate, \
           ratings_back = 0, \
           remove_deleted = excluded.remove_deleted, \
           sync_automatically = excluded.sync_automatically, \
           prepare_before_sync = excluded.prepare_before_sync",
        params![
            settings.device_serial,
            settings.device_name,
            selection,
            mp3_quality.kbps(),
            settings.profile.storage_value(),
            settings.opus_bitrate,
            settings.remove_deleted,
            settings.sync_automatically,
            settings.prepare_before_sync,
        ],
    )?;
    Ok(())
}

pub fn load_device_files(
    db: &crate::db::Db,
    serial: &str,
) -> Result<Vec<DeviceFileRecord>, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT track_id, source_path, source_size, source_mtime, device_path, device_size, \
                profile_fingerprint, pinned \
         FROM device_files WHERE device_serial = ?1 ORDER BY track_id",
    )?;
    let rows = statement.query_map([serial], |row| {
        let source_size = row.get::<_, i64>(2)?;
        let device_size = row.get::<_, i64>(5)?;
        Ok(DeviceFileRecord {
            device_serial: serial.to_string(),
            track_id: row.get(0)?,
            source_path: row.get(1)?,
            source_size: u64::try_from(source_size).unwrap_or(0),
            source_mtime: row.get(3)?,
            device_path: row.get(4)?,
            device_size: u64::try_from(device_size).unwrap_or(0),
            profile_fingerprint: row.get(6)?,
            pinned: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn upsert_device_file(
    db: &crate::db::Db,
    file: &DeviceFileRecord,
) -> Result<(), DeviceSettingsError> {
    let conn = db.conn();
    let source_size = sqlite_size(file.source_size)?;
    let device_size = sqlite_size(file.device_size)?;
    conn.execute(
        "INSERT INTO device_files \
         (device_serial, track_id, source_path, source_size, source_mtime, device_path, \
          device_size, profile_fingerprint, pinned) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(device_serial, track_id) DO UPDATE SET \
           source_path = excluded.source_path, source_size = excluded.source_size, \
           source_mtime = excluded.source_mtime, device_path = excluded.device_path, \
           device_size = excluded.device_size, \
           profile_fingerprint = excluded.profile_fingerprint, pinned = device_files.pinned",
        params![
            file.device_serial,
            file.track_id,
            file.source_path,
            source_size,
            file.source_mtime,
            file.device_path,
            device_size,
            file.profile_fingerprint,
            file.pinned
        ],
    )?;
    Ok(())
}

pub fn load_device_playlists(
    db: &crate::db::Db,
    serial: &str,
) -> Result<Vec<DevicePlaylistRecord>, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT source_kind, source_id, source_name, device_path, last_synced_at \
         FROM device_playlists \
         WHERE device_serial = ?1 \
         ORDER BY source_kind, source_id",
    )?;
    let rows = statement.query_map([serial], |row| {
        let kind = row.get::<_, String>(0)?;
        let id = row.get::<_, i64>(1)?;
        Ok(DevicePlaylistRecord {
            device_serial: serial.to_string(),
            source: decode_source_columns(&kind, id)?,
            source_name: row.get(2)?,
            device_path: row.get(3)?,
            last_synced_at: row.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn upsert_device_playlist(
    db: &crate::db::Db,
    playlist: &DevicePlaylistRecord,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let (kind, id) = source_columns(&playlist.source);
    conn.execute(
        "INSERT INTO device_playlists \
         (device_serial, source_kind, source_id, source_name, device_path, last_synced_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(device_serial, source_kind, source_id) DO UPDATE SET \
           source_name = excluded.source_name, device_path = excluded.device_path, \
           last_synced_at = COALESCE(excluded.last_synced_at, device_playlists.last_synced_at)",
        params![
            playlist.device_serial,
            kind,
            id,
            playlist.source_name,
            playlist.device_path,
            playlist.last_synced_at,
        ],
    )?;
    Ok(())
}

pub fn mark_device_playlists_synced(
    db: &crate::db::Db,
    serial: &str,
    sources: &[SelectionSource],
    timestamp: i64,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    let mut seen = HashSet::new();
    for source in sources {
        if !seen.insert(source) {
            continue;
        }
        let (kind, id) = source_columns(source);
        let updated = transaction.execute(
            "UPDATE device_playlists SET last_synced_at = ?4 \
             WHERE device_serial = ?1 AND source_kind = ?2 AND source_id = ?3",
            params![serial, kind, id, timestamp],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
    }
    transaction.commit()
}

pub fn delete_device_playlist(
    db: &crate::db::Db,
    serial: &str,
    source: &SelectionSource,
) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    let (kind, id) = source_columns(source);
    Ok(conn.execute(
        "DELETE FROM device_playlists \
         WHERE device_serial = ?1 AND source_kind = ?2 AND source_id = ?3",
        params![serial, kind, id],
    )? > 0)
}

pub fn set_file_pinned(
    db: &crate::db::Db,
    serial: &str,
    track_id: i64,
    pinned: bool,
) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    Ok(conn.execute(
        "UPDATE device_files SET pinned = ?3 WHERE device_serial = ?1 AND track_id = ?2",
        params![serial, track_id, pinned],
    )? > 0)
}

pub fn delete_device_file(
    db: &crate::db::Db,
    serial: &str,
    track_id: i64,
) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    Ok(conn.execute(
        "DELETE FROM device_files WHERE device_serial = ?1 AND track_id = ?2",
        params![serial, track_id],
    )? > 0)
}

pub fn resolve_selection_track_ids(
    db: &crate::db::Db,
    selection: &DeviceSelection,
) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = db.conn();
    resolve_selection_track_ids_in(conn, selection)
}

fn resolve_selection_track_ids_in(
    conn: &Connection,
    selection: &DeviceSelection,
) -> Result<Vec<i64>, rusqlite::Error> {
    if selection == &DeviceSelection::EntireLibrary {
        return crate::queries::query_track_ids_in(
            conn,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            &[],
        );
    }
    let DeviceSelection::Sources(sources) = selection else {
        unreachable!("the entire-library case returned above");
    };
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for source in sources {
        let source = match source {
            SelectionSource::Playlist(id) => ViewSource::Playlist(*id),
            SelectionSource::Smart(id) => ViewSource::Smart(*id),
        };
        for id in crate::queries::query_track_ids_in(conn, &source, "title", "asc", "", &[])? {
            if seen.insert(id) {
                selected.push(id);
            }
        }
    }
    Ok(selected)
}

fn encode_selection(selection: &DeviceSelection) -> Result<String, serde_json::Error> {
    match selection {
        DeviceSelection::EntireLibrary => serde_json::to_string("entire_library"),
        DeviceSelection::Sources(sources) => {
            let values = sources
                .iter()
                .map(SelectionSource::encode)
                .collect::<Vec<_>>();
            serde_json::to_string(&values)
        }
    }
}

fn decode_selection(value: &str) -> Result<DeviceSelection, serde_json::Error> {
    let decoded = serde_json::from_str::<serde_json::Value>(value)?;
    if decoded.as_str() == Some("entire_library") {
        return Ok(DeviceSelection::EntireLibrary);
    }
    let Some(values) = decoded.as_array() else {
        return Err(unknown_selection_error());
    };
    let Some(sources) = values
        .iter()
        .map(|value| value.as_str().and_then(SelectionSource::decode))
        .collect::<Option<Vec<_>>>()
    else {
        return Err(unknown_selection_error());
    };
    Ok(DeviceSelection::Sources(sources))
}

fn unknown_selection_error() -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unrecognized synchronization selection shape",
    ))
}

fn normalized_bitrate(value: u32) -> u32 {
    if SUPPORTED_OPUS_BITRATES.contains(&value) {
        value
    } else {
        0
    }
}

fn sqlite_size(value: u64) -> Result<i64, DeviceSettingsError> {
    i64::try_from(value).map_err(|_| DeviceSettingsError::FileTooLarge(value))
}

fn source_columns(source: &SelectionSource) -> (&'static str, i64) {
    if source == &super::selection::EVERYTHING_SOURCE {
        return ("library", 0);
    }
    match source {
        SelectionSource::Playlist(id) => ("playlist", *id),
        SelectionSource::Smart(id) => ("smart", *id),
    }
}

fn decode_source_columns(kind: &str, id: i64) -> Result<SelectionSource, rusqlite::Error> {
    let source = match kind {
        "library" if id == 0 => super::selection::EVERYTHING_SOURCE,
        "playlist" if id > 0 => SelectionSource::Playlist(id),
        "smart" if id > 0 => SelectionSource::Smart(id),
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid device playlist source",
                )),
            ));
        }
    };
    Ok(source)
}
