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

use super::segment_support::{
    cue_item, encode_flac, gain_of, on_every_pipeline, record_heard, write_regions_wav, Harness,
};
use super::*;
use crate::player::start_watchdog::allowed_idle;

/// The watchdog's deadline where the test only waits for it.
const TEST_DEADLINE: Duration = Duration::from_millis(150);
/// The deadline where the test acts on the hung start first: it has to land
/// before the watchdog does, so it gets room to do so on a busy machine.
pub(super) const ROOMY_DEADLINE: Duration = Duration::from_millis(1_000);
/// Far more than the attempts need, and far less than a hang would take.
pub(super) const PATIENCE: Duration = Duration::from_secs(20);
/// Long enough for several more deadlines to pass, for the "nothing happens" arms.
pub(super) const QUIET_FOR: Duration = Duration::from_millis(2_500);
/// Three starts in all are two starts over. Counted by the watchdog rather
/// than by the starts that reached the probe: on a loaded machine the last
/// start can still be on its way to the probe, inside its short test deadline,
/// when the watchdog gives up on it.
const RESTARTS_BEFORE_GIVING_UP: u64 = 2;
const TRACK_MS: u32 = 2_000;
/// Where the tone of the CUE fixture starts, and where its CUE track does.
const CUE_START_MS: i64 = 2_000;
const CUE_END_MS: i64 = 4_000;
/// How far the first sample heard may lie from the CUE track's start (the
/// tolerance of `segment_clock_tests`).
const CUE_START_TOLERANCE_MS: i64 = 20;
/// The preroll at the file's start, then the seek to the track's start.
const SEEKS_AFTER_START: usize = 2;

/// Blocks the first `hang_starts` starts at the gain element's sink pad and
/// lets every later one through. Returns how many starts reached the pad.
pub(super) fn hang_starts(player: &Player, hang_starts: usize) -> Arc<AtomicUsize> {
    let reached = Arc::new(AtomicUsize::new(0));
    let counted = reached.clone();
    // On every pipeline a start over builds as well: the count is of starts.
    on_every_pipeline(player, move |playbin| {
        let counted = counted.clone();
        gain_of(playbin).static_pad("sink").unwrap().add_probe(
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
    });
    reached
}

pub(super) fn local_track(dir: &tempfile::TempDir) -> String {
    let path = dir.path().join("track.wav");
    write_regions_wav(&path, &[(TRACK_MS, true)]);
    path.to_str().unwrap().to_owned()
}

pub(super) fn errors(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::Error(_)))
        .count()
}

pub(super) fn state_of(player: &Player) -> (gst::State, gst::State) {
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let (_, current, pending) = playbin.state(gst::ClockTime::ZERO);
    (current, pending)
}

pub(super) fn harness_with_deadline(deadline: Duration) -> Harness {
    let mut harness = Harness::new();
    harness.player.start_watchdog.deadline = deadline;
    harness
}

/// After the give-up: exactly three starts were made, the two starts over
/// really reached the pipeline's hang probe, and nothing more happens.
///
/// `restarts` is counted when the watchdog begins a start over, which does not
/// prove the start over built a pipeline that carried data to the probe, so
/// that is checked as well, as a lower bound: the last start can still be on
/// its way to the probe when the watchdog gives up on it, which is why only the
/// two earlier starts are required, and why the test waits for them.
fn assert_three_starts_then_quiet(harness: &Harness, reached: &AtomicUsize) {
    assert_eq!(
        harness.player.start_watchdog.restarts(),
        RESTARTS_BEFORE_GIVING_UP,
        "three starts, no fourth"
    );
    harness.pump_until(PATIENCE, |_| {
        reached.load(Ordering::SeqCst) >= RESTARTS_BEFORE_GIVING_UP as usize
    });
    let reached_at_give_up = reached.load(Ordering::SeqCst);
    assert!(
        reached_at_give_up >= RESTARTS_BEFORE_GIVING_UP as usize,
        "a start over must reach the stream again: only {reached_at_give_up} start(s) did"
    );

    let later = harness.pump_for(QUIET_FOR);
    assert_eq!(errors(&later), 0, "the error is reported once");
    assert_eq!(
        harness.player.start_watchdog.restarts(),
        RESTARTS_BEFORE_GIVING_UP
    );
    assert_eq!(reached.load(Ordering::SeqCst), reached_at_give_up);
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
    assert_three_starts_then_quiet(&harness, &reached);
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

/// A start the watchdog does not cover — a live stream — arms nothing, so no
/// later `arm` can cancel the hung local start's watch before it. Two things
/// then keep that watch from starting the live stream over: the play's
/// `disarm`, and the watch's own check that the stream is still the one it was
/// armed for (every play bumps the generation). Either alone is enough, so this
/// test fails only when both are gone; with one of them removed it still
/// passes, and each is a safe backstop for the other. The restart's own checks
/// under the lock, and the tests that need each one alone, are in
/// `start_watchdog_restart_tests`.
#[test]
fn a_hung_start_that_a_live_start_replaced_is_not_started_over_by_the_old_watch() {
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let live_uri = format!("file://{path}");
    let reached = hang_starts(&harness.player, usize::MAX);
    harness.player.play(item(&path)).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);

    // The live start hangs as well, in READY with PLAYING pending. Were the
    // first start's watch still running, it would find that and start over.
    harness.player.play_live_uri(&live_uri).unwrap();
    harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 2);
    let events = harness.pump_for(QUIET_FOR);

    assert_eq!(errors(&events), 0, "events: {events:?}");
    assert_eq!(
        harness.player.start_watchdog.restarts(),
        0,
        "the first play's watch started the live start over"
    );
    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "the first play's watch started the live start over"
    );
}

/// Silence, the track's tone, silence, as a FLAC: the tone starts exactly at
/// the CUE track's start, so the first buffer heard proves where it began.
fn cue_flac(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let wav = dir.path().join("album.wav");
    let flac = dir.path().join("album.flac");
    write_regions_wav(&wav, &[(2_000, false), (2_000, true), (2_000, false)]);
    encode_flac(&wav, &flac);
    flac
}

fn playing(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
        .count()
}

/// A CUE start is begun over like a whole file's, and the gate's pending start
/// survives the `Null`: the next preroll seeks to the segment start and plays.
#[test]
fn play_26_a_hung_cue_start_is_started_over_and_plays_from_its_segment_start() {
    // Roomy: the start over is counted exactly, so a second start that is only
    // slow under load must not be begun over again.
    let harness = harness_with_deadline(ROOMY_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let flac = cue_flac(&dir);
    let reached = hang_starts(&harness.player, 1);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&flac, (CUE_START_MS, CUE_END_MS), 0.0))
        .unwrap();
    // The control arm: the first start really is the hang the watchdog is for.
    let mut events = harness.pump_until(PATIENCE, |_| reached.load(Ordering::SeqCst) >= 1);
    assert_eq!(playing(&events), 0, "a hung start has not played");

    // Nothing but the watchdog can end it: the pump just runs the main context.
    events.extend(harness.pump_until(PATIENCE, |collected| {
        playing(collected) > 0 && heard.landings() >= SEEKS_AFTER_START
    }));

    assert_eq!(
        reached.load(Ordering::SeqCst),
        2,
        "the watchdog must start the hung CUE track over once (events: {events:?})"
    );
    assert!(playing(&events) > 0, "the start must finish: {events:?}");
    let first = heard.latest_landing();
    assert!(
        (first.start_ms - CUE_START_MS).abs() <= CUE_START_TOLERANCE_MS,
        "the first sample heard must be the track's own start, got {first:?}"
    );
    assert!(first.audible, "the first buffer heard must carry the tone");
    assert_eq!(errors(&events), 0, "a recovered start reports nothing");
}

#[test]
fn play_26_a_cue_start_that_never_finishes_is_given_up_after_three_attempts() {
    let harness = harness_with_deadline(TEST_DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let flac = cue_flac(&dir);
    let reached = hang_starts(&harness.player, usize::MAX);

    harness
        .player
        .play(cue_item(&flac, (CUE_START_MS, CUE_END_MS), 0.0))
        .unwrap();
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
    assert_three_starts_then_quiet(&harness, &reached);
}

/// Makes the file source slow rather than stuck: each of its first `slowed`
/// reads takes `delay`, so the start stays in READY for a long while but the
/// source keeps moving. Returns how many file sources the pipeline created —
/// a start that is begun over builds a new one.
fn slow_source(player: &Player, slowed: usize, delay: Duration) -> Arc<AtomicUsize> {
    let sources = Arc::new(AtomicUsize::new(0));
    let created = sources.clone();
    let reads = Arc::new(AtomicUsize::new(0));
    // The handler goes on every pipeline a start over builds, and the read
    // count runs across all of them.
    on_every_pipeline(player, move |playbin| {
        let created = created.clone();
        let reads = reads.clone();
        let Ok(bin) = playbin.clone().downcast::<gst::Bin>() else {
            return;
        };
        bin.connect("deep-element-added", false, move |values| {
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
///
/// The injected hang is one start, so one start over. On a loaded machine the
/// start over can hang for real, the very thing the watchdog is for, and is
/// begun over once more; that adds a source this test is not about. Such a run
/// is made again, but only on positive evidence: two starts over, and a run
/// that took at least the whole standstill both attempts are allowed. A watchdog
/// that fires too early, or twice for one hang, cannot have that, so it fails
/// here. A run with no start over fails at once, and so does every other count.
#[test]
fn a_start_whose_source_stopped_reading_is_still_started_over() {
    const REAL_HANG_RETRIES: usize = 5;
    // The first start over comes after one deadline of standstill, the second
    // after two more: a second hang that is real cannot be answered sooner.
    let both_standstills = allowed_idle(ROOMY_DEADLINE, 1) + allowed_idle(ROOMY_DEADLINE, 2);
    for attempt in 1..=REAL_HANG_RETRIES {
        // Roomy: a second start that is merely slow under load must not be
        // begun over again either.
        let harness = harness_with_deadline(ROOMY_DEADLINE);
        let dir = tempfile::tempdir().unwrap();
        let path = local_track(&dir);
        let sources = slow_source(&harness.player, 0, Duration::ZERO);
        let reached = hang_starts(&harness.player, 1);

        let began = std::time::Instant::now();
        harness.player.play(item(&path)).unwrap();
        harness.pump_until(PATIENCE, |_| {
            state_of(&harness.player).0 == gst::State::Playing
        });
        let took = began.elapsed();

        assert_eq!(
            state_of(&harness.player).0,
            gst::State::Playing,
            "the watchdog must start the stuck source over"
        );
        let restarts = harness.player.start_watchdog.restarts();
        if restarts == 2 && took >= both_standstills {
            eprintln!(
                "a real hang on attempt {attempt}: the start over stood still too, after {took:?}; trying again"
            );
            continue;
        }
        assert_eq!(
            restarts, 1,
            "one injected hang is one start over (took {took:?}, of {both_standstills:?} for two)"
        );
        assert_eq!(reached.load(Ordering::SeqCst), 2);
        assert_eq!(sources.load(Ordering::SeqCst), 2, "one source per start");
        return;
    }
    panic!("the start over hung for real on all {REAL_HANG_RETRIES} runs");
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
