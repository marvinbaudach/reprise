use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::render_data_segments::SegmentBounds;

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
            loudness: None,
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
        fn extract_peaks(&self, _path: &Path, _buckets: usize) -> Result<Vec<u8>, WaveformError> {
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

/// Cuts the data it hands back from the stretch it was asked for, and refuses
/// to decode a whole file for a track that is only part of one.
#[derive(Default)]
struct CuttingBackend {
    whole_file_decodes: AtomicUsize,
    cuts: std::sync::Mutex<Vec<SegmentBounds>>,
}

impl WaveformBackend for CuttingBackend {
    fn extract_peaks(&self, _path: &Path, _buckets: usize) -> Result<Vec<u8>, WaveformError> {
        panic!("the on-play path must not ask for peaks alone");
    }
}

impl RenderDataBackend for CuttingBackend {
    fn extract_render_data(
        &self,
        _path: &Path,
        buckets: usize,
    ) -> Result<TrackRenderData, WaveformError> {
        self.whole_file_decodes.fetch_add(1, Ordering::Relaxed);
        Ok(TrackRenderData {
            waveform_peaks: vec![255; buckets],
            spectrogram: TrackSpectrogram::from_cells(vec![9; 48]).unwrap(),
            loudness: None,
        })
    }

    fn extract_segment_render_data_cancellable(
        &self,
        _path: &Path,
        segments: &[SegmentBounds],
        buckets: usize,
        _cancelled: &AtomicBool,
    ) -> Result<Vec<crate::waveform::SegmentRenderData>, WaveformError> {
        self.cuts.lock().unwrap().extend_from_slice(segments);
        Ok(segments
            .iter()
            .map(|_| {
                Ok(TrackRenderData {
                    waveform_peaks: vec![5; buckets],
                    spectrogram: TrackSpectrogram::from_cells(vec![9; 48]).unwrap(),
                    loudness: None,
                })
            })
            .collect())
    }
}

fn database_with_a_cue_track() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, device, inode, \
              segment_index, segment_start_ms, segment_end_ms) \
             VALUES (2, '/live.flac', '', 0, 11, 22, 33, 44, 2, 3000, 8000)",
            [],
        )
        .unwrap();
    db
}

#[test]
fn cue_9_playing_a_cue_track_stores_its_own_stretch_and_never_the_whole_file() {
    let db = database_with_a_cue_track();
    let backend = CuttingBackend::default();

    let peaks = peaks_for_playback(&db, 2, Path::new("/live.flac"), &backend).unwrap();

    assert_eq!(backend.whole_file_decodes.load(Ordering::Relaxed), 0);
    assert_eq!(
        backend.cuts.lock().unwrap().as_slice(),
        [SegmentBounds {
            start_ms: 3_000,
            end_ms: 8_000
        }]
    );
    assert_eq!(peaks[0], 5);
    assert_eq!(get_waveform_peaks(&db, 2).unwrap().unwrap()[0], 5);
}

#[test]
fn cue_9_a_backend_that_cannot_cut_a_file_leaves_a_cue_track_without_data() {
    let db = database_with_a_cue_track();

    let peaks = peaks_for_playback(&db, 2, Path::new("/live.flac"), &CountingBackend::default());
    let curve = ensure_centroid_for_playback(
        &db,
        2,
        Path::new("/live.flac"),
        16,
        &CountingBackend::default(),
    );

    assert_eq!(peaks, None);
    assert_eq!(curve, None);
    assert_eq!(get_waveform_peaks(&db, 2).unwrap(), None);
    assert_eq!(get_track_spectrogram(&db, 2).unwrap(), None);
    assert_eq!(
        crate::db::pending_segment_render_data_files(&db)
            .unwrap()
            .len(),
        1,
        "the backfill still owes it a measurement"
    );
}

#[test]
fn cue_9_a_cue_track_without_its_stretch_recorded_is_not_measured() {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, device, inode, segment_index) \
             VALUES (3, '/live.flac', '', 0, 11, 22, 33, 44, 2)",
            [],
        )
        .unwrap();
    let backend = CuttingBackend::default();

    let peaks = peaks_for_playback(&db, 3, Path::new("/live.flac"), &backend);

    assert_eq!(peaks, None);
    assert!(backend.cuts.lock().unwrap().is_empty(), "nothing to cut");
    assert_eq!(backend.whole_file_decodes.load(Ordering::Relaxed), 0);
    assert_eq!(get_waveform_peaks(&db, 3).unwrap(), None);
}

/// Re-cuts track 2 through its own connection while the file decodes, as a
/// rescan that applies an edited sheet does.
struct RecuttingBackend {
    rescanner: std::sync::Mutex<Db>,
    inner: CuttingBackend,
}

impl WaveformBackend for RecuttingBackend {
    fn extract_peaks(&self, _path: &Path, _buckets: usize) -> Result<Vec<u8>, WaveformError> {
        unreachable!("the on-play path asks for render data")
    }
}

impl RenderDataBackend for RecuttingBackend {
    fn extract_segment_render_data_cancellable(
        &self,
        path: &Path,
        segments: &[SegmentBounds],
        buckets: usize,
        cancelled: &AtomicBool,
    ) -> Result<Vec<crate::waveform::SegmentRenderData>, WaveformError> {
        self.rescanner
            .lock()
            .unwrap()
            .conn()
            .execute("UPDATE tracks SET segment_end_ms = 9000 WHERE id = 2", [])
            .unwrap();
        self.inner
            .extract_segment_render_data_cancellable(path, segments, buckets, cancelled)
    }
}

#[test]
fn cue_9_a_track_re_cut_while_it_decodes_keeps_nothing_from_the_old_cut() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recut.db");
    let db = Db::open_migrated(Some(&path)).unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, device, inode, \
              segment_index, segment_start_ms, segment_end_ms) \
             VALUES (2, '/live.flac', '', 0, 11, 22, 33, 44, 2, 3000, 8000)",
            [],
        )
        .unwrap();
    let backend = RecuttingBackend {
        rescanner: std::sync::Mutex::new(Db::open_migrated(Some(&path)).unwrap()),
        inner: CuttingBackend::default(),
    };

    let peaks = peaks_for_playback(&db, 2, Path::new("/live.flac"), &backend);

    assert!(peaks.is_some(), "what plays still gets a shape");
    assert_eq!(
        get_waveform_peaks(&db, 2).unwrap(),
        None,
        "data measured from the old cut must not be stored under the new one"
    );
}
