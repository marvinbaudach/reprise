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

/// Resolves the currently registered sidecar path without opening it.
///
/// Android uses this as the short database half of a two-phase import so its
/// provider read can happen without the app-wide library lock held.
pub fn analysis_sidecar_path_for_track(db: &Db, track_id: i64) -> Result<Option<String>, DbError> {
    Ok(crate::db_mobile_sync::analysis_sidecar_state(db, track_id)?.map(|state| state.path))
}

/// Reads one sidecar through its owning library source.
///
/// Missing and failed reads are ordinary no-data answers, matching playback's
/// plain-bar fallback.
pub fn read_analysis_sidecar(
    source: &dyn LibrarySource,
    track_id: i64,
    sidecar_path: &str,
) -> Option<Vec<u8>> {
    let mut reader = match source.open_read(Path::new(sidecar_path)) {
        Ok(reader) => reader,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(track_id, sidecar = sidecar_path, %error, "could not read analysis sidecar");
            }
            return None;
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = reader.read_to_end(&mut bytes) {
        tracing::warn!(track_id, sidecar = sidecar_path, %error, "could not read analysis sidecar");
        return None;
    }
    Some(bytes)
}

/// Imports one discovered sidecar without making its absence or corruption a
/// playback failure. Database failures remain explicit to Rust callers.
pub fn import_analysis_for_track(
    source: &dyn LibrarySource,
    db: &Db,
    track_id: i64,
) -> Result<AnalysisImportOutcome, DbError> {
    let Some(sidecar_path) = analysis_sidecar_path_for_track(db, track_id)? else {
        return Ok(AnalysisImportOutcome::Missing);
    };
    let Some(bytes) = read_analysis_sidecar(source, track_id, &sidecar_path) else {
        return Ok(AnalysisImportOutcome::Missing);
    };
    import_analysis_bytes_for_track(db, track_id, &sidecar_path, &bytes)
}

/// Validates and stores bytes read for the still-current registered path.
///
/// Rechecking the path under the database lock prevents an earlier provider
/// read from being applied after a concurrent scan registered a replacement.
pub fn import_analysis_bytes_for_track(
    db: &Db,
    track_id: i64,
    sidecar_path: &str,
    bytes: &[u8],
) -> Result<AnalysisImportOutcome, DbError> {
    let Some(state) = crate::db_mobile_sync::analysis_sidecar_state(db, track_id)? else {
        return Ok(AnalysisImportOutcome::Missing);
    };
    if state.path != sidecar_path {
        return Ok(AnalysisImportOutcome::Missing);
    }
    let sidecar = match AnalysisSidecar::decode(bytes) {
        Ok(sidecar) => sidecar,
        Err(error) => {
            tracing::warn!(track_id, sidecar = sidecar_path, %error, "could not decode analysis sidecar");
            return Ok(AnalysisImportOutcome::Invalid);
        }
    };
    if state.imported_source == Some(sidecar.source)
        && crate::db_spectrogram::get_track_spectrogram(db, track_id)?.is_some()
        && crate::db_spectrogram::get_waveform_peaks(db, track_id)?.is_some()
    {
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
    crate::db_mobile_sync::record_imported_source(db, track_id, sidecar_path, sidecar.source)?;
    Ok(AnalysisImportOutcome::Imported)
}

#[cfg(test)]
#[path = "mobile_import_tests.rs"]
mod tests;
