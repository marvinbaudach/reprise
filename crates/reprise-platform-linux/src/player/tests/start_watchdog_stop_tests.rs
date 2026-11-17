//! Proves a `stop` ends the start watch, where nothing else would.
//!
//! A `stop` that builds its idle pipeline leaves the watch nothing to act on:
//! the slot holds a pipeline in `Null`, which is not a hung start, and the start
//! record is gone. Only when no idle pipeline can be built does the hung
//! pipeline stay in the slot, still in READY with its start pending and still on
//! the stream generation the watch was armed for, its `Null` queued behind
//! whatever its queue is stuck in. The watch's give-up reports the track with no
//! check under the lock, so there only the `disarm` in `stop` keeps it from
//! telling the frontend, which answers by skipping, that the stopped track
//! never started.

use std::sync::mpsc::sync_channel;

use super::start_watchdog_restart_tests::on_the_injected_hang;
use super::start_watchdog_tests::{
    errors, hang_starts, harness_with_deadline, local_track, state_of, PATIENCE,
};
use super::*;
use crate::player::pipeline_swap::test_seam;

/// Short enough that three attempts fit a test, long enough that the stop lands
/// between the second restart and the give-up (three deadlines) on a busy machine.
const DEADLINE: Duration = Duration::from_millis(400);
/// More than the give-up of the last attempt would need.
const PAST_THE_GIVE_UP: Duration = Duration::from_millis(2_400);

#[test]
fn a_stop_that_leaves_the_hung_pipeline_in_the_slot_ends_the_watch_before_its_give_up() {
    on_the_injected_hang(|| {
        let harness = harness_with_deadline(DEADLINE);
        let dir = tempfile::tempdir().unwrap();
        let path = local_track(&dir);
        let reached = hang_starts(&harness.player, usize::MAX);
        harness.player.play(item(&path)).unwrap();
        // The third and last attempt is under way: nothing but the give-up is left.
        let early = harness.pump_until(PATIENCE, |events| {
            harness.player.start_watchdog.restarts() >= 2 || errors(events) > 0
        });
        if errors(&early) > 0 || reached.load(Ordering::SeqCst) < 2 {
            return false;
        }
        // Its `Null` waits behind a job that does not end: a source stuck in
        // the kernel does that to a pipeline's queue.
        let (release, released) = sync_channel::<()>(0);
        harness
            .player
            .playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .submit(move |_| {
                let _ = released.recv_timeout(PATIENCE);
            });

        test_seam::fail_next_builds(1);
        harness.player.stop().unwrap();
        let events = harness.pump_for(PAST_THE_GIVE_UP);

        // The control arm: the pipeline really stood there hung, long enough
        // for the give-up to have come.
        assert_eq!(
            state_of(&harness.player).0,
            gst::State::Ready,
            "the hung pipeline left the slot, so the test measured nothing"
        );
        assert_eq!(
            errors(&events),
            0,
            "the watch of a stopped start gave up on it: {events:?}"
        );
        let _ = release.send(());
        true
    });
}
