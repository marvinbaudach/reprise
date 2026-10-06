//! What a running decode reports before it has stored anything: the partial
//! picture is read from the decode's own session, never from the database.

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use reprise_core::spectrogram::{SPECTROGRAM_BAND_COUNT, SPECTROGRAM_SAMPLE_RATE_HZ};

use crate::track_analysis::decodes::expected_frame_count;
use crate::track_analysis::{AndroidAnalysisOutcome, TrackPcmDecoder};
use crate::{LibraryError, MusicLibrary};

use super::tests::{
    library_with_one_track, set_flag, succeeding_decoder, wait_flag, ClosureDecoder,
};
use super::CurrentDecodeSlot;

pub(super) type Gate = Arc<(Mutex<bool>, Condvar)>;

const SAMPLES_PER_FRAME: usize = SPECTROGRAM_SAMPLE_RATE_HZ as usize / 20;
const BAR_COUNT: u32 = 100;

pub(super) fn gate() -> Gate {
    Arc::new((Mutex::new(false), Condvar::new()))
}

/// Little-endian mono PCM at the spectrogram rate, `frames` spectrogram
/// frames long.
pub(super) fn pcm_frames(frames: usize) -> Vec<u8> {
    (0..frames * SAMPLES_PER_FRAME)
        .flat_map(|index| {
            let phase = std::f64::consts::TAU * 440.0 * index as f64
                / f64::from(SPECTROGRAM_SAMPLE_RATE_HZ);
            ((phase.sin() * 0.5 * f64::from(i16::MAX)) as i16).to_le_bytes()
        })
        .collect()
}

/// A decoder that pushes `first_frames`, raises `pushed`, waits for
/// `release`, then pushes `rest_frames` (refused once the sink is stopped).
pub(super) fn blocking_decoder(
    first_frames: usize,
    rest_frames: usize,
    pushed: Gate,
    release: Gate,
) -> impl TrackPcmDecoder {
    ClosureDecoder::new(Arc::new(AtomicUsize::new(0)), move |_uri, sink| {
        assert!(sink.push_pcm_i16(pcm_frames(first_frames), SPECTROGRAM_SAMPLE_RATE_HZ, 1));
        set_flag(&pushed);
        wait_flag(&release);
        let _ = sink.push_pcm_i16(pcm_frames(rest_frames), SPECTROGRAM_SAMPLE_RATE_HZ, 1);
        Ok(())
    })
}

/// The frame count `decode_one` expects for `track_id`.
pub(super) fn expected_frames(library: &MusicLibrary, track_id: i64) -> usize {
    let reader = library.reader().unwrap();
    let track = reprise_core::queries::query_present_track_by_id(&reader, track_id)
        .unwrap()
        .unwrap();
    expected_frame_count(track.duration_ms).expect("the fixture has a duration")
}

type Outcome = Result<AndroidAnalysisOutcome, LibraryError>;

pub(super) fn foreground_import(library: &Arc<MusicLibrary>, track_id: i64) -> JoinHandle<Outcome> {
    let library = Arc::clone(library);
    std::thread::spawn(move || library.import_track_analysis(track_id))
}

fn background_compute(library: &Arc<MusicLibrary>, track_id: i64) -> JoinHandle<Outcome> {
    let library = Arc::clone(library);
    std::thread::spawn(move || {
        library
            .analysis_context()
            .compute(track_id, true, None, None)
    })
}

fn assert_about_half(library: &MusicLibrary, track_id: i64, total: usize) {
    let progress = library
        .track_analysis_progress(track_id, BAR_COUNT)
        .unwrap()
        .expect("a running decode reports progress for its track");
    assert!(
        (progress.covered_fraction - 0.5).abs() < 0.05,
        "fraction was {}",
        progress.covered_fraction
    );
    assert!(
        (progress.bars.len() as i64 - i64::from(BAR_COUNT) / 2).abs() <= 3,
        "bar count was {}",
        progress.bars.len()
    );
    assert_eq!(
        progress.spectrogram.cells.len(),
        total / 2 * SPECTROGRAM_BAND_COUNT
    );
    assert_eq!(
        progress.spectrogram.band_count,
        SPECTROGRAM_BAND_COUNT as u32
    );
}

#[test]
fn nav_15d_a_running_decode_reports_progress_for_its_track() {
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

    let import = foreground_import(&library, track_id);
    wait_flag(&pushed);
    assert_about_half(&library, track_id, total);

    set_flag(&release);
    assert_eq!(
        import.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed
    );
}

#[test]
fn nav_15d_progress_is_none_without_a_decode_and_after_the_store() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    assert_eq!(
        library
            .track_analysis_progress(track_id, BAR_COUNT)
            .unwrap(),
        None
    );
    library.register_track_pcm_decoder(Box::new(succeeding_decoder(Arc::new(AtomicUsize::new(0)))));

    assert_eq!(
        library.import_track_analysis(track_id).unwrap(),
        AndroidAnalysisOutcome::Computed
    );

    assert_eq!(
        library
            .track_analysis_progress(track_id, BAR_COUNT)
            .unwrap(),
        None
    );
}

#[test]
fn nav_15d_progress_is_none_once_final_data_is_stored() {
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
    let import = foreground_import(&library, track_id);
    wait_flag(&pushed);
    {
        let writer = library.writer().unwrap();
        let fingerprint = reprise_core::db::track_source_fingerprint(&writer, track_id)
            .unwrap()
            .unwrap();
        reprise_core::db::set_track_render_data(
            &writer,
            track_id,
            fingerprint,
            &reprise_core::waveform::TrackRenderData {
                waveform_peaks: vec![0, u8::MAX],
                spectrogram: reprise_core::spectrogram::TrackSpectrogram::from_cells(vec![
                    0;
                    SPECTROGRAM_BAND_COUNT
                ])
                .unwrap(),
                loudness: None,
            },
        )
        .unwrap();
    }

    assert_eq!(
        library
            .track_analysis_progress(track_id, BAR_COUNT)
            .unwrap(),
        None,
        "stored data is final and is read through the final reads"
    );

    set_flag(&release);
    import.join().unwrap().unwrap();
}

#[test]
fn nav_15d_a_cancelled_decode_leaves_no_render_data() {
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
    let decode = std::thread::spawn(move || {
        library_in_thread
            .analysis_context()
            .compute(track_id, true, None, Some(&slot_in_thread))
    });
    wait_flag(&pushed);
    assert_about_half(&library, track_id, total);

    slot.lock()
        .unwrap()
        .as_ref()
        .expect("the decode published its sink")
        .1
        .cancel();
    set_flag(&release);

    assert_eq!(
        decode.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Cancelled
    );
    let reader = library.reader().unwrap();
    assert!(reprise_core::db::pending_render_data_tracks(&reader)
        .unwrap()
        .iter()
        .any(|track| track.track_id == track_id));
    assert!(reprise_core::db::get_waveform_peaks(&reader, track_id)
        .unwrap()
        .is_none());
    assert!(reprise_core::db::get_track_spectrogram(&reader, track_id)
        .unwrap()
        .is_none());
    drop(reader);
    assert_eq!(
        library
            .track_analysis_progress(track_id, BAR_COUNT)
            .unwrap(),
        None
    );
}

#[test]
fn nav_15d_the_backfill_decode_reports_progress_too() {
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

    let decode = background_compute(&library, track_id);
    wait_flag(&pushed);
    assert_about_half(&library, track_id, total);

    set_flag(&release);
    assert_eq!(
        decode.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed
    );
}

#[test]
fn nav_15d_a_track_without_duration_has_no_expected_length() {
    assert_eq!(expected_frame_count(0), None);
    assert_eq!(expected_frame_count(-1), None);
    assert_eq!(expected_frame_count(50), Some(1));
    assert_eq!(expected_frame_count(51), Some(2));
    assert_eq!(expected_frame_count(240_000), Some(4_800));
}

#[test]
fn nav_15d_progress_for_zero_bars_is_none() {
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
    let import = foreground_import(&library, track_id);
    wait_flag(&pushed);

    assert_eq!(library.track_analysis_progress(track_id, 0).unwrap(), None);

    set_flag(&release);
    import.join().unwrap().unwrap();
}
