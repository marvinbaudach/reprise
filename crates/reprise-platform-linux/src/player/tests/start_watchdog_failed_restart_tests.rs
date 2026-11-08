//! Proves a restart that fails is reported as the track's failure, once.
//!
//! Where it fails decides who reports. A restart that never swapped (the new
//! pipeline could not be built, or could not take the start over) leaves the
//! hung pipeline in place with nothing on any bus, so the watchdog is the only
//! one who can tell the frontend. A start that fails after the swap puts its
//! own error on the bus of the pipeline it began on, and the watchdog must not
//! add a second report for it.

use super::start_watchdog_tests::{
    current_playbin, errors, hang_starts, harness_with_deadline, local_track, PATIENCE, QUIET_FOR,
    ROOMY_DEADLINE,
};
use super::*;
use crate::player::pipeline_swap::test_seam;

/// Long enough for a restart to have been tried, short enough to fail fast.
const SETTLE: Duration = Duration::from_secs(4);

fn failures(events: &[PlayerEvent]) -> Vec<PlaybackFailureKind> {
    events
        .iter()
        .filter_map(|event| match event {
            PlayerEvent::Error(failure) => Some(failure.kind()),
            _ => None,
        })
        .collect()
}

/// A swap-built pipeline whose audio filter is not the player's, so the track
/// gain the start carries over cannot be found on it.
fn without_track_gain(fresh: &gst::Element) {
    let filter = gst::ElementFactory::make("identity").build().unwrap();
    fresh.set_property("audio-filter", &filter);
}

#[test]
fn a_restart_that_cannot_carry_the_start_over_reports_the_track_once() {
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    test_seam::on_pipeline_built(Arc::new(without_track_gain));

    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    let mut events = harness.pump_until(SETTLE, |events| errors(events) > 0);
    events.extend(harness.pump_for(QUIET_FOR));

    assert_eq!(
        failures(&events),
        vec![PlaybackFailureKind::StartNeverFinished],
        "events: {events:?}"
    );
    assert_eq!(
        harness.player.start_watchdog.restarts(),
        0,
        "nothing was begun over"
    );
}

/// How long after the first error the bus's own follow-up is waited for: a
/// failed state change posts two messages. Shorter than the next deadline.
const FOLLOW_UP: Duration = Duration::from_millis(300);

/// The file is gone when the start over opens it, so the new pipeline's state
/// change fails after the swap and its own error reaches the bus. The watchdog
/// adds no report of its own: every error is the bus's, of the one session the
/// start belongs to, which the frontend answers once.
#[test]
fn a_start_over_that_fails_after_the_swap_is_reported_by_its_bus_alone() {
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, 1);

    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    let hung = current_playbin(&harness.player);
    std::fs::remove_file(&path).unwrap();
    let mut events = harness.pump_until(SETTLE, |events| errors(events) > 0);
    events.extend(harness.pump_for(FOLLOW_UP));

    assert!(
        errors(&events) > 0,
        "the failed start over reported nothing"
    );
    assert!(
        !failures(&events).contains(&PlaybackFailureKind::StartNeverFinished),
        "the failed start over is the bus's to report: {events:?}"
    );
    let mut sessions: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            PlayerEvent::Error(failure) => Some(failure.session_id()),
            _ => None,
        })
        .collect();
    sessions.dedup();
    assert_eq!(sessions.len(), 1, "more than one session: {events:?}");
    assert_ne!(
        current_playbin(&harness.player),
        hung,
        "the start over never reached the swap, so the test measured nothing"
    );
    assert!(harness.player.start_watchdog.restarts() >= 1);
}
