//! A CUE start's `Playing` is never the last word after a pause or a stop
//! that came in while the start's job was reporting it (issue #1378).
//!
//! `finish_start` checks that `Playing` is still what the user asked for, then
//! reports it. The transport reports a pause or a stop from its own thread, so
//! without one lock around the check and the send a pause that lands between
//! them is reported first and the stale `Playing` follows it. The job is
//! parked exactly between the check and the send, the user acts, and then the
//! job goes on.
//!
//! A stop reaches the gate's lock first (`reset_transition`), so here the lock
//! alone orders it; the epoch the check compares is pinned by
//! `finish_start_tests`, where a newer start moves it before the job's check.

use std::time::Duration;

use super::segment_support::{cue_item, Harness};
use super::*;
use crate::player::pipeline_swap::test_seam::{self, Parking};

const PATIENCE: Duration = Duration::from_secs(10);
/// How long the interruption is given to report while the job is still parked.
const WHILE_PARKED: Duration = Duration::from_millis(300);
/// How long the test keeps collecting events after the job went on.
const SETTLE: Duration = Duration::from_millis(500);
const ZERO_START_CUE: (i64, i64) = (0, 800);

fn states_of(events: &[PlayerEvent]) -> Vec<PlaybackState> {
    events
        .iter()
        .filter_map(|event| match event {
            PlayerEvent::StateChanged(state) => Some(*state),
            _ => None,
        })
        .collect()
}

/// Starts a CUE track, parks its job between the check and the `Playing` send,
/// lets `interrupt` run on another thread meanwhile, releases the job, and
/// returns the state changes the frontend saw, in order.
fn states_seen_when_interrupted(interrupt: impl Fn(&Player) + Sync) -> Vec<PlaybackState> {
    let harness = Harness::new();
    let parking = Parking::new();
    let armed = parking.clone();
    test_seam::on_pipeline_built(Arc::new(move |pipeline| {
        test_seam::park_playing_event_of(pipeline, armed.clone());
    }));
    let album = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("sine.flac");
    harness.start(|| {
        harness
            .player
            .play(cue_item(&album, ZERO_START_CUE, 0.0))
            .unwrap();
    });
    let mut events = harness.pump_until(PATIENCE, |_| parking.entered());
    assert!(
        parking.entered(),
        "the CUE start never reached its Playing report, so the test measured nothing: {events:?}"
    );

    std::thread::scope(|scope| {
        let interruption = scope.spawn(|| interrupt(&harness.player));
        // With the job parked, an interruption that does not wait for it has
        // reported by now; one that does is still waiting for the job.
        std::thread::sleep(WHILE_PARKED);
        parking.release();
        interruption.join().unwrap();
    });
    events.extend(harness.pump_for(SETTLE));
    let states = states_of(&events);
    assert!(!states.is_empty(), "no state change at all: {events:?}");
    states
}

#[test]
fn a_pause_that_lands_while_playing_is_reported_is_the_last_word() {
    let states = states_seen_when_interrupted(|player| {
        player.toggle_pause().unwrap();
    });
    assert_eq!(
        states.last(),
        Some(&PlaybackState::Paused),
        "the stale Playing outlived the pause: {states:?}"
    );
}

#[test]
fn a_stop_that_lands_while_playing_is_reported_is_the_last_word() {
    let states = states_seen_when_interrupted(|player| {
        player.stop().unwrap();
    });
    assert_eq!(
        states.last(),
        Some(&PlaybackState::Stopped),
        "the stale Playing outlived the stop: {states:?}"
    );
}
