//! Proves the watchdog's restart stands down when the start it looked at is no
//! longer the one it would replace.
//!
//! A look takes the playbin lock only to observe, so between the look and the
//! restart a `play`, a `stop` or a gapless step can land. The restart checks
//! again under the lock, and these tests put each of those in that gap by
//! wrapping the player's restart callback: the wrapper does what the other
//! thread would, then calls the real restart.
//!
//! Two guards stand behind each other for a superseded stream — the look's
//! `same_stream` and the restart's own check — so each has a test that only it
//! can satisfy: the look's by a counting wrapper that must never be called, the
//! restart's by a wrapper that supersedes the stream after the look.

use std::sync::atomic::AtomicUsize;

use super::segment_support::write_regions_wav;
use super::start_watchdog_tests::{
    current_playbin, errors, hang_starts, harness_with_deadline, local_track, state_of, PATIENCE,
    QUIET_FOR, ROOMY_DEADLINE,
};
use super::*;

/// Replaces the player's restart with one that runs `before` first and then the
/// real one. Returns how many times it was called.
pub(super) fn restart_after(
    player: &mut Player,
    before: impl Fn() + Send + Sync + 'static,
) -> Arc<AtomicUsize> {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let real = player.start_watchdog.restart.clone();
    player.start_watchdog.restart = Arc::new(move |request| {
        counted.fetch_add(1, Ordering::SeqCst);
        before();
        real(request)
    });
    calls
}

/// What a gapless step or a crossfade promotion does to the stream: replaces it
/// without a `disarm` in its path, so only the generation tells the watch.
#[test]
fn a_hung_start_whose_stream_was_replaced_is_not_even_asked_to_restart() {
    let mut harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    let calls = restart_after(&mut harness.player, || {});
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);

    harness
        .player
        .stream_generation
        .fetch_add(1, Ordering::SeqCst);
    let events = harness.pump_for(QUIET_FOR);

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the look must stand down on a replaced stream"
    );
    assert_eq!(harness.player.start_watchdog.restarts(), 0);
    assert_eq!(reached.load(Ordering::SeqCst), 1);
    assert_eq!(errors(&events), 0, "events: {events:?}");
}

/// `stop` leaves the generation where it was, so a resume of the same URI that
/// hangs again looks like the start the old watch was armed for. Only the
/// `disarm` in `stop` keeps that watch from starting the resume over.
#[test]
fn a_hung_resume_after_a_stop_is_not_started_over_by_the_old_watch() {
    // Twice the usual room: a start slow to reach the probe on a loaded machine
    // must not be started over before the `stop`, which would end the test on
    // a restart it did not set out to look at. The old watch, if it survived
    // the `stop`, still fires well inside the quiet period.
    let harness = harness_with_deadline(ROOMY_DEADLINE * 2);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);

    harness.player.stop().unwrap();
    harness.player.toggle_pause().unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 2);
    let events = harness.pump_for(QUIET_FOR);

    assert_eq!(harness.player.start_watchdog.restarts(), 0);
    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "the old watch started the resume over"
    );
    assert_eq!(errors(&events), 0, "events: {events:?}");
}

/// The gap the unlocked look opens: the stream is replaced after the look
/// found the start hung, and before the restart takes the lock.
#[test]
fn a_restart_whose_stream_was_replaced_after_the_look_leaves_everything_alone() {
    let mut harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    let generation = harness.player.stream_generation.clone();
    let calls = restart_after(&mut harness.player, move || {
        generation.fetch_add(1, Ordering::SeqCst);
    });
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    let hung = current_playbin(&harness.player);

    let events = harness.pump_for(QUIET_FOR);

    assert_eq!(calls.load(Ordering::SeqCst), 1, "the look must have asked");
    assert_eq!(harness.player.start_watchdog.restarts(), 0);
    assert_eq!(
        current_playbin(&harness.player),
        hung,
        "a superseded start's restart replaced the pipeline"
    );
    assert_eq!(reached.load(Ordering::SeqCst), 1, "no start over was made");
    assert_eq!(errors(&events), 0, "events: {events:?}");
}

/// A `stop` lands after the look: it disarms the watch before it takes the lock
/// and does not move the generation, so only the armed check under the lock
/// keeps the restart from starting the stopped track again.
#[test]
fn a_restart_after_a_stop_that_landed_after_the_look_leaves_the_pipeline_alone() {
    let mut harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    let epoch = harness.player.start_watchdog.epoch_handle();
    let calls = restart_after(&mut harness.player, move || {
        epoch.fetch_add(1, Ordering::SeqCst);
    });
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    let hung = current_playbin(&harness.player);

    harness.pump_for(QUIET_FOR);

    assert_eq!(calls.load(Ordering::SeqCst), 1, "the look must have asked");
    assert_eq!(harness.player.start_watchdog.restarts(), 0);
    assert_eq!(current_playbin(&harness.player), hung);
    assert_eq!(reached.load(Ordering::SeqCst), 1);
}

/// A real newer `play` lands in the gap: its start is left running, and the
/// watchdog neither restarts it nor reports anything. The `play` is made by
/// another thread while the restart waits on it, which is what a `play` from
/// the frontend's other threads does between a look and a restart.
#[test]
fn a_play_that_lands_between_the_look_and_the_restart_is_left_to_play() {
    let mut harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let hung_path = local_track(&dir);
    let newer_path = dir.path().join("newer.wav");
    write_regions_wav(&newer_path, &[(2_000, true)]);
    // Only the first start hangs; the newer one passes the probe.
    let reached = hang_starts(&harness.player, 1);
    let (in_the_gap, play_now) = std::sync::mpsc::channel::<()>();
    let (played, newer_played) = std::sync::mpsc::channel::<()>();
    let newer_played = Mutex::new(newer_played);
    let calls = restart_after(&mut harness.player, move || {
        let _ = in_the_gap.send(());
        let _ = newer_played
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(PATIENCE);
    });
    harness.player.play(item(&hung_path)).unwrap();
    let hung = current_playbin(&harness.player);

    let player = &harness.player;
    let events = std::thread::scope(|scope| {
        scope.spawn(move || {
            if play_now.recv_timeout(PATIENCE).is_ok() {
                player.play(item(newer_path.to_str().unwrap())).unwrap();
            }
            let _ = played.send(());
        });
        harness.pump_until(PATIENCE, |_| {
            state_of(&harness.player).0 == gst::State::Playing
        });
        harness.pump_for(QUIET_FOR)
    });

    assert_eq!(calls.load(Ordering::SeqCst), 1, "the look must have asked");
    assert_eq!(harness.player.start_watchdog.restarts(), 0);
    assert_eq!(
        current_playbin(&harness.player),
        hung,
        "the newer start was moved to another pipeline"
    );
    assert_eq!(state_of(&harness.player).0, gst::State::Playing);
    assert_eq!(reached.load(Ordering::SeqCst), 2, "hung start, newer start");
    assert_eq!(errors(&events), 0, "events: {events:?}");
}
