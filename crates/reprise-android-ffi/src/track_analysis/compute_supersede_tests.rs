//! A track change stops the outgoing track's foreground decode: its waiters
//! settle on `Superseded` at once, nothing is stored, and the backfill is
//! never touched.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use reprise_core::db::Db;
use reprise_core::library::scanner::scan_folder;
use reprise_core::queries::{query_library_text_search, WindowRange};

use crate::track_analysis::AndroidAnalysisOutcome;
use crate::MusicLibrary;

use super::progress_tests::{
    blocking_decoder, expected_frames, foreground_import, gate, pcm_frames, Gate,
};
use super::tests::{
    library_with_one_track, set_flag, succeeding_decoder, wait_flag, wait_for_in_flight_waiter,
    ClosureDecoder,
};
use super::CurrentDecodeSlot;

/// Two scanned copies of the sine fixture, `a` and `b`, and their ids.
pub(super) fn library_with_two_tracks() -> (tempfile::TempDir, Arc<MusicLibrary>, i64, i64) {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(&fixture, music.join("a.flac")).unwrap();
    std::fs::copy(&fixture, music.join("b.flac")).unwrap();
    let database_path = directory.path().join("reprise.db");
    let db = Db::open_migrated(Some(&database_path)).unwrap();
    scan_folder(&db, &music).unwrap();
    let rows = query_library_text_search(
        &db,
        "",
        WindowRange {
            offset: 0,
            limit: 2,
        },
    )
    .unwrap()
    .rows;
    let id_of = |title: &str| rows.iter().find(|row| row.title == title).unwrap().id;
    let (a, b) = (id_of("a"), id_of("b"));
    drop(db);
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    (directory, Arc::new(library), a, b)
}

/// Decodes of `a.flac` and `b.flac` each push a few frames, raise their own
/// `pushed` gate, and then wait on the shared `release` gate before pushing
/// the rest.
fn two_blocking_decoders(
    calls: Arc<AtomicUsize>,
    a_pushed: Gate,
    b_pushed: Gate,
    release: Gate,
) -> ClosureDecoder<
    impl Fn(
            &str,
            &Arc<crate::track_analysis::AnalysisPcmSink>,
        ) -> Result<(), crate::track_analysis::AnalysisDecodeError>
        + Send
        + Sync,
> {
    ClosureDecoder::new(calls, move |uri, sink| {
        let pushed = if uri.ends_with("a.flac") {
            &a_pushed
        } else {
            &b_pushed
        };
        assert!(sink.push_pcm_i16(pcm_frames(8), 32_000, 1));
        set_flag(pushed);
        wait_flag(&release);
        let _ = sink.push_pcm_i16(pcm_frames(16), 32_000, 1);
        Ok(())
    })
}

fn has_render_data(library: &MusicLibrary, track_id: i64) -> bool {
    let reader = library.reader().unwrap();
    reprise_core::db::get_waveform_peaks(&reader, track_id)
        .unwrap()
        .is_some()
        && reprise_core::db::get_track_spectrogram(&reader, track_id)
            .unwrap()
            .is_some()
}

#[test]
fn nav_15e_superseding_keeps_the_playing_track() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let (a_pushed, b_pushed, release) = (gate(), gate(), gate());
    library.register_track_pcm_decoder(Box::new(two_blocking_decoders(
        Arc::new(AtomicUsize::new(0)),
        Arc::clone(&a_pushed),
        Arc::clone(&b_pushed),
        Arc::clone(&release),
    )));
    let import_a = foreground_import(&library, a);
    let import_b = foreground_import(&library, b);
    wait_flag(&a_pushed);
    wait_flag(&b_pushed);

    library.supersede_foreground_track_analysis(Some(b));
    set_flag(&release);

    assert_eq!(
        import_a.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Superseded
    );
    assert_eq!(
        import_b.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed
    );
    assert!(
        !has_render_data(&library, a),
        "a superseded decode stores nothing"
    );
    assert!(has_render_data(&library, b));
    let reader = library.reader().unwrap();
    assert!(
        reprise_core::db::pending_render_data_tracks(&reader)
            .unwrap()
            .iter()
            .any(|track| track.track_id == a),
        "the superseded track stays pending for the backfill"
    );
}

#[test]
fn nav_15e_a_superseded_decode_is_final_for_its_waiter() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let calls = Arc::new(AtomicUsize::new(0));
    let total = expected_frames(&library, track_id);
    let (pushed, release) = (gate(), gate());
    let decoder = blocking_decoder(
        total / 2,
        total - total / 2,
        Arc::clone(&pushed),
        Arc::clone(&release),
    );
    // Count the decoder's runs: the supersede must not trigger a retry.
    library.register_track_pcm_decoder(Box::new(CountingDecoder {
        calls: Arc::clone(&calls),
        inner: decoder,
    }));
    let first = foreground_import(&library, track_id);
    wait_flag(&pushed);
    let second = foreground_import(&library, track_id);
    wait_for_in_flight_waiter(&library, track_id);

    library.supersede_foreground_track_analysis(Some(track_id + 1));
    set_flag(&release);

    assert_eq!(
        first.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Superseded
    );
    assert_eq!(
        second.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Superseded
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the waiter must not re-decode"
    );
    assert!(!has_render_data(&library, track_id));
}

#[test]
fn nav_15e_superseding_never_cancels_the_backfill() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let total = expected_frames(&library, track_id);
    let (pushed, release) = (gate(), gate());
    library.register_track_pcm_decoder(Box::new(blocking_decoder(
        total / 2,
        total - total / 2,
        Arc::clone(&pushed),
        Arc::clone(&release),
    )));
    let slot: Arc<CurrentDecodeSlot> = Arc::new(Mutex::new(None));
    let library_in_thread = Arc::clone(&library);
    let slot_in_thread = Arc::clone(&slot);
    let backfill = std::thread::spawn(move || {
        library_in_thread
            .analysis_context()
            .compute(track_id, true, None, Some(&slot_in_thread))
    });
    wait_flag(&pushed);

    library.supersede_foreground_track_analysis(None);
    set_flag(&release);

    assert_eq!(
        backfill.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed
    );
    assert!(has_render_data(&library, track_id));
}

#[test]
fn supersede_with_no_decodes_is_a_no_op() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    library.supersede_foreground_track_analysis(None);
    library.supersede_foreground_track_analysis(Some(track_id));
    library.register_track_pcm_decoder(Box::new(succeeding_decoder(Arc::new(AtomicUsize::new(0)))));

    assert_eq!(
        library.import_track_analysis(track_id).unwrap(),
        AndroidAnalysisOutcome::Computed,
        "an earlier supersede must not poison a later decode"
    );
}

/// Counts the runs of the decoder it wraps.
struct CountingDecoder<D> {
    calls: Arc<AtomicUsize>,
    inner: D,
}

impl<D: crate::track_analysis::TrackPcmDecoder> crate::track_analysis::TrackPcmDecoder
    for CountingDecoder<D>
{
    fn decode(
        &self,
        track_uri: String,
        sink: Arc<crate::track_analysis::AnalysisPcmSink>,
        background: bool,
    ) -> Result<(), crate::track_analysis::AnalysisDecodeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.decode(track_uri, sink, background)
    }
}
