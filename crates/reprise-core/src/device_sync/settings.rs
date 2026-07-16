//! Durable per-device synchronization preferences and managed-file inventory.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::view_source::ViewSource;

const SUPPORTED_OPUS_BITRATES: [u32; 6] = [0, 64, 96, 128, 160, 192];

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
    pub opus_bitrate: u32,
    pub ratings_back: bool,
    pub remove_deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceFileRecord {
    pub device_serial: String,
    pub track_id: i64,
    pub device_path: String,
    pub size: u64,
    pub mtime: i64,
    pub pinned: bool,
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
            "SELECT device_name, selection_json, opus_bitrate, ratings_back, remove_deleted \
             FROM device_settings WHERE device_serial = ?1",
            [serial],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            },
        )
        .optional()?;
    if let Some((device_name, selection, bitrate, _ratings_back, remove_deleted)) = existing {
        let bitrate = u32::try_from(bitrate).unwrap_or(0);
        return Ok(DeviceSettings {
            device_serial: serial.to_string(),
            device_name,
            selection: decode_selection(&selection)?,
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
    conn.execute(
        "INSERT INTO device_settings \
         (device_serial, device_name, selection_json, opus_bitrate, ratings_back, remove_deleted) \
         VALUES (?1, ?2, ?3, ?4, 0, ?5) \
         ON CONFLICT(device_serial) DO UPDATE SET \
           device_name = excluded.device_name, \
           selection_json = excluded.selection_json, \
           opus_bitrate = excluded.opus_bitrate, \
           ratings_back = 0, \
           remove_deleted = excluded.remove_deleted",
        params![
            settings.device_serial,
            settings.device_name,
            selection,
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
        "SELECT track_id, device_path, size, mtime, pinned \
         FROM device_files WHERE device_serial = ?1 ORDER BY track_id",
    )?;
    let rows = statement.query_map([serial], |row| {
        let size = row.get::<_, i64>(2)?;
        Ok(DeviceFileRecord {
            device_serial: serial.to_string(),
            track_id: row.get(0)?,
            device_path: row.get(1)?,
            size: u64::try_from(size).unwrap_or(0),
            mtime: row.get(3)?,
            pinned: row.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn upsert_device_file(
    conn: &Connection,
    file: &DeviceFileRecord,
) -> Result<(), DeviceSettingsError> {
    let size =
        i64::try_from(file.size).map_err(|_| DeviceSettingsError::FileTooLarge(file.size))?;
    conn.execute(
        "INSERT INTO device_files \
         (device_serial, track_id, device_path, size, mtime, pinned) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(device_serial, track_id) DO UPDATE SET \
           device_path = excluded.device_path, size = excluded.size, \
           mtime = excluded.mtime, pinned = device_files.pinned",
        params![
            file.device_serial,
            file.track_id,
            file.device_path,
            size,
            file.mtime,
            file.pinned
        ],
    )?;
    Ok(())
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
    if selection == &DeviceSelection::EntireLibrary {
        return crate::queries::query_track_ids(
            conn,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            &[],
        );
    }
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
