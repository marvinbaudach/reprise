//! Lazy phone-side import of rendering data produced by the desktop.

use std::io::Read;
use std::path::Path;

use crate::db::{Db, DbError};
use crate::library::source::LibrarySource;
use crate::waveform::TrackRenderData;

use super::analysis_sidecar::AnalysisSidecar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisImportOutcome {
    Imported,
    AlreadyImported,
    Missing,
    Invalid,
    PhoneSourceChanged,
}

/// Imports one discovered sidecar without making its absence or corruption a
/// playback failure. Database failures remain explicit to Rust callers.
pub fn import_analysis_for_track(
    source: &dyn LibrarySource,
    db: &Db,
    track_id: i64,
) -> Result<AnalysisImportOutcome, DbError> {
    let Some(state) = crate::db_mobile_sync::analysis_sidecar_state(db, track_id)? else {
        return Ok(AnalysisImportOutcome::Missing);
    };
    let mut reader = match source.open_read(Path::new(&state.path)) {
        Ok(reader) => reader,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(track_id, sidecar = state.path, %error, "could not read analysis sidecar");
            }
            return Ok(AnalysisImportOutcome::Missing);
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = reader.read_to_end(&mut bytes) {
        tracing::warn!(track_id, sidecar = state.path, %error, "could not read analysis sidecar");
        return Ok(AnalysisImportOutcome::Missing);
    }
    let sidecar = match AnalysisSidecar::decode(&bytes) {
        Ok(sidecar) => sidecar,
        Err(error) => {
            tracing::warn!(track_id, sidecar = state.path, %error, "could not decode analysis sidecar");
            return Ok(AnalysisImportOutcome::Invalid);
        }
    };
    if state.imported_source == Some(sidecar.source) {
        return Ok(AnalysisImportOutcome::AlreadyImported);
    }
    let Some(phone_source) = crate::db_spectrogram::track_source_fingerprint(db, track_id)? else {
        return Ok(AnalysisImportOutcome::PhoneSourceChanged);
    };
    let data = TrackRenderData {
        waveform_peaks: sidecar.waveform_peaks,
        spectrogram: sidecar.spectrogram,
    };
    if crate::db_spectrogram::set_track_render_data(db, track_id, phone_source, &data)?
        == crate::db_spectrogram::SpectrogramStoreOutcome::SourceChanged
    {
        return Ok(AnalysisImportOutcome::PhoneSourceChanged);
    }
    crate::db_mobile_sync::record_imported_source(db, track_id, &state.path, sidecar.source)?;
    Ok(AnalysisImportOutcome::Imported)
}

#[cfg(test)]
#[path = "mobile_import_tests.rs"]
mod tests;
