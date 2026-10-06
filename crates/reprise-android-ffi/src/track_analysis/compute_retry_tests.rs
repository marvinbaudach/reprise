use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, PoisonError};
use std::time::Duration;

use crate::track_analysis::{
    AnalysisDecodeError, AnalysisPcmSink, AndroidAnalysisOutcome, TrackPcmDecoder,
};
use crate::MusicLibrary;

use super::tests::library_with_one_track;
use super::{AnalysisCell, SharedAnalysisCell};

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
