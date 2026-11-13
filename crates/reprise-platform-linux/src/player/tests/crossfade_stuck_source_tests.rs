//! Proves a crossfade never stands the main context, or the outgoing track's
//! end, still on a source that is stuck in the kernel (#1327).
//!
//! The incoming track is a FIFO nobody writes (its `open()` blocks), or the
//! outgoing pipeline is stalled in the stream, so that its `Null` hangs. See
//! `transport_freeze_tests` for the shape of the assertions.

use super::segment_support::{count, write_regions_wav, Harness};
use super::start_watchdog_tests::state_of;
use super::stuck_source_support::{
    assert_every_retired_pipeline_went_down, make_fifo, Heartbeat, OpenerRelease, StreamStall,
    HEARTBEAT, MAX_GAP, PATIENCE,
};
use super::*;
use crate::player::START_ANSWER_WAIT;
use reprise_core::library::settings::TrackTransition;
use std::time::Instant;

/// The secondary's `open()` is released after this, long after the fade has
/// given way, so only the bound can have ended the wait.
const OPEN_RELEASED_AFTER: Duration = Duration::from_secs(12);
const FADE_SECONDS: u8 = 2;
const OUTGOING_MS: u32 = 4_000;
/// What a `play` may take on the main context: the answer bound plus slack.
const PLAY_BOUND: Duration = Duration::from_millis(START_ANSWER_WAIT.as_millis() as u64 + 350);

fn finished(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::TrackFinished)
}

fn advanced(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::AdvancedToNext)
}

fn tone(directory: &tempfile::TempDir, name: &str, ms: u32) -> String {
    let path = directory.path().join(name);
    write_regions_wav(&path, &[(ms, true)]);
    path.to_str().unwrap().to_owned()
}

fn secondary_published(player: &Player) -> bool {
    player
        .incoming
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some()
}

#[test]
fn a_secondary_blocked_in_open_gives_way_and_the_outgoing_track_is_reported_finished() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", OUTGOING_MS);
    let fifo = directory.path().join("dead-mount.wav");
    make_fifo(&fifo);
    let openers = OpenerRelease::after(&fifo, OPEN_RELEASED_AFTER);
    player.set_transition(TrackTransition::Crossfade, FADE_SECONDS);
    let beat = Heartbeat::start();

    player.play(item(&first)).unwrap();
    player.set_next(Some(item(fifo.to_str().unwrap())));
    let began = Instant::now();
    let events = harness.pump_until(PATIENCE, |events| count(events, finished) > 0);
    let took = began.elapsed();
    let gap = beat.longest_gap();
    let guard_down = !player.crossfading.load(Ordering::SeqCst);
    let slot_empty = !secondary_published(player);
    drop(openers);
    assert_every_retired_pipeline_went_down();

    assert_eq!(
        count(&events, finished),
        1,
        "the outgoing track's end was swallowed by a fade that never gave way: {events:?}"
    );
    assert_eq!(count(&events, advanced), 0);
    assert!(guard_down, "the crossfade guard stayed up");
    assert!(slot_empty, "the secondary stayed in the incoming slot");
    assert!(
        took < Duration::from_millis(u64::from(OUTGOING_MS) + 2_000),
        "the end was reported {took:?} in"
    );
    assert!(gap < MAX_GAP, "the main context stood still for {gap:?}");
}

#[test]
fn a_play_during_a_fade_whose_secondary_is_blocked_in_open_shows_no_gap() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", OUTGOING_MS);
    let third = tone(&directory, "third.wav", OUTGOING_MS);
    let fifo = directory.path().join("dead-mount.wav");
    make_fifo(&fifo);
    let openers = OpenerRelease::after(&fifo, OPEN_RELEASED_AFTER);
    player.set_transition(TrackTransition::Crossfade, FADE_SECONDS);
    let beat = Heartbeat::start();

    player.play(item(&first)).unwrap();
    player.set_next(Some(item(fifo.to_str().unwrap())));
    harness.pump_until(PATIENCE, |_| secondary_published(player));
    assert!(
        secondary_published(player),
        "the fade never published its secondary, so the test measured nothing"
    );
    harness.pump_for(HEARTBEAT * 5);
    let began = Instant::now();
    let played = player.play(item(&third));
    let play_took = began.elapsed();
    harness.pump_until(PATIENCE, |_| state_of(player).0 == gst::State::Playing);
    let gap = beat.longest_gap();
    let playing = state_of(player).0;
    drop(openers);
    assert_every_retired_pipeline_went_down();

    assert!(played.is_ok());
    assert!(
        play_took < PLAY_BOUND,
        "play waited {play_took:?} on the secondary blocked in open()"
    );
    assert_eq!(playing, gst::State::Playing);
    assert!(gap < MAX_GAP, "the main context stood still for {gap:?}");
}

#[test]
fn a_promotion_retires_the_outgoing_pipeline_with_the_playbin_lock_free() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", 5_000);
    let second = tone(&directory, "second.wav", 4_000);
    player.set_transition(TrackTransition::Crossfade, FADE_SECONDS);
    // Only the pipeline of the outgoing track is stalled: it is the first
    // that a start builds from here on.
    let stall = StreamStall::default();
    let installed = stall.clone();
    let hooked = Arc::new(AtomicBool::new(false));
    super::super::pipeline_swap::test_seam::on_pipeline_built(Arc::new(move |playbin| {
        if !hooked.swap(true, Ordering::SeqCst) {
            installed.install_on(playbin);
        }
    }));
    let beat = Heartbeat::start();

    player.play(item(&first)).unwrap();
    player.set_next(Some(item(&second)));
    // The outgoing stream stalls once the fade has begun: from then on its
    // `Null` hangs, and the promotion is what retires it.
    let events = harness.pump_until(PATIENCE, |events| {
        if player.crossfading.load(Ordering::SeqCst) {
            stall.arm();
        }
        count(events, advanced) > 0
    });
    let began = Instant::now();
    player.set_volume(0.5);
    let volume_took = began.elapsed();
    harness.pump_for(HEARTBEAT * 5);
    let gap = beat.longest_gap();
    stall.release();
    assert_every_retired_pipeline_went_down();

    assert_eq!(count(&events, advanced), 1, "events: {events:?}");
    assert!(
        volume_took < Duration::from_millis(200),
        "set_volume waited {volume_took:?} for the playbin lock after the promotion"
    );
    assert!(gap < MAX_GAP, "the main context stood still for {gap:?}");
}
