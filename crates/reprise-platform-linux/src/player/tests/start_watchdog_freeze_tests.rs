//! Proves a watchdog restart does not freeze the main context when the stuck
//! start sits inside a kernel read (#1323).
//!
//! The stuck source is a FIFO whose only writer is the test: a start of it
//! opens the file, reads what was written (a WAV header, no sample), and then
//! blocks in `read()` with nothing more to come, which `Null` cannot interrupt.
//! That is the shape a dead NFS/SMB mount gives a `filesrc`. The test's fd is
//! closed after a bounded time, so a regression stalls the main context for
//! that long and then fails on the gap, instead of hanging the suite.

use std::sync::Mutex;
use std::time::Instant;

use super::segment_support::Harness;
use super::stuck_source_support::{
    assert_every_retired_pipeline_went_down, longest_gap, StuckFifo, HEARTBEAT, MAX_GAP, PATIENCE,
    STUCK_FOR, WATCH_AFTER_RESTART,
};
use super::*;

/// The watchdog's deadline: short, so the restart lands within a second.
const TEST_DEADLINE: Duration = Duration::from_millis(150);

#[test]
fn a_restart_of_a_start_stuck_in_a_read_does_not_freeze_the_main_context() {
    let mut harness = Harness::new();
    harness.player.start_watchdog.deadline = TEST_DEADLINE;
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("stuck.wav");
    // Declared after the harness, so it is dropped first: the fd is closed
    // before the harness stops a pipeline that may still be reading from it.
    let stuck = StuckFifo::make(&fifo);

    harness.player.play(item(fifo.to_str().unwrap())).unwrap();
    let beats = Arc::new(Mutex::new(vec![Instant::now()]));
    let recorded = beats.clone();
    let heartbeat = gst::glib::timeout_add(HEARTBEAT, move || {
        recorded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Instant::now());
        gst::glib::ControlFlow::Continue
    });

    // The restart, or the end of the stuck period without one: the old code
    // fails to restart once the FIFO is gone, so waiting for the counter alone
    // would wait out the whole patience.
    let began = Instant::now();
    harness.pump_until(PATIENCE, |_| {
        harness.player.start_watchdog.restarts() >= 1 || began.elapsed() > STUCK_FOR * 2
    });
    harness.pump_for(WATCH_AFTER_RESTART);
    heartbeat.remove();

    let gap = longest_gap(&beats.lock().unwrap_or_else(PoisonError::into_inner));
    assert!(
        gap < MAX_GAP,
        "the main context stood still for {gap:?} while the watchdog restarted a stuck start"
    );
    assert!(
        harness.player.start_watchdog.restarts() >= 1,
        "the watchdog never restarted the stuck start, so the test measured nothing"
    );

    // The retired pipeline is the reaper's, and it finishes once the read does:
    // nothing is left running behind the test.
    drop(stuck);
    assert_every_retired_pipeline_went_down();
}
