//! Phone-library actions that also enter the desktop export journal.

use crate::{LibraryError, MusicLibrary};

#[uniffi::export]
impl MusicLibrary {
    /// Persists one row's rating and refuses to report success if the row was
    /// removed after it crossed the boundary.
    pub fn set_track_rating(&self, track_id: i64, rating: i32) -> Result<(), LibraryError> {
        let state = self.lock()?;
        let device_path =
            reprise_core::device_sync::mobile_import::device_path_for_track(&state.db, track_id)
                .map_err(|error| LibraryError::Database {
                    detail: error.to_string(),
                })?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
        let rated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let changed = reprise_core::library::stats::set_rating_at_if_present(
            &state.db, track_id, rating, rated_at,
        )
        .map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        if !changed {
            return Err(LibraryError::TrackNotFound { track_id });
        }
        crate::listen_export_journal::record_rating(
            &self.database_path,
            &device_path,
            rating.clamp(0, 5),
            rated_at,
        )
        .map_err(|error| LibraryError::ListenReport {
            detail: error.to_string(),
        })?;
        Ok(())
    }

    /// Produces the complete pending `RPT-BACK` bytes after applying only a
    /// valid desktop acknowledgement. Kotlin owns the sync-tree read and write.
    // UniFFI transfers optional byte buffers by value across the ABI.
    #[allow(clippy::needless_pass_by_value)]
    pub fn prepare_listen_report(
        &self,
        acknowledgement: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, LibraryError> {
        crate::listen_export_journal::prepare_report(
            &self.database_path,
            acknowledgement.as_deref(),
        )
        .map_err(|error| LibraryError::ListenReport {
            detail: error.to_string(),
        })
    }
}
