//! The lazy, on-play half of rendering-data production.
//!
//! A listener who starts a track that has never been analyzed must wait for a
//! decode before the seek bar shows a real shape. That decode is the expensive
//! part; the frequency bands ride along on it for roughly a tenth of its cost
//! (measured, see `docs/research/spectrogram-pipeline.md`). So this path takes
//! both and *stores* both: the listener pays once, and the background backfill
//! never decodes that file again.

use std::path::Path;

use crate::db::{
    get_track_spectrogram, get_waveform_peaks, set_track_render_data, track_source_fingerprint, Db,
};
use crate::waveform::{RenderDataBackend, STORED_PEAK_COUNT};

/// Returns the track's waveform peaks, decoding once if nothing is stored yet.
///
/// A cached waveform is returned untouched — a missing spectrogram is the
/// backfill's job, not a reason to make a listener wait. Returns `None` when
/// the track is unknown or its audio cannot be decoded.
pub fn peaks_for_playback(
    db: &Db,
    track_id: i64,
    path: &Path,
    backend: &dyn RenderDataBackend,
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
    let data = match backend.extract_render_data(path, STORED_PEAK_COUNT) {
        Ok(data) => data,
        Err(error) => {
            tracing::warn!(track_id, %error, "on-demand waveform extraction failed");
            return None;
        }
    };
    // A `SourceChanged` outcome means the file moved under us mid-decode. The
    // peaks still describe what is playing, so they go to the player either
    // way; only storing them would be wrong.
    if let Err(error) = set_track_render_data(db, track_id, source, &data) {
        tracing::warn!(track_id, %error, "could not store on-demand rendering data");
    }
    Some(data.waveform_peaks)
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
/// Returns `None` if the track is unknown, already has a curve, or cannot be
/// decoded.
pub fn ensure_centroid_for_playback(
    db: &Db,
    track_id: i64,
    path: &Path,
    buckets: usize,
    backend: &dyn RenderDataBackend,
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
    let data = match backend.extract_render_data(path, STORED_PEAK_COUNT) {
        Ok(data) => data,
        Err(error) => {
            tracing::warn!(track_id, %error, "on-demand spectrogram extraction failed");
            return None;
        }
    };
    if let Err(error) = set_track_render_data(db, track_id, source, &data) {
        tracing::warn!(track_id, %error, "could not store the on-demand spectrogram");
        return None;
    }
    Some(data.spectrogram.centroid_curve(buckets))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::db::pending_render_data_tracks;
    use crate::spectrogram::TrackSpectrogram;
    use crate::waveform::{TrackRenderData, WaveformBackend, WaveformError};

    #[derive(Default)]
    struct CountingBackend {
        decodes: AtomicUsize,
    }

    impl WaveformBackend for CountingBackend {
        fn extract_peaks(&self, _path: &Path, _buckets: usize) -> Result<Vec<u8>, WaveformError> {
            panic!("the on-play path must not ask for peaks alone and drop the bands");
        }
    }

    impl RenderDataBackend for CountingBackend {
        fn extract_render_data(
            &self,
            _path: &Path,
            buckets: usize,
        ) -> Result<TrackRenderData, WaveformError> {
            self.decodes.fetch_add(1, Ordering::Relaxed);
            Ok(TrackRenderData {
                waveform_peaks: vec![7; buckets],
                spectrogram: TrackSpectrogram::from_cells(vec![9; 48]).unwrap(),
            })
        }
    }

    struct FailingBackend;

    impl WaveformBackend for FailingBackend {
        fn extract_peaks(&self, _path: &Path, _buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Err(WaveformError::EmptyStream)
        }
    }

    impl RenderDataBackend for FailingBackend {
        fn extract_render_data(
            &self,
            _path: &Path,
            _buckets: usize,
        ) -> Result<TrackRenderData, WaveformError> {
            Err(WaveformError::DecodeFailed("no decoder".into()))
        }
    }

    fn database() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, added_at, file_mtime, file_size, device, inode) \
                 VALUES (1, '/played.flac', '', 0, 11, 22, 33, 44)",
                [],
            )
            .unwrap();
        db
    }

    #[test]
    fn the_first_play_decodes_once_and_nothing_decodes_it_again() {
        let db = database();
        let backend = CountingBackend::default();

        let first = peaks_for_playback(&db, 1, Path::new("/played.flac"), &backend);
        let second = peaks_for_playback(&db, 1, Path::new("/played.flac"), &backend);

        assert_eq!(first, Some(vec![7; STORED_PEAK_COUNT]));
        assert_eq!(second, first);
        assert_eq!(
            backend.decodes.load(Ordering::Relaxed),
            1,
            "a second play must read the stored peaks, not decode again"
        );
        assert!(
            pending_render_data_tracks(&db).unwrap().is_empty(),
            "the decode the listener paid for must leave nothing for the backfill to redo"
        );
    }

    #[test]
    fn a_track_whose_source_moved_mid_decode_still_plays_without_storing() {
        // Two handles onto one file database: the backend can move the file's
        // identity while the "decode" is in flight, which is what a rescan
        // does to a track someone just started playing.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("moved.db");
        let db = Db::open_migrated(Some(&path)).unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, added_at, file_mtime, file_size, device, inode) \
                 VALUES (1, '/played.flac', '', 0, 11, 22, 33, 44)",
                [],
            )
            .unwrap();
        struct MovingBackend {
            rescanner: std::sync::Mutex<Db>,
            inner: CountingBackend,
        }
        impl WaveformBackend for MovingBackend {
            fn extract_peaks(
                &self,
                _path: &Path,
                _buckets: usize,
            ) -> Result<Vec<u8>, WaveformError> {
                unreachable!("the on-play path asks for render data")
            }
        }
        impl RenderDataBackend for MovingBackend {
            fn extract_render_data(
                &self,
                path: &Path,
                buckets: usize,
            ) -> Result<TrackRenderData, WaveformError> {
                self.rescanner
                    .lock()
                    .unwrap()
                    .conn()
                    .execute("UPDATE tracks SET file_size = 999 WHERE id = 1", [])
                    .unwrap();
                self.inner.extract_render_data(path, buckets)
            }
        }
        let moving = MovingBackend {
            rescanner: std::sync::Mutex::new(Db::open_migrated(Some(&path)).unwrap()),
            inner: CountingBackend::default(),
        };

        let peaks = peaks_for_playback(&db, 1, Path::new("/played.flac"), &moving);

        assert_eq!(peaks, Some(vec![7; STORED_PEAK_COUNT]));
        assert_eq!(
            get_waveform_peaks(&db, 1).unwrap(),
            None,
            "peaks measured from a file that has since changed must not be stored"
        );
    }

    #[test]
    fn an_undecodable_track_yields_no_peaks_and_stores_nothing() {
        let db = database();

        let peaks = peaks_for_playback(&db, 1, Path::new("/played.flac"), &FailingBackend);

        assert_eq!(peaks, None);
        assert_eq!(get_waveform_peaks(&db, 1).unwrap(), None);
        assert_eq!(pending_render_data_tracks(&db).unwrap().len(), 1);
    }

    #[test]
    fn an_unknown_track_is_never_decoded() {
        let db = database();
        let backend = CountingBackend::default();

        let peaks = peaks_for_playback(&db, 404, Path::new("/missing.flac"), &backend);

        assert_eq!(peaks, None);
        assert_eq!(backend.decodes.load(Ordering::Relaxed), 0);
    }

    /// The gap this closes: peaks stored before the spectrogram column existed
    /// make `peaks_for_playback` return early forever, so those tracks never
    /// gain a colour curve from the on-play path alone.
    #[test]
    fn a_track_with_only_cached_peaks_gains_its_curve_on_play() {
        let db = database();
        let backend = CountingBackend::default();
        crate::db::set_waveform_peaks(&db, 1, &vec![3; STORED_PEAK_COUNT]).unwrap();
        assert_eq!(centroid_for_playback(&db, 1, STORED_PEAK_COUNT), None);

        let curve =
            ensure_centroid_for_playback(&db, 1, Path::new("/played.flac"), 16, &backend).unwrap();

        assert_eq!(curve.len(), 16);
        assert_eq!(backend.decodes.load(Ordering::Relaxed), 1);
        assert!(centroid_for_playback(&db, 1, STORED_PEAK_COUNT).is_some());
    }

    #[test]
    fn a_track_that_already_has_a_curve_is_never_decoded_again() {
        let db = database();
        let backend = CountingBackend::default();
        peaks_for_playback(&db, 1, Path::new("/played.flac"), &backend);
        let decodes_after_first_play = backend.decodes.load(Ordering::Relaxed);

        let curve = ensure_centroid_for_playback(&db, 1, Path::new("/played.flac"), 16, &backend);

        assert_eq!(curve, None, "nothing to redo, so nothing is handed back");
        assert_eq!(
            backend.decodes.load(Ordering::Relaxed),
            decodes_after_first_play
        );
    }

    #[test]
    fn an_undecodable_track_gains_no_curve_and_stores_nothing() {
        let db = database();
        crate::db::set_waveform_peaks(&db, 1, &vec![3; STORED_PEAK_COUNT]).unwrap();

        let curve =
            ensure_centroid_for_playback(&db, 1, Path::new("/played.flac"), 16, &FailingBackend);

        assert_eq!(curve, None);
        assert_eq!(centroid_for_playback(&db, 1, STORED_PEAK_COUNT), None);
    }
}
