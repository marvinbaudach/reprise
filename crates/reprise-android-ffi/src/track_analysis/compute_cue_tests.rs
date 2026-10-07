//! The phone measures the tracks a CUE sheet cuts from one file (MTP-67): one
//! decode of the file stores each track's own stretch, placed by the decoder's
//! timestamps, and the file's last track is measured to the decoded end.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use reprise_core::spectrogram::{SPECTROGRAM_FRAME_RATE_HZ, SPECTROGRAM_SAMPLE_RATE_HZ};
use reprise_core::spectrogram_backfill::render_data_failed;

use super::progress_tests::{foreground_import, gate};
use super::tests::{set_flag, wait_flag, ClosureDecoder};
use crate::cue_album_test_support::{cue_album, CueAlbum};
use crate::track_analysis::{
    AndroidAnalysisOutcome, TrackAnalysisProgress, TrackAnalysisProgressListener,
};
use crate::MusicLibrary;

const RATE: u32 = SPECTROGRAM_SAMPLE_RATE_HZ;
const CHUNK_MS: u64 = 500;
const BAR_COUNT: u32 = 100;

/// `milliseconds` of a 440 Hz tone at the analysis rate, mono, little-endian.
fn tone(milliseconds: u64) -> Vec<u8> {
    let samples = milliseconds * u64::from(RATE) / 1_000;
    (0..samples)
        .flat_map(|index| {
            let phase = std::f64::consts::TAU * 440.0 * index as f64 / f64::from(RATE);
            ((phase.sin() * 0.5 * f64::from(i16::MAX)) as i16).to_le_bytes()
        })
        .collect()
}

/// Pushes `file_ms` of audio in half-second chunks stamped with their start,
/// skipping the chunks whose index `dropped` names.
fn push_file(sink: &crate::track_analysis::AnalysisPcmSink, file_ms: u64, dropped: &[u64]) {
    let chunk = tone(CHUNK_MS);
    for index in 0..file_ms / CHUNK_MS {
        if dropped.contains(&index) {
            continue;
        }
        let start_us = i64::try_from(index * CHUNK_MS * 1_000).unwrap();
        assert!(sink.push_pcm_i16_at(chunk.clone(), RATE, 1, start_us));
    }
}

/// A decoder that decodes the album file as `file_ms` long, counting calls.
fn file_decoder(
    file_ms: u64,
    dropped: Vec<u64>,
    calls: Arc<AtomicUsize>,
) -> Box<dyn super::TrackPcmDecoder> {
    Box::new(ClosureDecoder::new(calls, move |_uri, sink| {
        push_file(sink, file_ms, &dropped);
        Ok(())
    }))
}

fn frames_of(library: &MusicLibrary, track_id: i64) -> Option<usize> {
    library
        .track_spectrogram(track_id)
        .unwrap()
        .map(|spectrogram| spectrogram.cells.len() / spectrogram.band_count as usize)
}

fn peaks_of(library: &MusicLibrary, track_id: i64) -> Option<Vec<u8>> {
    reprise_core::db::get_waveform_peaks(&library.reader().unwrap(), track_id).unwrap()
}

const fn frames_in(seconds: usize) -> usize {
    seconds * SPECTROGRAM_FRAME_RATE_HZ as usize
}

fn measured(album: &CueAlbum) -> Vec<Option<usize>> {
    album
        .track_ids
        .iter()
        .map(|id| frames_of(&album.library, *id))
        .collect()
}

#[test]
fn mtp_67_one_decode_stores_each_track_of_the_file_from_its_own_stretch() {
    let album = cue_album();
    let calls = Arc::new(AtomicUsize::new(0));
    album
        .library
        .register_track_pcm_decoder(file_decoder(30_000, Vec::new(), Arc::clone(&calls)));

    let outcome = album
        .library
        .import_track_analysis(album.track_ids[1])
        .unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::Computed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        measured(&album),
        [
            Some(frames_in(10)),
            Some(frames_in(10)),
            Some(frames_in(10))
        ]
    );
    for track_id in &album.track_ids {
        assert_eq!(
            album.library.import_track_analysis(*track_id).unwrap(),
            AndroidAnalysisOutcome::AlreadyImported
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1, "nothing is decoded again");
}

#[test]
fn mtp_67_a_chunk_the_decoder_dropped_does_not_shift_the_later_tracks() {
    let whole = cue_album();
    whole
        .library
        .register_track_pcm_decoder(file_decoder(30_000, Vec::new(), Arc::default()));
    whole
        .library
        .import_track_analysis(whole.track_ids[0])
        .unwrap();
    let dropped = cue_album();
    // Chunk 3 lies a second and a half into the first track.
    dropped
        .library
        .register_track_pcm_decoder(file_decoder(30_000, vec![3], Arc::default()));

    dropped
        .library
        .import_track_analysis(dropped.track_ids[0])
        .unwrap();

    assert_eq!(
        frames_of(&dropped.library, dropped.track_ids[0]),
        Some(frames_in(10) - 10),
        "the first track is half a second short, not padded"
    );
    for index in [1, 2] {
        assert_eq!(
            peaks_of(&dropped.library, dropped.track_ids[index]),
            peaks_of(&whole.library, whole.track_ids[index]),
            "track {index}"
        );
    }
}

#[test]
fn mtp_67_the_last_track_is_measured_to_the_decoded_end_of_its_file() {
    for (file_ms, last_seconds) in [(28_000, 8), (34_000, 14)] {
        let album = cue_album();
        album
            .library
            .register_track_pcm_decoder(file_decoder(file_ms, Vec::new(), Arc::default()));

        album
            .library
            .import_track_analysis(album.track_ids[2])
            .unwrap();

        assert_eq!(
            frames_of(&album.library, album.track_ids[2]),
            Some(frames_in(last_seconds)),
            "a file of {file_ms} ms"
        );
        assert_eq!(
            frames_of(&album.library, album.track_ids[0]),
            Some(frames_in(10))
        );
    }
}

#[test]
fn mtp_67_a_track_the_file_never_reaches_is_remembered_and_the_others_are_stored() {
    let album = cue_album();
    album
        .library
        .register_track_pcm_decoder(file_decoder(15_000, Vec::new(), Arc::default()));

    album
        .library
        .import_track_analysis(album.track_ids[0])
        .unwrap();

    assert_eq!(
        measured(&album),
        [Some(frames_in(10)), Some(frames_in(5)), None]
    );
    let reader = album.library.reader().unwrap();
    assert!(render_data_failed(&reader, album.track_ids[2]).unwrap());
    assert!(!render_data_failed(&reader, album.track_ids[0]).unwrap());
}

#[test]
fn mtp_67_a_decode_that_fails_remembers_every_track_it_was_measuring() {
    let album = cue_album();
    album
        .library
        .register_track_pcm_decoder(Box::new(ClosureDecoder::new(
            Arc::default(),
            |_uri, _sink| {
                Err(super::AnalysisDecodeError::DecodeFailed {
                    detail: "unreadable".into(),
                })
            },
        )));

    let outcome = album
        .library
        .import_track_analysis(album.track_ids[0])
        .unwrap();

    assert_eq!(outcome, AndroidAnalysisOutcome::DecodeFailed);
    let reader = album.library.reader().unwrap();
    for track_id in &album.track_ids {
        assert!(
            render_data_failed(&reader, *track_id).unwrap(),
            "{track_id}"
        );
    }
}

#[test]
fn mtp_67_the_seek_bar_of_a_later_track_fills_from_its_own_start() {
    let album = cue_album();
    let library = Arc::clone(&album.library);
    let (pushed, release) = (gate(), gate());
    let (pushed_in_decoder, release_in_decoder) = (Arc::clone(&pushed), Arc::clone(&release));
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::default(),
        move |_uri, sink| {
            // Fifteen seconds: the first track whole, the second half-way.
            push_file(sink, 15_000, &[]);
            set_flag(&pushed_in_decoder);
            wait_flag(&release_in_decoder);
            Ok(())
        },
    )));

    let import = foreground_import(&library, album.track_ids[1]);
    wait_flag(&pushed);
    let progress = library
        .track_analysis_progress(album.track_ids[1], BAR_COUNT)
        .unwrap()
        .expect("the playing track reports its own progress");
    set_flag(&release);
    import.join().unwrap().unwrap();

    assert!(
        (progress.covered_fraction - 0.5).abs() < 0.05,
        "covered {}",
        progress.covered_fraction
    );
}

struct CountingListener(Arc<std::sync::Mutex<Option<TrackAnalysisProgress>>>);

impl TrackAnalysisProgressListener for CountingListener {
    fn on_progress(&self, progress: TrackAnalysisProgress) {
        *self.0.lock().unwrap() = Some(progress);
    }
}

#[test]
fn mtp_67_the_backfill_measures_a_cue_file_in_one_decode_and_counts_each_track() {
    let album = cue_album();
    let calls = Arc::new(AtomicUsize::new(0));
    album
        .library
        .register_track_pcm_decoder(file_decoder(30_000, Vec::new(), Arc::clone(&calls)));
    let last = Arc::new(std::sync::Mutex::new(None));

    album
        .library
        .start_track_analysis_backfill(Box::new(CountingListener(Arc::clone(&last))));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !last.lock().unwrap().is_some_and(|progress| {
        progress.total > 0 && progress.done + progress.failed >= progress.total
    }) {
        assert!(
            std::time::Instant::now() < deadline,
            "the backfill never ended"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    album.library.cancel_track_analysis_backfill();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let progress = last
        .lock()
        .unwrap()
        .expect("the backfill published progress");
    assert_eq!((progress.done, progress.failed, progress.total), (3, 0, 3));
    assert_eq!(
        measured(&album),
        [
            Some(frames_in(10)),
            Some(frames_in(10)),
            Some(frames_in(10))
        ]
    );
}
