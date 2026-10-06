use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::track_analysis::{
    AnalysisDecodeError, AnalysisPcmSink, AndroidAnalysisOutcome, TrackPcmDecoder,
};
use crate::MusicLibrary;

use super::progress_tests::{expected_frames, gate, pcm_frames};
use super::supersede_tests::has_render_data;
use super::tests::{library_with_one_track, set_flag, wait_flag, ClosureDecoder};
use super::{AnalysisCell, CurrentDecodeSlot, SharedAnalysisCell};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

fn pending_cell() -> SharedAnalysisCell {
    AnalysisCell::new()
}

fn replace_in_flight_cell(
    library: &MusicLibrary,
    track_id: i64,
    replacement: Option<&SharedAnalysisCell>,
) {
    let mut entries = library
        .analysis_in_flight
        .entries
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    match replacement {
        Some(cell) => {
            entries.insert(track_id, Arc::clone(cell));
        }
        None => {
            entries.remove(&track_id);
        }
    }
}

fn wait_for_cell_waiter(library: &MusicLibrary, track_id: i64) {
    let deadline = std::time::Instant::now() + TEST_TIMEOUT;
    loop {
        let strong_count = library
            .analysis_in_flight
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&track_id)
            .map(Arc::strong_count)
            .unwrap_or_default();
        if strong_count >= 3 {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the compute request never joined the prepared cell"
        );
        std::thread::yield_now();
    }
}

fn finish_cancelled(cell: &AnalysisCell) {
    cell.settle(AndroidAnalysisOutcome::Cancelled);
}

#[test]
fn nav_15c_a_foreground_request_stops_after_three_inherited_cancellations() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let cells = [pending_cell(), pending_cell(), pending_cell()];
    replace_in_flight_cell(&library, track_id, Some(&cells[0]));

    let (result_sender, result_receiver) = mpsc::channel();
    let library_in_request = Arc::clone(&library);
    let request = std::thread::spawn(move || {
        let result = library_in_request
            .analysis_context()
            .compute(track_id, false, None, None);
        result_sender.send(result).unwrap();
    });

    for round in 0..cells.len() {
        wait_for_cell_waiter(&library, track_id);
        replace_in_flight_cell(&library, track_id, cells.get(round + 1));
        finish_cancelled(&cells[round]);
    }

    assert_eq!(
        result_receiver.recv_timeout(TEST_TIMEOUT).unwrap().unwrap(),
        AndroidAnalysisOutcome::Cancelled
    );
    request.join().unwrap();
}

struct CancellingDecoder {
    calls: Arc<AtomicUsize>,
}

impl TrackPcmDecoder for CancellingDecoder {
    fn decode(
        &self,
        _track_uri: String,
        sink: Arc<AnalysisPcmSink>,
        _background: bool,
    ) -> Result<(), AnalysisDecodeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        sink.cancel();
        Ok(())
    }
}

#[test]
fn nav_15c_a_background_request_does_not_retry_an_inherited_cancellation() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let decoder_calls = Arc::new(AtomicUsize::new(0));
    library.register_track_pcm_decoder(Box::new(CancellingDecoder {
        calls: Arc::clone(&decoder_calls),
    }));
    let cell = pending_cell();
    replace_in_flight_cell(&library, track_id, Some(&cell));

    let (result_sender, result_receiver) = mpsc::channel();
    let library_in_request = Arc::clone(&library);
    let request = std::thread::spawn(move || {
        let result = library_in_request
            .analysis_context()
            .compute(track_id, true, None, None);
        result_sender.send(result).unwrap();
    });

    wait_for_cell_waiter(&library, track_id);
    replace_in_flight_cell(&library, track_id, None);
    finish_cancelled(&cell);

    assert_eq!(
        result_receiver.recv_timeout(TEST_TIMEOUT).unwrap().unwrap(),
        AndroidAnalysisOutcome::Cancelled
    );
    assert_eq!(
        decoder_calls.load(Ordering::SeqCst),
        0,
        "the background request must return the inherited cancellation without retrying"
    );
    request.join().unwrap();
}

/// A cell some earlier caller's decode left behind: a supersede for another
/// track reached it before the request under test joined.
fn superseded_before_joining() -> SharedAnalysisCell {
    let cell = pending_cell();
    cell.note_supersede();
    cell
}

#[test]
fn a_superseded_decode_inherited_on_the_last_round_is_a_cancel_to_retry() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let cells = [
        superseded_before_joining(),
        superseded_before_joining(),
        superseded_before_joining(),
    ];
    replace_in_flight_cell(&library, track_id, Some(&cells[0]));

    let (result_sender, result_receiver) = mpsc::channel();
    let library_in_request = Arc::clone(&library);
    let request = std::thread::spawn(move || {
        let result = library_in_request
            .analysis_context()
            .compute(track_id, false, None, None);
        result_sender.send(result).unwrap();
    });

    for round in 0..cells.len() {
        wait_for_cell_waiter(&library, track_id);
        replace_in_flight_cell(&library, track_id, cells.get(round + 1));
        cells[round].settle(AndroidAnalysisOutcome::Superseded);
    }

    assert_eq!(
        result_receiver.recv_timeout(TEST_TIMEOUT).unwrap().unwrap(),
        AndroidAnalysisOutcome::Cancelled,
        "nobody superseded this request, so it must stay free to ask again"
    );
    request.join().unwrap();
}

#[test]
fn a_whole_stream_cancelled_by_a_foreground_preemption_is_not_stored() {
    let (_directory, library, track_id, _music) = library_with_one_track();
    let library = Arc::new(library);
    let total = expected_frames(&library, track_id);
    let (pushed, release) = (gate(), gate());
    let (pushed_in, release_in) = (Arc::clone(&pushed), Arc::clone(&release));
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            assert!(sink.push_pcm_i16(pcm_frames(total), 32_000, 1));
            set_flag(&pushed_in);
            // End of stream reached; the decoder is only returning.
            wait_flag(&release_in);
            Ok(())
        },
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

    // What `TrackAnalysisBackfill::preempt_current_unless` does for another track.
    let (_, sink) = slot
        .lock()
        .unwrap()
        .clone()
        .expect("the decode is published");
    sink.cancel();
    set_flag(&release);

    assert_eq!(
        backfill.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Cancelled,
        "only a supersede keeps a whole stream; a preemption discards it"
    );
    assert!(!has_render_data(&library, track_id));
}
