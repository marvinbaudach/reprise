use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use reprise_core::db::Db;
use reprise_core::device_sync::analysis_sidecar::AnalysisSidecar;
use reprise_core::library::scanner::scan_folder;
use reprise_core::queries::{query_library_text_search, WindowRange};
use reprise_core::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};

use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use crate::track_analysis::{
    AnalysisDecodeError, AnalysisPcmSink, AndroidAnalysisOutcome, TrackPcmDecoder,
};
use crate::MusicLibrary;

struct InertSource;

impl SafSource for InertSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
        Ok(None)
    }

    fn probe(
        &self,
        _uri: String,
        _follow_links: bool,
    ) -> Result<Option<SourceFacts>, SafSourceError> {
        Ok(None)
    }

    fn list_children(&self, _uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        Ok(Vec::new())
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
        Err(SafSourceError::NotFound {
            detail: format!("inert source has no documents ({uri})"),
        })
    }
}

/// A one-track library with no SAF sync history at all: `analysis_sidecar_
/// path_for_track` naturally answers `None` for it, matching the "no sidecar
/// has ever been registered" state every phone track starts in.
fn library_with_one_track() -> (tempfile::TempDir, MusicLibrary, i64, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(&fixture, music.join("song.flac")).unwrap();
    let database_path = directory.path().join("reprise.db");
    let db = Db::open_migrated(Some(&database_path)).unwrap();
    scan_folder(&db, &music).unwrap();
    let track_id = query_library_text_search(
        &db,
        "",
        WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows
    .remove(0)
    .id;
    drop(db);
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    library
        .set_tree_uri("content://inert".into(), Box::new(InertSource))
        .unwrap();
    (directory, library, track_id, music)
}

/// One channel of 16-bit PCM long enough for `RenderDataSession::finish` to
/// succeed, as little-endian bytes ready for `push_pcm_i16`.
fn valid_pcm_bytes() -> Vec<u8> {
    let sample_rate_hz = 32_000_u32;
    let mut bytes = Vec::new();
    for index in 0..sample_rate_hz {
        let phase = std::f64::consts::TAU * 440.0 * f64::from(index) / f64::from(sample_rate_hz);
        let sample = (phase.sin() * 0.5 * f64::from(i16::MAX)) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn push_valid_pcm(sink: &Arc<AnalysisPcmSink>) {
    assert!(sink.push_pcm_i16(valid_pcm_bytes(), 32_000, 1));
}

/// Waits for a test-controlled gate to open. Bounded: an un-timed-out wait on
/// a background decode is a hang waiting to happen the day the decoder never
/// reaches the gate — a bug here must fail this test, not the whole suite.
fn wait_flag(state: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, condvar) = &**state;
    let guard = lock.lock().unwrap();
    let (_guard, timed_out) = condvar
        .wait_timeout_while(guard, Duration::from_secs(10), |set| !*set)
        .unwrap();
    assert!(!timed_out.timed_out(), "the gate was never opened");
}

/// A [`TrackPcmDecoder`] whose body is an arbitrary closure, counting calls.
struct ClosureDecoder<F> {
    calls: Arc<AtomicUsize>,
    run: F,
}

impl<F> ClosureDecoder<F>
where
    F: Fn(&str, &Arc<AnalysisPcmSink>) -> Result<(), AnalysisDecodeError> + Send + Sync,
{
    fn new(calls: Arc<AtomicUsize>, run: F) -> Self {
        Self { calls, run }
    }
}

impl<F> TrackPcmDecoder for ClosureDecoder<F>
where
    F: Fn(&str, &Arc<AnalysisPcmSink>) -> Result<(), AnalysisDecodeError> + Send + Sync,
{
    fn decode(
        &self,
        track_uri: String,
        sink: Arc<AnalysisPcmSink>,
        _background: bool,
    ) -> Result<(), AnalysisDecodeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.run)(&track_uri, &sink)
    }
}

fn succeeding_decoder(
    calls: Arc<AtomicUsize>,
) -> ClosureDecoder<
    impl Fn(&str, &Arc<AnalysisPcmSink>) -> Result<(), AnalysisDecodeError> + Send + Sync,
> {
    ClosureDecoder::new(calls, |_uri, sink| {
        push_valid_pcm(sink);
        Ok(())
    })
}

struct NeverCalledDecoder;

impl TrackPcmDecoder for NeverCalledDecoder {
    fn decode(
        &self,
        _track_uri: String,
        _sink: Arc<AnalysisPcmSink>,
        _background: bool,
    ) -> Result<(), AnalysisDecodeError> {
        panic!("the decoder must not run when a sidecar import already answered");
    }
}

const SIDECAR_ROOT: &str = "content://provider/tree/music";
const SIDECAR_TRACK: &str = "content://provider/document/audio-1.flac";
const SIDECAR_SIDECAR: &str = "content://provider/document/analysis-1.reprise-analysis";

struct SyncedTrackSource {
    audio: PathBuf,
    sidecar: PathBuf,
}

impl SafSource for SyncedTrackSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
        Ok(Some(1))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<SourceFacts>, SafSourceError> {
        assert_eq!(uri, SIDECAR_ROOT);
        Ok(Some(SourceFacts {
            display_name: Some("Music".into()),
            is_file: false,
            is_directory: true,
            size_bytes: None,
            modified_unix_ms: None,
            document_id: "opaque-root".into(),
        }))
    }

    fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        assert_eq!(uri, SIDECAR_ROOT);
        Ok(vec![
            SourceChild {
                uri: SIDECAR_TRACK.into(),
                display_name: Some("Artist - Song.flac".into()),
                is_file: true,
                is_directory: false,
                size_bytes: Some(std::fs::metadata(&self.audio).unwrap().len()),
                modified_unix_ms: Some(1_775_000_000_000),
                document_id: "opaque-track".into(),
            },
            SourceChild {
                uri: SIDECAR_SIDECAR.into(),
                display_name: Some("Artist - Song.reprise-analysis".into()),
                is_file: true,
                is_directory: false,
                size_bytes: Some(std::fs::metadata(&self.sidecar).unwrap().len()),
                modified_unix_ms: Some(1_775_000_000_000),
                document_id: "opaque-sidecar".into(),
            },
        ])
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
        let path = match uri.as_str() {
            SIDECAR_TRACK => &self.audio,
            SIDECAR_SIDECAR => &self.sidecar,
            _ => {
                return Err(SafSourceError::Io {
                    detail: format!("unexpected document {uri}"),
                })
            }
        };
        File::open(path)
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

struct QuietProgress;

impl crate::ScanProgressListener for QuietProgress {
    fn on_progress(&self, _progress: crate::ScanProgressUpdate) {}
}

fn library_with_a_synced_sidecar() -> (tempfile::TempDir, MusicLibrary, i64) {
    let directory = tempfile::tempdir().unwrap();
    let audio = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    let sidecar_path = directory.path().join("sidecar.bin");
    let sidecar = AnalysisSidecar::new(
        TrackSourceFingerprint {
            mtime_seconds: 1,
            size_bytes: 2,
            device: Some(3),
            inode: Some(4),
        },
        TrackSpectrogram::from_cells(vec![17; 48]).unwrap(),
        vec![19, 23],
    )
    .encode()
    .unwrap();
    std::fs::write(&sidecar_path, sidecar).unwrap();
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    library
        .set_tree_uri(
            SIDECAR_ROOT.into(),
            Box::new(SyncedTrackSource {
                audio,
                sidecar: sidecar_path,
            }),
        )
        .unwrap();
    library.scan(Box::new(QuietProgress)).unwrap();
    let track_id = library
        .list_tracks(crate::WindowRange {
            offset: 0,
            limit: 1,
        })
        .unwrap()
        .rows
        .remove(0)
        .id;
    (directory, library, track_id)
}

#[test]
fn a_missing_sidecar_is_computed_and_stored() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    library.register_track_pcm_decoder(Box::new(succeeding_decoder(Arc::new(AtomicUsize::new(0)))));

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::Computed);
    assert!(library.track_render_bars(track_id, 8).unwrap().is_some());
    let reader = library.reader().unwrap();
    let pending = reprise_core::db::pending_render_data_tracks(&reader).unwrap();
    assert!(!pending.iter().any(|pending| pending.track_id == track_id));
}

/// A decoder registration that only proves how long it lives: its `Drop`
/// flips [`dropped`](Self::dropped) so the test can tell whether the last
/// strong reference — the library's own `pcm_decoder` field — was ever
/// released.
struct DropSignalDecoder {
    dropped: Arc<AtomicBool>,
}

impl TrackPcmDecoder for DropSignalDecoder {
    fn decode(
        &self,
        _track_uri: String,
        _sink: Arc<AnalysisPcmSink>,
        _background: bool,
    ) -> Result<(), AnalysisDecodeError> {
        panic!("this decoder only proves its own lifetime; it must never be called");
    }
}

impl Drop for DropSignalDecoder {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

/// Regression test for the Android suite's `OutOfMemoryError`: the platform
/// decoder Kotlin registers (`SharedMusicLibrary.kt`) is kept alive by a
/// UniFFI foreign-callback handle for exactly as long as `pcm_decoder` holds
/// it. If anything gave that registration a lifetime independent of the
/// library — a process-wide static, or a second clone stashed outside this
/// field — dropping the library would no longer be enough to release it,
/// which is exactly the shape of leak that pinned a whole Kotlin
/// `Application` graph (and its native library) across every Robolectric
/// test that touched `sharedMusicLibrary()`.
#[test]
fn dropping_the_library_releases_the_registered_decoder() {
    let (_directory, library, _track_id, _music) = library_with_one_track();
    let dropped = Arc::new(AtomicBool::new(false));
    library.register_track_pcm_decoder(Box::new(DropSignalDecoder {
        dropped: Arc::clone(&dropped),
    }));

    drop(library);

    assert!(
        dropped.load(Ordering::SeqCst),
        "the registered decoder must not outlive the library that registered it",
    );
}

#[test]
fn a_present_sidecar_wins_and_the_decoder_is_never_called() {
    let (_directory, library, track_id) = library_with_a_synced_sidecar();
    library.register_track_pcm_decoder(Box::new(NeverCalledDecoder));

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::Imported);
}

#[test]
fn a_computed_analysis_makes_a_later_sidecar_already_imported() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let calls = Arc::new(AtomicUsize::new(0));
    library.register_track_pcm_decoder(Box::new(succeeding_decoder(Arc::clone(&calls))));

    let first = library.import_track_analysis(track_id).unwrap();
    let second = library.import_track_analysis(track_id).unwrap();

    assert_eq!(first, AndroidAnalysisOutcome::Computed);
    assert_eq!(second, AndroidAnalysisOutcome::AlreadyImported);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the decoder must not run again"
    );
}

#[test]
fn the_writer_is_free_while_the_decoder_runs() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let writer = library.writer_handle();
    let checked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer_was_free = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let checked_in_decode = Arc::clone(&checked);
    let writer_was_free_in_decode = Arc::clone(&writer_was_free);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            checked_in_decode.store(true, Ordering::SeqCst);
            writer_was_free_in_decode.store(writer.try_lock().is_ok(), Ordering::SeqCst);
            push_valid_pcm(sink);
            Ok(())
        },
    )));

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::Computed);
    assert!(checked.load(Ordering::SeqCst), "the decoder never ran");
    assert!(
        writer_was_free.load(Ordering::SeqCst),
        "the writer must not be held while the decoder runs"
    );
}

#[test]
fn a_decoder_failure_is_reported_and_stores_nothing() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        |_uri, _sink| {
            Err(AnalysisDecodeError::DecodeFailed {
                detail: "boom".into(),
            })
        },
    )));

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::DecodeFailed);
    let reader = library.reader().unwrap();
    assert!(reprise_core::db::get_track_spectrogram(&reader, track_id)
        .unwrap()
        .is_none());
    assert!(reprise_core::db::get_waveform_peaks(&reader, track_id)
        .unwrap()
        .is_none());
}

/// A decoder reporting `sample_rate_hz == 0` (e.g. Kotlin's `MediaCodec`
/// when a format lacks `KEY_SAMPLE_RATE`) used to spin `RenderDataSession`'s
/// resampler forever instead of erroring. The session now refuses the chunk,
/// which `AnalysisPcmSink::push_pcm_i16` surfaces as `false` and
/// `AnalysisContext::decode_one` reports through the same `refused` path a
/// mid-stream rate change already used — this pins the whole flow, not just
/// the session-level rejection.
#[test]
fn a_zero_sample_rate_chunk_is_refused_and_reported_as_decode_failed() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        |_uri, sink| {
            assert!(!sink.push_pcm_i16(valid_pcm_bytes(), 0, 1));
            Ok(())
        },
    )));

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::DecodeFailed);
    let reader = library.reader().unwrap();
    assert!(reprise_core::db::get_track_spectrogram(&reader, track_id)
        .unwrap()
        .is_none());
}

#[test]
fn a_file_replaced_during_the_decode_is_not_stored() {
    let (_directory, library, track_id, music) = library_with_one_track();
    let writer = library.writer_handle();
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            let song_path = music.join("song.flac");
            let file = File::open(&song_path).unwrap();
            file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120))
                .unwrap();
            drop(file);
            {
                let db = writer.lock().unwrap();
                scan_folder(&db, &music).unwrap();
            }
            push_valid_pcm(sink);
            Ok(())
        },
    )));

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::PhoneSourceChanged);
    let reader = library.reader().unwrap();
    assert!(reprise_core::db::get_track_spectrogram(&reader, track_id)
        .unwrap()
        .is_none());
}

#[test]
fn no_registered_decoder_reports_no_decoder() {
    let (_directory, library, track_id, _music) = library_with_one_track();

    let outcome = library.import_track_analysis(track_id).unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::NoDecoder);
}

#[test]
fn two_callers_for_one_track_decode_once() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let calls = Arc::new(AtomicUsize::new(0));
    let decoding: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let release: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let decoding_in_decode = Arc::clone(&decoding);
    let release_in_decode = Arc::clone(&release);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::clone(&calls),
        move |_uri, sink| {
            {
                let (lock, condvar) = &*decoding_in_decode;
                *lock.lock().unwrap() = true;
                condvar.notify_all();
            }
            {
                let (lock, condvar) = &*release_in_decode;
                let mut guard = lock.lock().unwrap();
                while !*guard {
                    guard = condvar.wait(guard).unwrap();
                }
            }
            push_valid_pcm(sink);
            Ok(())
        },
    )));

    let library_a = Arc::clone(&library);
    let handle_a = std::thread::spawn(move || library_a.import_track_analysis(track_id));

    wait_flag(&decoding);

    let library_b = Arc::clone(&library);
    let handle_b = std::thread::spawn(move || library_b.import_track_analysis(track_id));

    {
        let (lock, condvar) = &*release;
        *lock.lock().unwrap() = true;
        condvar.notify_all();
    }

    let outcome_a = handle_a.join().unwrap().unwrap();
    let outcome_b = handle_b.join().unwrap().unwrap();

    assert_eq!(outcome_a, AndroidAnalysisOutcome::Computed);
    assert_eq!(outcome_b, AndroidAnalysisOutcome::Computed);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "only one decode must run");
}
