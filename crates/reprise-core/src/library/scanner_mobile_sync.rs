//! Small desktop-to-phone cargo discovered during the library walk.
//!
//! The scanner records handles while it is already walking, then opens only
//! the root metadata list. Per-track analysis sidecars remain unopened until
//! playback asks for one.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::source::{LibraryEntry, LibrarySource};

#[derive(Default)]
pub(super) struct MobileSyncDiscovery {
    metadata_list: Option<PathBuf>,
    tracks_by_device_path: HashMap<String, String>,
    analysis_sidecars_by_device_path: HashMap<String, PathBuf>,
}

impl MobileSyncDiscovery {
    pub(super) fn observe(
        &mut self,
        source: &dyn LibrarySource,
        root: &Path,
        entry: &LibraryEntry,
    ) {
        if !entry.is_file {
            return;
        }
        let Some(relative) = source.relative_path(root, &entry.path) else {
            return;
        };
        let device_path = relative.to_string_lossy().into_owned();
        if relative.components().count() == 1
            && crate::device_sync::track_metadata_list::is_list_path(&relative)
        {
            self.metadata_list = Some(entry.path.clone());
        } else if crate::device_sync::analysis_sidecar::is_sidecar_path(&relative) {
            self.analysis_sidecars_by_device_path
                .insert(device_path, entry.path.clone());
        } else if super::is_audio_file(&relative) {
            self.tracks_by_device_path
                .insert(device_path, entry.path.to_string_lossy().into_owned());
        }
    }

    pub(super) fn register_analysis_sidecars(
        &self,
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        if self.metadata_list.is_none() && self.analysis_sidecars_by_device_path.is_empty() {
            return Ok(());
        }
        for (device_path, track_path) in &self.tracks_by_device_path {
            let sidecar = crate::device_sync::analysis_sidecar::device_path_for_track(device_path)
                .and_then(|sidecar| self.analysis_sidecars_by_device_path.get(&sidecar));
            match sidecar {
                Some(sidecar_path) => {
                    crate::db_mobile_sync::register_sidecar(conn, track_path, sidecar_path)?;
                }
                None => crate::db_mobile_sync::unregister_sidecar(conn, track_path)?,
            }
        }
        Ok(())
    }

    pub(super) fn apply_metadata(
        &self,
        source: &dyn LibrarySource,
        conn: &Connection,
    ) -> Result<u32, rusqlite::Error> {
        let Some(path) = &self.metadata_list else {
            return Ok(0);
        };
        let mut reader = match source.open_read(path) {
            Ok(reader) => reader,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not read synced track metadata");
                return Ok(0);
            }
        };
        let mut bytes = Vec::new();
        if let Err(error) = reader.read_to_end(&mut bytes) {
            tracing::warn!(path = %path.display(), %error, "could not read synced track metadata");
            return Ok(0);
        }
        let list = match crate::device_sync::track_metadata_list::TrackMetadataList::decode(&bytes)
        {
            Ok(list) => list,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not decode synced track metadata");
                return Ok(0);
            }
        };
        let mut changed = 0_u32;
        let rated_at = crate::library::stats::now_unix();
        for entry in list.entries {
            let Some(track_path) = self.tracks_by_device_path.get(&entry.device_path) else {
                continue;
            };
            let rows = conn.execute(
                "UPDATE tracks SET rating = ?1, play_count = ?2, \
                                   rated_at = CASE WHEN rating IS NOT ?1 THEN ?3 ELSE rated_at END \
                 WHERE path = ?4 AND (rating IS NOT ?1 OR play_count IS NOT ?2)",
                rusqlite::params![entry.rating, entry.play_count, rated_at, track_path],
            )?;
            changed = changed.saturating_add(u32::try_from(rows).unwrap_or(u32::MAX));
        }
        Ok(changed)
    }
}
