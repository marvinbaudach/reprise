//! `complete_start` writes the start-seek's record only for the pipeline whose
//! start the gate serves.
//!
//! A start over swaps the pipeline and advances the gate's epoch before the
//! hung pipeline is retired, so the retirement is no proof that a late
//! `ASYNC_DONE` is stale: the epoch the pipeline carries is.

use super::*;
use crate::player::pipeline_queue::Pipeline;

const PREROLL_WAIT: u64 = 5;
const TRACK_START_MS: i64 = 5_000;

/// A pipeline prerolled in Paused, as a CUE start is when `ASYNC_DONE` arrives.
fn prerolled_pipeline() -> Pipeline {
    gst::init().unwrap();
    let element = gst::parse::launch("audiotestsrc ! fakesink").unwrap();
    element.set_state(gst::State::Paused).unwrap();
    element
        .state(gst::ClockTime::from_seconds(PREROLL_WAIT))
        .0
        .unwrap();
    Pipeline::new(element)
}

fn gate_with_a_pending_start() -> Arc<SegmentGate> {
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///album.flac", (TRACK_START_MS, 9_000));
    gate
}

#[test]
fn a_hung_pipelines_late_preroll_records_nothing_after_the_start_over_moved_the_epoch() {
    let hung = prerolled_pipeline();
    let gate = gate_with_a_pending_start();
    hung.set_start_epoch(gate.lock().epoch);
    // The start over: the gate moves on, and the hung pipeline is not yet
    // retired, as in `restart_hung_start` until its lock is released.
    gate.next_epoch();
    assert!(!hung.queue().is_retired());

    gate.complete_start(hung.element(), &hung.queue());

    let state = gate.lock();
    assert_eq!(
        state.start_seek_ms, None,
        "a stale preroll wrote the start-seek of the pipeline that replaced it"
    );
    assert_eq!(state.pending_start_ms, Some(TRACK_START_MS));
}

#[test]
fn the_current_pipelines_preroll_records_its_start_seek() {
    let current = prerolled_pipeline();
    let gate = gate_with_a_pending_start();
    current.set_start_epoch(gate.lock().epoch);
    gate.next_epoch();
    current.set_start_epoch(gate.lock().epoch);

    gate.complete_start(current.element(), &current.queue());

    assert_eq!(gate.lock().start_seek_ms, Some(TRACK_START_MS));
}

#[test]
fn a_pipeline_the_gate_does_not_track_is_guarded_by_its_retirement_alone() {
    let untracked = prerolled_pipeline();
    let gate = gate_with_a_pending_start();

    gate.complete_start(untracked.element(), &untracked.queue());

    assert_eq!(gate.lock().start_seek_ms, Some(TRACK_START_MS));
}
