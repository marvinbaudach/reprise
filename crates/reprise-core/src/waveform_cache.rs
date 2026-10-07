//! The lazy, on-play half of rendering-data production.
//!
//! A listener who starts a track that has never been analyzed must wait for a
//! decode before the seek bar shows a real shape. That decode is the expensive
//! part; the frequency bands ride along on it for roughly a tenth of its cost
//! (measured, see `docs/research/spectrogram-pipeline.md`). So this path takes
//! both and *stores* both: the listener pays once, and the background backfill
//! never decodes that file again.
//!
//! Each call takes a cancel flag. A frontend sets it when the listener moves on
//! to another track, so a decode nobody waits for any more stops and stores
//! nothing.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use rusqlite::OptionalExtension;

use crate::db::{
    get_track_spectrogram, get_waveform_peaks, pending_segment_tracks_of, set_segment_render_data,
    set_track_render_data, track_source_fingerprint, Db, DbError, SpectrogramStoreOutcome,
};
use crate::render_data_segments::SegmentBounds;
use crate::spectrogram::TrackSourceFingerprint;
use crate::waveform::{RenderDataBackend, TrackRenderData, WaveformError, STORED_PEAK_COUNT};

/// What one decode measured for the track asked about, and how storing it went.
struct Measured {
    data: TrackRenderData,
    stored: Result<SpectrogramStoreOutcome, DbError>,
}

/// Decodes the audio of `track_id` and stores what it measured: the whole file
/// for an ordinary track, and for a track cut from a CUE file only that track's
/// stretch of it (see [`measure_cue_file`]). Storing the whole file's data
/// under a CUE track would be wrong for good, since the fingerprint it is
/// stored under is the file's and nothing would ever measure the track again.
/// A backend that cannot cut a file reports an error, and nothing is stored.
fn measure(
    db: &Db,
    track_id: i64,
    source: TrackSourceFingerprint,
    path: &Path,
    backend: &dyn RenderDataBackend,
    cancelled: &AtomicBool,
) -> Result<Measured, WaveformError> {
    let Some(bounds) = segment_bounds(db, track_id)? else {
        let data = backend.extract_render_data_cancellable(path, STORED_PEAK_COUNT, cancelled)?;
        let stored = set_track_render_data(db, track_id, source, &data);
        return Ok(Measured { data, stored });
    };
    measure_cue_file(db, track_id, bounds, source, path, backend, cancelled)
}

/// Decodes a CUE track's file once, for the track and for every other track of
/// the file that still lacks its data, and stores each: skipping through an
/// album costs one decode of its file, not one per track. Each store holds only
/// while the file and that track's cut are what they were when the decode began.
fn measure_cue_file(
    db: &Db,
    track_id: i64,
    bounds: SegmentBounds,
    source: TrackSourceFingerprint,
    path: &Path,
    backend: &dyn RenderDataBackend,
    cancelled: &AtomicBool,
) -> Result<Measured, WaveformError> {
    let siblings: Vec<(i64, SegmentBounds)> =
        match pending_segment_tracks_of(db, &path.to_string_lossy()) {
            Ok(pending) => pending
                .into_iter()
                .filter(|track| track.track_id != track_id)
                .map(|track| (track.track_id, track.bounds()))
                .collect(),
            Err(error) => {
                tracing::warn!(track_id, %error, "could not list the other tracks of the file");
                Vec::new()
            }
        };
    let wanted: Vec<SegmentBounds> = std::iter::once(bounds)
        .chain(siblings.iter().map(|(_, bounds)| *bounds))
        .collect();
    let mut results = backend
        .extract_segment_render_data_cancellable(path, &wanted, STORED_PEAK_COUNT, cancelled)?
        .into_iter();
    if results.len() != wanted.len() {
        return Err(WaveformError::DecodeFailed(
            "the backend returned the wrong number of tracks".into(),
        ));
    }
    let data = results
        .next()
        .ok_or_else(|| WaveformError::DecodeFailed("the backend returned no track".into()))??;
    for ((sibling, sibling_bounds), result) in siblings.into_iter().zip(results) {
        // A stretch the decode never reached stays pending for the backfill.
        let Ok(sibling_data) = result else {
            continue;
        };
        if let Err(error) =
            set_segment_render_data(db, sibling, source, sibling_bounds, &sibling_data)
        {
            tracing::warn!(track_id = sibling, %error, "could not store a sibling's rendering data");
        }
    }
    let stored = set_segment_render_data(db, track_id, source, bounds, &data);
    Ok(Measured { data, stored })
}

/// The stretch of its file a CUE track covers; `None` for a whole-file track.
/// A CUE track without its stretch recorded cannot be measured at all. The
/// file's last track (the highest segment index of its path) runs to the end
/// of the decoded file.
fn segment_bounds(db: &Db, track_id: i64) -> Result<Option<SegmentBounds>, WaveformError> {
    let row: Option<(Option<i64>, Option<i64>, bool)> = db
        .conn()
        .query_row(
            "SELECT segment_start_ms, segment_end_ms, \
                    segment_index = (SELECT MAX(u.segment_index) FROM tracks u \
                                     WHERE u.path = t.path) \
             FROM tracks t WHERE id = ?1 AND segment_index > 0",
            [track_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    match row {
        None => Ok(None),
        Some((Some(start_ms), Some(end_ms), last_in_file)) => Ok(Some(SegmentBounds {
            start_ms,
            end_ms,
            last_in_file,
        })),
        Some(_) => Err(WaveformError::DecodeFailed(
            "the track's stretch of its file is not recorded".into(),
        )),
    }
}

/// Returns the track's waveform peaks, decoding once if nothing is stored yet.
///
/// A cached waveform is returned untouched — a missing spectrogram is the
/// backfill's job, not a reason to make a listener wait. Returns `None` when
/// the track is unknown, its audio cannot be decoded, or `cancelled` was set.
pub fn peaks_for_playback(
    db: &Db,
    track_id: i64,
    path: &Path,
    backend: &dyn RenderDataBackend,
    cancelled: &AtomicBool,
) -> Option<Vec<u8>> {
    match get_waveform_peaks(db, track_id) {
        Ok(Some(peaks)) => return Some(peaks),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(track_id, %error, "could not read stored waveform peaks");
            return None;
        }
    }
    let source = match track_source_fingerprint(db, track_id) {
        Ok(Some(source)) => source,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(track_id, %error, "could not read the track's source identity");
            return None;
        }
    };
    let measured = match measure(db, track_id, source, path, backend, cancelled) {
        Ok(measured) => measured,
        Err(error) => {
            tracing::warn!(track_id, %error, "on-demand waveform extraction failed");
            return None;
        }
    };
    // A `SourceChanged` outcome means the file moved, or the track was re-cut,
    // under us mid-decode. The peaks still describe what is playing, so they go
    // to the player either way; only storing them would be wrong.
    if let Err(error) = measured.stored {
        tracing::warn!(track_id, %error, "could not store on-demand rendering data");
    }
    Some(measured.data.waveform_peaks)
}

/// The seek bar's colour curve for a track, with one value per stored peak.
///
/// Derived from the spectrogram the decode above already produced and stored —
/// there is no second analysis and no second column. Returns `None` while a
/// track has no stored spectrogram yet (the backfill has not reached it, or a
/// rescan moved the file mid-decode); the bar then draws in the plain accent.
pub fn centroid_for_playback(db: &Db, track_id: i64, buckets: usize) -> Option<Vec<u8>> {
    match get_track_spectrogram(db, track_id) {
        Ok(Some(spectrogram)) => Some(spectrogram.centroid_curve(buckets)),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(track_id, %error, "could not read the stored spectrogram");
            None
        }
    }
}

/// Produces and stores the track's spectrogram when only its peaks are cached,
/// then returns the colour curve.
///
/// Tracks whose peaks were stored before there was a spectrogram column keep
/// their peaks and have no bands, so `peaks_for_playback` returns early and
/// never fills that gap. The background backfill closes it eventually; this
/// closes it now, for the one track a listener is actually hearing.
///
/// Costs a full decode, so callers must not block a listener on it: the peaks
/// are already on screen by then and the colour is applied when it arrives.
/// Returns `None` if the track is unknown, already has a curve, cannot be
/// decoded, or `cancelled` was set.
pub fn ensure_centroid_for_playback(
    db: &Db,
    track_id: i64,
    path: &Path,
    buckets: usize,
    backend: &dyn RenderDataBackend,
    cancelled: &AtomicBool,
) -> Option<Vec<u8>> {
    match get_track_spectrogram(db, track_id) {
        Ok(Some(_)) => return None,
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(track_id, %error, "could not read the stored spectrogram");
            return None;
        }
    }
    let source = match track_source_fingerprint(db, track_id) {
        Ok(Some(source)) => source,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(track_id, %error, "could not read the track's source identity");
            return None;
        }
    };
    let measured = match measure(db, track_id, source, path, backend, cancelled) {
        Ok(measured) => measured,
        Err(error) => {
            tracing::warn!(track_id, %error, "on-demand spectrogram extraction failed");
            return None;
        }
    };
    if let Err(error) = measured.stored {
        tracing::warn!(track_id, %error, "could not store the on-demand spectrogram");
        return None;
    }
    Some(measured.data.spectrogram.centroid_curve(buckets))
}

#[cfg(test)]
#[path = "waveform_cache_tests.rs"]
mod tests;
