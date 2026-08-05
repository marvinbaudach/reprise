//! File facts displayed beside a sound profile.

use std::path::{Path, PathBuf};

use lofty::file::AudioFile;

use crate::db::{Db, DbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundFileInfo {
    pub format: String,
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub file_size: u64,
    pub occupied_upper_hz: Option<u32>,
}

pub fn load_sound_file_info(db: &Db, track_id: i64) -> Result<Option<SoundFileInfo>, DbError> {
    let row = db
        .conn()
        .query_row(
            "SELECT path, bitrate_kbps, file_size FROM tracks \
             WHERE id = ?1 AND missing_since IS NULL AND removed_at IS NULL",
            [track_id],
            |row| {
                Ok((
                    PathBuf::from(row.get::<_, String>(0)?),
                    row.get::<_, Option<u32>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((path, bitrate_kbps, file_size)) = row else {
        return Ok(None);
    };
    let properties = lofty::probe::Probe::open(&path)
        .and_then(lofty::probe::Probe::read)
        .ok()
        .map(|tagged| tagged.properties().clone());
    let occupied_upper_hz = crate::db::get_track_spectrogram(db, track_id)?
        .and_then(|spectrogram| spectrogram.occupied_upper_hz());
    Ok(Some(SoundFileInfo {
        format: extension_label(&path),
        bit_depth: properties
            .as_ref()
            .and_then(lofty::properties::FileProperties::bit_depth),
        sample_rate_hz: properties
            .as_ref()
            .and_then(lofty::properties::FileProperties::sample_rate),
        bitrate_kbps,
        file_size: u64::try_from(file_size).unwrap_or(0),
        occupied_upper_hz,
    }))
}

fn extension_label(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_uppercase)
        .unwrap_or_default()
}

use rusqlite::OptionalExtension;
