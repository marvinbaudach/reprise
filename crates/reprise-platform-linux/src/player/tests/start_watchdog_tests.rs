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

use std::path::Path;

use super::segment_support::{cue_item, gain_element, write_regions_wav, Harness};
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
    assert!(
        events.iter().any(|event| matches!(
            event,
            PlayerEvent::Error(failure)
                if failure.kind() == PlaybackFailureKind::StartNeverFinished
        )),
        "the frontend words the give-up from its kind: {events:?}"
    );
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

/// The watchdog does not cover CUE starts, so nothing re-arms it when one
/// replaces a hung local start: the old watch has to be cancelled by the play.
#[test]
fn a_hung_start_that_a_cue_start_replaced_is_not_started_over_by_the_old_watch() {
    const CUE_TRACK: (i64, i64) = (500, 1_500);
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);

    // The CUE start hangs as well, in READY with PAUSED pending. Were the
    // first start's watch still running, it would find that and start over.
    harness
        .player
        .play(cue_item(Path::new(&path), CUE_TRACK, 0.0))
        .unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 2);
    let events = harness.pump_for(QUIET_FOR);

    assert_eq!(errors(&events), 0, "events: {events:?}");
    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "the first play's watch started the CUE start over"
    );
}

/// Makes the file source slow rather than stuck: each of its first `slowed`
/// reads takes `delay`, so the start stays in READY for a long while but the
/// source keeps moving. Returns how many file sources the pipeline created —
/// a start that is begun over builds a new one.
fn slow_source(player: &Player, slowed: usize, delay: Duration) -> Arc<AtomicUsize> {
    let sources = Arc::new(AtomicUsize::new(0));
    let created = sources.clone();
    let reads = Arc::new(AtomicUsize::new(0));
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .downcast::<gst::Bin>()
        .unwrap();
    playbin.connect("deep-element-added", false, move |values| {
        let element = values[2].get::<gst::Element>().unwrap();
        if element.factory().is_some_and(|f| f.name() == "filesrc") {
            created.fetch_add(1, Ordering::SeqCst);
            let reads = reads.clone();
            element.static_pad("src").unwrap().add_probe(
                gst::PadProbeType::PULL | gst::PadProbeType::BUFFER,
                move |_, _| {
                    if reads.fetch_add(1, Ordering::SeqCst) < slowed {
                        std::thread::sleep(delay);
                    }
                    gst::PadProbeReturn::Ok
                },
            );
        }
        None
    });
    sources
}

/// A slow disk or share keeps a healthy start in READY for longer than the
/// deadline. As long as the source delivers, that is a start in progress.
#[test]
fn a_slow_start_that_keeps_reading_is_left_alone() {
    const SLOW_READS: usize = 12;
    const READ_TIME: Duration = Duration::from_millis(100);
    // Each read is well inside the deadline; all of them together are far past it.
    let harness = harness_with_deadline(Duration::from_millis(400));
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let sources = slow_source(&harness.player, SLOW_READS, READ_TIME);

    harness.player.play(item(&path)).unwrap();
    let events = harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });

    assert_eq!(state_of(&harness.player).0, gst::State::Playing);
    assert_eq!(errors(&events), 0, "events: {events:?}");
    assert_eq!(
        sources.load(Ordering::SeqCst),
        1,
        "the slow start was begun over although its source kept reading"
    );
}

/// A disk that has spun down answers its first reads with long waits, not a
/// trickle. The source stands still for a whole deadline, so the first attempt
/// is started over — but the next one is allowed longer, and the track plays
/// instead of being skipped as unplayable.
#[test]
fn a_start_stuck_on_one_long_read_plays_after_a_restart_instead_of_being_skipped() {
    let deadline = Duration::from_millis(400);
    let harness = harness_with_deadline(deadline);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    // The first reads each outlast a deadline, as the ones from a cold disk do.
    slow_source(&harness.player, 3, deadline * 5 / 4);

    harness.player.play(item(&path)).unwrap();
    let events = harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });

    assert_eq!(state_of(&harness.player).0, gst::State::Playing);
    assert_eq!(errors(&events), 0, "events: {events:?}");
}

/// Control for the test above: a source that is stuck is still started over.
#[test]
fn a_start_whose_source_stopped_reading_is_still_started_over() {
    let harness = harness_with_deadline(TEST_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let sources = slow_source(&harness.player, 0, Duration::ZERO);
    let reached = hang_starts(&harness.player, 1);

    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });

    assert_eq!(reached.load(Ordering::SeqCst), 2);
    assert_eq!(sources.load(Ordering::SeqCst), 2, "one source per start");
}

/// Every start that is given up on is its own session, so the frontend's
/// first-cause gate cannot take a later give-up for a repeat of an earlier one.
#[test]
fn each_give_up_carries_the_session_of_its_own_start() {
    let harness = harness_with_deadline(TEST_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    hang_starts(&harness.player, usize::MAX);

    let mut sessions = Vec::new();
    for _ in 0..2 {
        harness.player.play(item(&path)).unwrap();
        let events = harness.pump_until(PATIENCE, |events| errors(events) > 0);
        sessions.extend(events.iter().filter_map(|event| match event {
            PlayerEvent::Error(failure) => Some(failure.session_id()),
            _ => None,
        }));
    }

    assert_eq!(sessions.len(), 2);
    assert!(
        sessions.iter().all(|&id| id != PlaybackSessionId::UNSCOPED),
        "{sessions:?}"
    );
    assert_ne!(sessions[0], sessions[1]);
}

/// A podcast resumes by seeking right after the start, and the frontend keeps
/// that seek pending until it is accepted. The player refuses a seek while the
/// start has not prerolled — a hung one included — and never queues it, so
/// nothing is lost when `Null` throws a hung start away: the seek is made again
/// once the new start has prerolled.
#[test]
fn a_seek_during_a_hung_start_is_refused_and_works_once_the_start_is_over() {
    const RESUME_MS: i64 = 1_000;
    let harness = harness_with_deadline(TEST_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_regions_wav(&path, &[(30_000, true)]);
    let reached = hang_starts(&harness.player, 1);

    harness.player.play(item(path.to_str().unwrap())).unwrap();
    assert!(
        harness.player.seek_to(RESUME_MS).is_err(),
        "a seek on a start that has not prerolled must be refused, so its caller keeps it"
    );
    harness.pump_until(PATIENCE, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });
    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "the start was begun over"
    );

    harness.player.seek_to(RESUME_MS).unwrap();
    harness.pump_for(Duration::from_millis(300));
    assert!(
        position_ms(&harness.player) >= RESUME_MS - 100,
        "plays from {} ms",
        position_ms(&harness.player)
    );
}

fn position_ms(player: &Player) -> i64 {
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    playbin
        .query_position::<gst::ClockTime>()
        .map_or(-1, |position| position.mseconds() as i64)
}
