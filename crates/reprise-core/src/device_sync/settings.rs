//! Durable per-device synchronization preferences and managed-file inventory.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::view_source::ViewSource;

use super::{Mp3Quality, TransferProfile};

pub const SUPPORTED_OPUS_BITRATES: [u32; 7] = [0, 64, 96, 128, 160, 192, 256];

#[derive(Clone, Debug, PartialEq, Eq)]
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
}

pub fn load_or_create_settings(
    conn: &Connection,
    serial: &str,
    name: &str,
) -> Result<DeviceSettings, DeviceSettingsError> {
    let existing = conn
        .query_row(
            "SELECT device_name, selection_json, mp3_quality, opus_bitrate, ratings_back, \
                    remove_deleted \
             FROM device_settings WHERE device_serial = ?1",
            [serial],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .optional()?;
    if let Some((device_name, selection, mp3_quality, bitrate, _ratings_back, remove_deleted)) =
        existing
    {
        let bitrate = u32::try_from(bitrate).unwrap_or(0);
        let mp3_quality = u32::try_from(mp3_quality)
            .ok()
            .and_then(|quality| Mp3Quality::try_from(quality).ok())
            .unwrap_or_default();
        return Ok(DeviceSettings {
            device_serial: serial.to_string(),
            device_name,
            selection: decode_selection(&selection)?,
            profile: TransferProfile::Mp3(mp3_quality),
            opus_bitrate: normalized_bitrate(bitrate),
            ratings_back: false,
            remove_deleted,
        });
    }

    conn.execute(
        "INSERT INTO device_settings (device_serial, device_name) VALUES (?1, ?2)",
        params![serial, name],
    )?;
    Ok(DeviceSettings {
        device_serial: serial.to_string(),
        device_name: name.to_string(),
        selection: DeviceSelection::default(),
        profile: TransferProfile::default(),
        opus_bitrate: 0,
        ratings_back: false,
        remove_deleted: true,
    })
}

pub fn save_settings(
    conn: &Connection,
    settings: &DeviceSettings,
) -> Result<(), DeviceSettingsError> {
    if !SUPPORTED_OPUS_BITRATES.contains(&settings.opus_bitrate) {
        return Err(DeviceSettingsError::UnsupportedBitrate(
            settings.opus_bitrate,
        ));
    }
    let selection = encode_selection(&settings.selection)?;
    let TransferProfile::Mp3(mp3_quality) = settings.profile;
    conn.execute(
        "INSERT INTO device_settings \
         (device_serial, device_name, selection_json, mp3_quality, opus_bitrate, ratings_back, \
          remove_deleted) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6) \
         ON CONFLICT(device_serial) DO UPDATE SET \
           device_name = excluded.device_name, \
           selection_json = excluded.selection_json, \
           mp3_quality = excluded.mp3_quality, \
           opus_bitrate = excluded.opus_bitrate, \
           ratings_back = 0, \
           remove_deleted = excluded.remove_deleted",
        params![
            settings.device_serial,
            settings.device_name,
            selection,
            mp3_quality.kbps(),
            settings.opus_bitrate,
            settings.remove_deleted
        ],
    )?;
    Ok(())
}

pub fn load_device_files(
    conn: &Connection,
    serial: &str,
) -> Result<Vec<DeviceFileRecord>, rusqlite::Error> {
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
    conn: &Connection,
    file: &DeviceFileRecord,
) -> Result<(), DeviceSettingsError> {
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
    conn: &Connection,
    serial: &str,
) -> Result<Vec<DevicePlaylistRecord>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT source_kind, source_id, source_name, device_path \
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
        })
    })?;
    rows.collect()
}

pub fn upsert_device_playlist(
    conn: &Connection,
    playlist: &DevicePlaylistRecord,
) -> Result<(), rusqlite::Error> {
    let (kind, id) = source_columns(&playlist.source);
    conn.execute(
        "INSERT INTO device_playlists \
         (device_serial, source_kind, source_id, source_name, device_path) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(device_serial, source_kind, source_id) DO UPDATE SET \
           source_name = excluded.source_name, device_path = excluded.device_path",
        params![
            playlist.device_serial,
            kind,
            id,
            playlist.source_name,
            playlist.device_path
        ],
    )?;
    Ok(())
}

pub fn delete_device_playlist(
    conn: &Connection,
    serial: &str,
    source: &SelectionSource,
) -> Result<bool, rusqlite::Error> {
    let (kind, id) = source_columns(source);
    Ok(conn.execute(
        "DELETE FROM device_playlists \
         WHERE device_serial = ?1 AND source_kind = ?2 AND source_id = ?3",
        params![serial, kind, id],
    )? > 0)
}

pub fn set_file_pinned(
    conn: &Connection,
    serial: &str,
    track_id: i64,
    pinned: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE device_files SET pinned = ?3 WHERE device_serial = ?1 AND track_id = ?2",
        params![serial, track_id, pinned],
    )? > 0)
}

pub fn delete_device_file(
    conn: &Connection,
    serial: &str,
    track_id: i64,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "DELETE FROM device_files WHERE device_serial = ?1 AND track_id = ?2",
        params![serial, track_id],
    )? > 0)
}

pub fn resolve_selection_track_ids(
    conn: &Connection,
    selection: &DeviceSelection,
) -> Result<Vec<i64>, rusqlite::Error> {
    let DeviceSelection::Sources(sources) = selection else {
        return Ok(Vec::new());
    };
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for source in sources {
        let source = match source {
            SelectionSource::Playlist(id) => ViewSource::Playlist(*id),
            SelectionSource::Smart(id) => ViewSource::Smart(*id),
        };
        for id in crate::queries::query_track_ids(conn, &source, "title", "asc", "", &[])? {
            if seen.insert(id) {
                selected.push(id);
            }
        }
    }
    Ok(selected)
}

fn encode_selection(selection: &DeviceSelection) -> Result<String, serde_json::Error> {
    match selection {
        DeviceSelection::EntireLibrary => serde_json::to_string(&Vec::<String>::new()),
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
        return Ok(DeviceSelection::Sources(Vec::new()));
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
    match source {
        SelectionSource::Playlist(id) => ("playlist", *id),
        SelectionSource::Smart(id) => ("smart", *id),
    }
}

fn decode_source_columns(kind: &str, id: i64) -> Result<SelectionSource, rusqlite::Error> {
    let source = match kind {
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
