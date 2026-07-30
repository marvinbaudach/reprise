//! Non-destructive manual-playlist updates for the MCP adapter.
//!
//! This module deliberately exposes only rename and ordered append. Removing
//! membership rows and deleting playlists are outside the safe MCP surface.

use std::path::Path;

use reprise_core::library::playlists;

use crate::capability;
use crate::data::{self, DataError, MAX_TRACK_IDS};
use crate::dto::{UpdatePlaylistParams, UpdatePlaylistResult};

/// Applies one capability-gated playlist update through core facades.
pub fn update(
    path: &Path,
    granted_at_startup: bool,
    params: &UpdatePlaylistParams,
) -> Result<UpdatePlaylistResult, DataError> {
    let db = data::open(path)?;
    if !capability::playlist_manage_effective(&db, granted_at_startup).map_err(DataError::Db)? {
        return Err(DataError::CapabilityDenied("playlist:manage"));
    }

    match params.action.as_str() {
        "rename" => {
            if !params.track_ids.is_empty() {
                return Err(DataError::InvalidInput(
                    "rename does not accept track_ids".to_owned(),
                ));
            }
            let name = params
                .name
                .as_deref()
                .ok_or_else(|| DataError::InvalidInput("rename requires name".to_owned()))?;
            let name = name.trim();
            if name.is_empty() {
                return Err(DataError::InvalidInput(
                    "playlist name must not be empty".to_owned(),
                ));
            }
            let changed =
                playlists::rename(&db, params.playlist_id, name).map_err(DataError::Db)?;
            if changed == 0 {
                return Err(DataError::InvalidInput(
                    "playlist does not exist".to_owned(),
                ));
            }
            result(&db, params.playlist_id, "rename", changed)
        }
        "add_tracks" => {
            if params.name.is_some() {
                return Err(DataError::InvalidInput(
                    "add_tracks does not accept name".to_owned(),
                ));
            }
            if playlists::get(&db, params.playlist_id)
                .map_err(DataError::Db)?
                .is_none()
            {
                return Err(DataError::InvalidInput(
                    "playlist does not exist".to_owned(),
                ));
            }
            if params.track_ids.is_empty() {
                return Err(DataError::InvalidInput(
                    "at least one track id is required".to_owned(),
                ));
            }
            if params.track_ids.len() > MAX_TRACK_IDS {
                return Err(DataError::InvalidInput(format!(
                    "too many track ids: {} (maximum {MAX_TRACK_IDS})",
                    params.track_ids.len()
                )));
            }
            data::reject_absent_track_ids(&db, &params.track_ids)?;
            let inserted = playlists::add_tracks(&db, params.playlist_id, &params.track_ids)
                .map_err(DataError::Db)?;
            result(&db, params.playlist_id, "add_tracks", inserted as usize)
        }
        other => Err(DataError::InvalidInput(format!(
            "unknown playlist update action '{other}'"
        ))),
    }
}

fn result(
    db: &reprise_core::db::Db,
    playlist_id: i64,
    action: &str,
    affected: usize,
) -> Result<UpdatePlaylistResult, DataError> {
    let summary = playlists::get(db, playlist_id)
        .map_err(DataError::Db)?
        .ok_or_else(|| DataError::InvalidInput("playlist does not exist".to_owned()))?;
    Ok(UpdatePlaylistResult {
        playlist_id,
        name: summary.name,
        track_count: summary.track_count,
        action: action.to_owned(),
        affected,
    })
}
