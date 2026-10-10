//! Proves a local start that hangs in READY is started over, and given up on
//! when it keeps hanging.
//!
//! The hang is made, not waited for: a blocking probe on the gain element's
//! sink pad keeps the first buffer of a start from reaching the sink, so the
//! pipeline never prerolls and sits in READY with PLAYING pending, exactly as
//! a real hang does. `Null` flushes the pad and ends the block. The watchdog's
//! deadline is shortened to a fraction of a second; nothing here depends on
//! how loaded the machine is.

use std::sync::atomic::AtomicUsize;

use super::segment_support::{gain_element, write_regions_wav, Harness};
use super::*;

/// The watchdog's deadline where the test only waits for it.
const TEST_DEADLINE: Duration = Duration::from_millis(150);
/// The deadline where the test acts on the hung start first: it has to land
/// before the watchdog does, so it gets room to do so on a busy machine.
const ROOMY_DEADLINE: Duration = Duration::from_millis(1_000);
/// Far more than the attempts need, and far less than a hang would take.
const PATIENCE: Duration = Duration::from_secs(20);
/// Long enough for several more deadlines to pass, for the "nothing happens" arms.
const QUIET_FOR: Duration = Duration::from_millis(2_500);
const TRACK_MS: u32 = 2_000;

/// Blocks the first `hang_starts` starts at the gain element's sink pad and
/// lets every later one through. Returns how many starts reached the pad.
fn hang_starts(player: &Player, hang_starts: usize) -> Arc<AtomicUsize> {
    let reached = Arc::new(AtomicUsize::new(0));
    let counted = reached.clone();
    gain_element(player).static_pad("sink").unwrap().add_probe(
        gst::PadProbeType::BLOCK_DOWNSTREAM,
        move |_, _| {
            let start = counted.fetch_add(1, Ordering::SeqCst) + 1;
            if start <= hang_starts {
                // Stay blocked: the pad is released by the next `Null`.
                gst::PadProbeReturn::Ok
            } else {
                gst::PadProbeReturn::Remove
            }
        },
    );
    reached
}

fn local_track(dir: &tempfile::TempDir) -> String {
    let path = dir.path().join("track.wav");
    write_regions_wav(&path, &[(TRACK_MS, true)]);
    path.to_str().unwrap().to_owned()
}

fn errors(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::Error(_)))
        .count()
}

fn state_of(player: &Player) -> (gst::State, gst::State) {
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let (_, current, pending) = playbin.state(gst::ClockTime::ZERO);
    (current, pending)
}

fn harness_with_deadline(deadline: Duration) -> Harness {
    let mut harness = Harness::new();
    harness.player.start_watchdog.deadline = deadline;
    harness
}

#[test]
fn a_local_start_that_hangs_in_ready_is_started_over() {
    let harness = harness_with_deadline(TEST_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, 1);

    harness.player.play(item(&path)).unwrap();
    // The control arm: the first start really is the hang the watchdog is for.
    let hung = harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    assert_eq!(errors(&hung), 0);

    // Nothing but the watchdog can end it: the pump just runs the main context.
    let events = harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });
    assert_eq!(
        state_of(&harness.player).0,
        gst::State::Playing,
        "the watchdog must start the hung track over (events: {events:?})"
    );
    assert_eq!(reached.load(Ordering::SeqCst), 2, "one restart, no more");
    assert_eq!(errors(&events), 0, "a recovered start reports nothing");
}

#[test]
fn a_paused_hung_start_is_started_over_into_paused() {
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, 1);
    harness.player.play(item(&path)).unwrap();
    // The hang is real before the user acts on it.
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    // Pausing the hung start leaves PAUSED, not PLAYING, as the pending state.
    harness
        .player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .set_state(gst::State::Paused)
        .unwrap();

    harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Paused
    });

    assert_eq!(state_of(&harness.player).0, gst::State::Paused);
    assert_eq!(reached.load(Ordering::SeqCst), 2);
}

#[test]
fn a_start_that_keeps_hanging_is_given_up_on_after_three_starts() {
    let harness = harness_with_deadline(TEST_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);

    harness.player.play(item(&path)).unwrap();
    let events = harness.pump_until(PATIENCE, |events| errors(events) > 0);
    assert_eq!(errors(&events), 1, "events: {events:?}");
    assert_eq!(reached.load(Ordering::SeqCst), 3, "three starts, no fourth");

    let later = harness.pump_for(QUIET_FOR);
    assert_eq!(errors(&later), 0, "the error is reported once");
    assert_eq!(reached.load(Ordering::SeqCst), 3);
}

#[test]
fn a_hung_start_the_user_stopped_is_left_stopped() {
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);

    harness.player.stop().unwrap();
    let events = harness.pump_for(QUIET_FOR);

    assert_eq!(errors(&events), 0);
    assert_eq!(reached.load(Ordering::SeqCst), 1, "no restart after a stop");
    assert_eq!(state_of(&harness.player).0, gst::State::Null);
}

#[test]
fn a_hung_start_that_another_track_replaced_is_not_started_over() {
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, 1);
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);

    // The user starts the track again by hand; that start is not hung.
    harness.player.play(item(&path)).unwrap();
    let events = harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });
    let later = harness.pump_for(QUIET_FOR);

    assert_eq!(state_of(&harness.player).0, gst::State::Playing);
    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "the old watch started nothing"
    );
    assert_eq!(errors(&events) + errors(&later), 0);
}
