//! Proves play, skip, pause, seek and stop never stand the main context still
//! on a source that is stuck in the kernel (#1327).
//!
//! The sources are FIFOs (see `stuck_source_support`): one nobody has written
//! to blocks its reader in `open()`, one whose writer went quiet blocks it in
//! `read()`. Every test watches a `glib::timeout_add` heartbeat on the main
//! context, which fires only if nothing keeps that context from running, and
//! asserts that the longest gap between two beats stays under `MAX_GAP` (plus the
//! answer wait of a `play` or seek the test thread makes itself).
//!
//! Every FIFO also lets go by itself after `STUCK_FOR`, so a regression stalls
//! the test for that long and then fails on the gap instead of hanging the
//! suite. At the end each test releases every source and waits for every
//! retired pipeline to reach `Null`.

use std::time::Instant;

use super::segment_support::{on_every_pipeline, Harness};
use super::start_watchdog_tests::{current_playbin, local_track, state_of};
use super::stuck_source_support::{
    assert_every_retired_pipeline_went_down, header_and_a_second_of_silence, make_fifo, Heartbeat,
    LateWriter, OpenerRelease, StreamStall, StuckFifo, HEARTBEAT, MAX_GAP, PATIENCE,
};
use super::*;
use crate::player::START_ANSWER_WAIT;

/// What a `play` or a seek may take on the main context: the bound it waits for
/// the pipeline's answer, plus the slack a loaded machine needs to build one
/// pipeline and to be scheduled at all.
const ANSWER_BOUND: Duration = Duration::from_millis(START_ANSWER_WAIT.as_millis() as u64 + 350);
/// What the heartbeat may miss when the test thread waits for a start's answer
/// by design: `play` and `seek_to` pump nothing for up to `START_ANSWER_WAIT`
/// while the pipeline's queue answers, so that wait is part of the gap and the
/// budget for what else may stall the main context stays `MAX_GAP`. A
/// regression still fails by a wide margin: the base blocks for `STUCK_FOR`.
const GAP_WITH_AWAITED_START: Duration =
    Duration::from_millis(MAX_GAP.as_millis() as u64 + START_ANSWER_WAIT.as_millis() as u64);
/// How long a start gets to leave READY before it is taken for the real
/// `playbin3` hang (see `segment_support::Harness::start`). A healthy start
/// takes milliseconds.
const HUNG_START_WAIT: Duration = Duration::from_millis(2_500);
/// How many times a scenario is made again when its start hung for real.
const REAL_HANG_ATTEMPTS: usize = 5;
/// How long a track that plays gets to be heard before the test moves on.
const A_WHILE: Duration = Duration::from_millis(700);
/// The silence a source delivers is about a second long; a track has been
/// playing it, and the source has been in `read()` for a while, after this.
const SOURCE_DRAINED: Duration = Duration::from_millis(1_600);
/// What a pump collects after the release of a source whose late start fails.
const AFTER_RELEASE: Duration = Duration::from_millis(1_500);

fn playing_events(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
        .count()
}

fn failures_in(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::Error(_)))
        .count()
}

/// Everything the player said after it said it had stopped. Exactly one
/// `Stopped` has to be there: with none, an assertion that nothing followed it
/// would hold of an empty list and prove nothing.
fn after_stopped(events: &[PlayerEvent]) -> &[PlayerEvent] {
    let stops: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, PlayerEvent::StateChanged(PlaybackState::Stopped)))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        stops.len(),
        1,
        "expected exactly one Stopped, so the events after it mean something: {events:?}"
    );
    &events[stops[0] + 1..]
}

/// Runs `scenario` until it ran on starts that left READY. GStreamer 1.28's
/// `playbin3` sometimes never does (see `Harness::start`), and a FIFO that was
/// read by the hung start or closed by `STUCK_FOR` is no use to a start over,
/// so a scenario that saw one returns `false`, before it asserts anything, and
/// is made again with a fresh player and a fresh FIFO. Every other outcome is
/// the scenario's own checks, which panic as usual.
fn without_a_real_hang(scenario: impl Fn() -> bool) {
    for attempt in 1..=REAL_HANG_ATTEMPTS {
        if scenario() {
            return;
        }
        eprintln!("a start hung in READY for real on attempt {attempt}; trying again");
    }
    panic!("the scenario never ran on a start that left READY in {REAL_HANG_ATTEMPTS} attempts");
}

/// Whether the pipeline reached `Playing`; `false` if it is still in READY or
/// has not left `Null` after [`HUNG_START_WAIT`], which is the real hang. A
/// pipeline that is on its way (any other state) gets [`PATIENCE`].
fn reach_playing(harness: &Harness) -> bool {
    let playing = |_: &[PlayerEvent]| state_of(&harness.player).0 == gst::State::Playing;
    harness.pump_until(HUNG_START_WAIT, playing);
    if state_of(&harness.player).0 == gst::State::Playing {
        return true;
    }
    if matches!(
        state_of(&harness.player).0,
        gst::State::Ready | gst::State::Null
    ) {
        return false;
    }
    harness.pump_until(PATIENCE, playing);
    assert_eq!(
        state_of(&harness.player).0,
        gst::State::Playing,
        "the pipeline never reached Playing, so the test measured nothing"
    );
    true
}

fn took<T>(call: impl FnOnce() -> T) -> (T, Duration) {
    let began = Instant::now();
    let result = call();
    (result, began.elapsed())
}

#[test]
fn a_play_blocked_in_open_and_the_stop_after_it_never_stand_the_main_context_still() {
    let harness = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("dead-mount.wav");
    make_fifo(&fifo);
    // Declared after the harness, so every source is let go before the
    // harness stops a pipeline that may still be blocked in it.
    let net = OpenerRelease::at(&fifo);
    let beat = Heartbeat::start();

    let (played, play_took) = took(|| harness.player.play(item(fifo.to_str().unwrap())));
    let started = current_playbin(&harness.player);
    let mut events = harness.pump_for(A_WHILE);
    let (stopped, stop_took) = took(|| harness.player.stop());
    events.extend(harness.pump_for(A_WHILE));
    let gap = beat.longest_gap();

    // The source shows up with bytes that are not audio: the start that is
    // still waiting for it fails, long after the player has stopped.
    let junk = LateWriter::arrive(&fifo, b"this is not audio, and no parser takes it");
    events.extend(harness.pump_for(AFTER_RELEASE));
    drop(junk);
    drop(net);
    assert_every_retired_pipeline_went_down();

    assert!(played.is_ok(), "a start that has not answered is Ok");
    assert!(
        play_took < ANSWER_BOUND,
        "play waited {play_took:?} on a source stuck in open()"
    );
    assert!(stopped.is_ok());
    assert!(
        stop_took < ANSWER_BOUND,
        "stop waited {stop_took:?} on a source stuck in open()"
    );
    assert!(
        gap < GAP_WITH_AWAITED_START,
        "the main context stood still for {gap:?}"
    );
    assert_eq!(
        started.current_state(),
        gst::State::Null,
        "the stopped start went on to run after its source came back"
    );
    let late = after_stopped(&events);
    assert_eq!(playing_events(late), 0, "late events: {late:?}");
    assert_eq!(failures_in(late), 0, "late events: {late:?}");
}

#[test]
fn a_skip_away_from_a_start_stuck_in_a_read_does_not_stand_the_main_context_still() {
    without_a_real_hang(|| {
        let harness = Harness::new();
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("stuck.wav");
        let next = local_track(&dir);
        let stuck = StuckFifo::make(&fifo);
        let beat = Heartbeat::start();

        harness.player.play(item(fifo.to_str().unwrap())).unwrap();
        harness.pump_for(A_WHILE);
        let (skipped, skip_took) = took(|| harness.player.play(item(&next)));
        if !reach_playing(&harness) {
            return false;
        }
        let gap = beat.longest_gap();
        drop(stuck);
        assert_every_retired_pipeline_went_down();

        assert!(skipped.is_ok());
        assert!(
            skip_took < ANSWER_BOUND,
            "the skip waited {skip_took:?} on the start stuck in read()"
        );
        assert!(
            gap < GAP_WITH_AWAITED_START,
            "the main context stood still for {gap:?}"
        );
        true
    });
}

#[test]
fn a_skip_away_from_a_track_stuck_in_a_read_after_playing_does_not_stand_the_main_context_still() {
    without_a_real_hang(|| {
        let harness = Harness::new();
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("stalled.wav");
        let next = local_track(&dir);
        let stalled = StuckFifo::after_playing(&fifo);
        let beat = Heartbeat::start();

        harness.player.play(item(fifo.to_str().unwrap())).unwrap();
        if !reach_playing(&harness) {
            return false;
        }
        harness.pump_for(SOURCE_DRAINED);
        let (skipped, skip_took) = took(|| harness.player.play(item(&next)));
        if !reach_playing(&harness) {
            return false;
        }
        let gap = beat.longest_gap();
        drop(stalled);
        assert_every_retired_pipeline_went_down();

        assert!(skipped.is_ok());
        assert!(
            skip_took < ANSWER_BOUND,
            "the skip waited {skip_took:?} on the track stuck in read()"
        );
        assert!(
            gap < GAP_WITH_AWAITED_START,
            "the main context stood still for {gap:?}"
        );
        true
    });
}

#[test]
fn a_stop_of_a_track_stuck_in_a_read_after_playing_does_not_stand_the_main_context_still() {
    without_a_real_hang(|| {
        let harness = Harness::new();
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("stalled.wav");
        let stalled = StuckFifo::after_playing(&fifo);
        let beat = Heartbeat::start();

        harness.player.play(item(fifo.to_str().unwrap())).unwrap();
        if !reach_playing(&harness) {
            return false;
        }
        harness.pump_for(SOURCE_DRAINED);
        let (stopped, stop_took) = took(|| harness.player.stop());
        harness.pump_for(HEARTBEAT * 3);
        let gap = beat.longest_gap();
        drop(stalled);
        assert_every_retired_pipeline_went_down();

        assert!(stopped.is_ok());
        assert!(
            stop_took < ANSWER_BOUND,
            "stop waited {stop_took:?} on the track stuck in read()"
        );
        assert!(
            gap < GAP_WITH_AWAITED_START,
            "the main context stood still for {gap:?}"
        );
        true
    });
}

#[test]
fn a_pause_during_a_start_blocked_in_open_is_answered_at_once_and_holds_after_the_release() {
    without_a_real_hang(|| {
        let harness = Harness::new();
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("dead-mount.wav");
        make_fifo(&fifo);
        let net = OpenerRelease::at(&fifo);
        let beat = Heartbeat::start();

        harness.player.play(item(fifo.to_str().unwrap())).unwrap();
        let (paused, pause_took) = took(|| harness.player.toggle_pause());
        harness.pump_for(HEARTBEAT * 3);
        let gap = beat.longest_gap();

        // The source comes back with real samples: the start finishes, and into
        // the state the user asked for.
        let writer = LateWriter::arrive(&fifo, &header_and_a_second_of_silence());
        let paused_state = (gst::State::Paused, gst::State::VoidPending);
        harness.pump_until(HUNG_START_WAIT, |_| {
            state_of(&harness.player) == paused_state
        });
        // Blocked in open() looks the same as the real hang until the source
        // comes back; a start still in READY after that is the hang.
        if state_of(&harness.player).0 == gst::State::Ready {
            return false;
        }
        harness.pump_until(PATIENCE, |_| state_of(&harness.player) == paused_state);
        harness.pump_for(A_WHILE);
        let settled = state_of(&harness.player);
        drop(writer);
        drop(net);
        assert_every_retired_pipeline_went_down();

        assert!(
            matches!(paused, Ok(PlaybackState::Paused)),
            "got {paused:?}"
        );
        assert!(
            pause_took < ANSWER_BOUND,
            "the pause waited {pause_took:?} on the start blocked in open()"
        );
        assert!(
            gap < GAP_WITH_AWAITED_START,
            "the main context stood still for {gap:?}"
        );
        assert_eq!(
            settled, paused_state,
            "the start must settle in Paused, the state the user asked for"
        );
        true
    });
}

#[test]
fn a_seek_on_a_track_stuck_in_the_stream_fails_within_the_bound_and_never_stands_the_main_context_still(
) {
    without_a_real_hang(|| {
        let harness = Harness::new();
        let dir = tempfile::tempdir().unwrap();
        let track = local_track(&dir);
        let stall = StreamStall::default();
        let installed = stall.clone();
        on_every_pipeline(&harness.player, move |playbin| {
            installed.install_on(playbin);
        });
        let beat = Heartbeat::start();

        harness.player.play(item(&track)).unwrap();
        if !reach_playing(&harness) {
            return false;
        }
        stall.arm();
        harness.pump_for(A_WHILE);
        let (sought, seek_took) = took(|| harness.player.seek_to(500));
        harness.pump_for(HEARTBEAT * 3);
        let gap = beat.longest_gap();
        stall.release();
        assert_every_retired_pipeline_went_down();

        assert!(
            sought.is_err(),
            "a seek that has not been answered in time is an Err, for the frontend's retry"
        );
        assert!(
            seek_took < ANSWER_BOUND,
            "the seek waited {seek_took:?} on the stalled stream"
        );
        assert!(
            gap < GAP_WITH_AWAITED_START,
            "the main context stood still for {gap:?}"
        );
        true
    });
}

/// A fast failure stays a synchronous `Err`: the frontend's `Ok` side effects
/// (scrobble, history) must never run for a file that never plays.
#[test]
fn a_missing_file_fails_synchronously_and_nothing_follows_it() {
    let harness = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("gone.flac");

    let failed = harness.player.play(item(missing.to_str().unwrap()));
    let events = harness.pump_for(AFTER_RELEASE);

    assert!(failed.is_err(), "a missing file must fail play");
    assert_eq!(playing_events(&events), 0, "events: {events:?}");
    assert_eq!(failures_in(&events), 0, "events: {events:?}");
}
