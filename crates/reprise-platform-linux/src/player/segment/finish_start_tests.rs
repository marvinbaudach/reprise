//! The tail of a CUE start reports `Playing` only if that is still the state
//! the user asked for.
//!
//! `finish_start` enters `Playing` on the pipeline's queue, and a pause or a
//! stop can land while that state change runs: the transport has already told
//! the frontend, and its own job is queued behind this one. The interleaving is
//! forced from the state-changed message the state change posts on the calling
//! thread, so the user's action lands exactly inside it.

use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use super::*;
use crate::player::pipeline_queue::Pipeline;

const PATIENCE: Duration = Duration::from_secs(10);

/// What the user does while the state change into `Playing` runs.
#[derive(Clone, Copy)]
enum Interruption {
    None,
    Pause,
    Stop,
    /// A newer start (or a stop) has taken the gate, and has not retired this
    /// pipeline yet: only the epoch says the start is stale.
    Swap,
}

fn events_of(interruption: Interruption) -> Vec<PlayerEvent> {
    gst::init().unwrap();
    let (tx, events): (_, Receiver<PlayerEvent>) = channel();
    let gate = SegmentGate::new(
        Arc::new(move |event| {
            let _ = tx.send(event);
        }),
        Arc::new(AtomicU64::new(0)),
    );
    let element = gst::parse::launch("audiotestsrc ! fakesink").unwrap();
    // Prerolled in Paused, as a CUE start is when its preroll completes.
    element.set_state(gst::State::Paused).unwrap();
    element.state(gst::ClockTime::from_seconds(5)).0.unwrap();

    let pipeline = Pipeline::new(element.clone());
    let queue = pipeline.queue();
    queue.set_asked(Asked::Playing);
    let holder = Arc::new(Mutex::new(Some(pipeline)));

    let during = {
        let (queue, holder) = (queue.clone(), holder.clone());
        let (element, gate) = (element.clone(), gate.clone());
        move |message: &gst::Message| {
            let into_playing = matches!(
                message.view(),
                gst::MessageView::StateChanged(change)
                    if change.current() == gst::State::Playing
                        && message.src().is_some_and(|src| *src == element)
            );
            if !into_playing {
                return;
            }
            match interruption {
                Interruption::None => {}
                Interruption::Pause => queue.set_asked(Asked::Paused),
                Interruption::Stop => {
                    queue.set_asked(Asked::Idle);
                    holder.lock().unwrap().take();
                }
                Interruption::Swap => gate.reset(),
            }
        }
    };
    element.bus().unwrap().set_sync_handler(move |_, message| {
        during(message);
        gst::BusSyncReply::Pass
    });

    let job_queue = queue.clone();
    let finished = queue
        .ask(move |playbin| gate.finish_start(playbin, &job_queue, StartEpoch::default(), None));
    assert!(
        finished.wait(PATIENCE).is_some(),
        "finish_start never ended"
    );
    events.try_iter().collect()
}

fn playing(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
        .count()
}

#[test]
fn an_uninterrupted_start_reports_playing_once() {
    let events = events_of(Interruption::None);
    assert_eq!(playing(&events), 1, "events: {events:?}");
}

#[test]
fn a_pause_during_the_state_change_is_not_answered_with_playing() {
    let events = events_of(Interruption::Pause);
    assert_eq!(playing(&events), 0, "events: {events:?}");
}

#[test]
fn a_newer_start_during_the_state_change_is_not_answered_with_playing() {
    let events = events_of(Interruption::Swap);
    assert_eq!(playing(&events), 0, "events: {events:?}");
}

#[test]
fn a_stop_during_the_state_change_is_not_answered_with_playing() {
    let events = events_of(Interruption::Stop);
    assert_eq!(playing(&events), 0, "events: {events:?}");
}

/// A pipeline already taken out of service, with a way into its queue and the
/// element a late job of it would still be handed. The element is in `Null`,
/// so a seek on it is refused.
fn retired_pipeline() -> (gst::Element, QueueHandle) {
    gst::init().unwrap();
    let element = gst::parse::launch("audiotestsrc ! fakesink").unwrap();
    let pipeline = Pipeline::new(element.clone());
    let queue = pipeline.queue();
    pipeline.retire();
    assert!(queue.is_retired());
    (element, queue)
}

/// A gate holding what a newer CUE start has installed: its cut, and its
/// start-seek still to come.
fn gate_with_a_newer_start() -> Arc<SegmentGate> {
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///newer.flac", (5_000, 9_000));
    gate
}

#[test]
fn a_retired_starts_refused_seek_leaves_a_newer_starts_pending_state_intact() {
    let (element, queue) = retired_pipeline();
    let gate = gate_with_a_newer_start();

    // The late job of the retired pipeline: its seek to the old track's start
    // is refused, and it must not clear the newer start's pending seek.
    let epoch = gate.lock().epoch;
    gate.finish_start(&element, &queue, epoch, Some(1_000));

    assert_eq!(
        gate.lock().pending_start_ms,
        Some(5_000),
        "a retired pipeline's late refusal wiped the newer start's pending seek"
    );
}

#[test]
fn a_retired_pipelines_refused_seek_leaves_the_newer_streams_seek_in_flight() {
    let (_element, queue) = retired_pipeline();
    let gate = gate_with_a_newer_start();
    gate.lock().seek_in_flight = true;

    let epoch = gate.lock().epoch;
    gate.seek_refused(&queue, epoch);

    assert!(
        gate.lock().seek_in_flight,
        "a retired pipeline's late refusal ended the newer stream's seek wait"
    );
}

#[test]
fn a_current_pipelines_refused_seek_ends_its_wait() {
    gst::init().unwrap();
    let element = gst::parse::launch("audiotestsrc ! fakesink").unwrap();
    let pipeline = Pipeline::new(element);
    let gate = gate_with_a_newer_start();
    gate.lock().seek_in_flight = true;

    let epoch = gate.lock().epoch;
    gate.seek_refused(&pipeline.queue(), epoch);

    assert!(!gate.lock().seek_in_flight);
}

/// A pipeline that has not been retired, whose seek is refused (`Null`).
fn live_pipeline_in_null() -> (gst::Element, Pipeline) {
    gst::init().unwrap();
    let element = gst::parse::launch("audiotestsrc ! fakesink").unwrap();
    let pipeline = Pipeline::new(element.clone());
    (element, pipeline)
}

#[test]
fn an_outgoing_starts_refused_seek_leaves_the_new_starts_pending_state_intact() {
    // The window of #1341: the new start has written the gate, and the pipeline
    // it replaces has not been retired yet, so only the gate itself can tell
    // the refusal is not the new start's.
    let (element, outgoing) = live_pipeline_in_null();
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///old.flac", (1_000, 4_000));
    let outgoing_epoch = gate.lock().epoch;
    gate.begin_start("file:///new.flac", (5_000, 9_000));
    assert!(!outgoing.queue().is_retired());

    gate.finish_start(&element, &outgoing.queue(), outgoing_epoch, Some(1_000));

    assert_eq!(
        gate.lock().pending_start_ms,
        Some(5_000),
        "the outgoing pipeline's late refusal cleared the new start's pending seek"
    );
}

#[test]
fn an_outgoing_streams_refused_seek_leaves_the_new_streams_seek_in_flight() {
    let (_element, outgoing) = live_pipeline_in_null();
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///old.flac", (1_000, 4_000));
    let outgoing_epoch = gate.lock().epoch;
    gate.begin_start("file:///new.flac", (5_000, 9_000));
    gate.lock().seek_in_flight = true;
    assert!(!outgoing.queue().is_retired());

    gate.seek_refused(&outgoing.queue(), outgoing_epoch);

    assert!(
        gate.lock().seek_in_flight,
        "the outgoing pipeline's late refusal ended the new stream's seek wait"
    );
}

#[test]
fn the_current_starts_refused_seek_still_clears_its_own_pending_state() {
    let (element, current) = live_pipeline_in_null();
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///current.flac", (5_000, 9_000));
    let epoch = gate.lock().epoch;

    gate.finish_start(&element, &current.queue(), epoch, Some(5_000));

    assert_eq!(gate.lock().pending_start_ms, None);
}

/// A pipeline that has prerolled in `Paused`, as a CUE start's file has when
/// its `ASYNC_DONE` is handled.
fn prerolled_pipeline() -> Pipeline {
    gst::init().unwrap();
    let element = gst::parse::launch("audiotestsrc ! fakesink").unwrap();
    element.set_state(gst::State::Paused).unwrap();
    element.state(gst::ClockTime::from_seconds(5)).0.unwrap();
    Pipeline::new(element)
}

#[test]
fn a_retired_pipelines_late_preroll_leaves_the_newer_starts_seek_to_itself() {
    // A start over takes the gate's cut from the hung pipeline to its
    // replacement; a late `ASYNC_DONE` of the retired one must not make the
    // replacement believe its start-seek is in flight and skip it.
    let retired = prerolled_pipeline();
    let element = retired.element().clone();
    let queue = retired.queue();
    // A job still running keeps the queue from taking the element to `Null`, so
    // it stays prerolled while the pipeline is retired, as a hung one does.
    let (release, released) = std::sync::mpsc::channel::<()>();
    retired.submit(move |_| {
        let _ = released.recv();
    });
    retired.retire();
    assert!(queue.is_retired());
    let gate = gate_with_a_newer_start();

    gate.complete_start(&element, &queue);
    let _ = release.send(());

    let state = gate.lock();
    assert_eq!(
        state.start_seek_ms, None,
        "the late preroll took the start-seek"
    );
    assert_eq!(state.pending_start_ms, Some(5_000));
}

#[test]
fn begin_start_never_shows_a_cut_whose_start_is_not_pending_yet() {
    // A seek that lands while the gate is written must be deferred to the
    // start-seek. Racy by nature: with the two-step write it fails within a few
    // thousand rounds on a multi-core machine; with the single write it can
    // never fail.
    const ROUNDS: usize = 50_000;
    let gate = gate_with_a_newer_start();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seeker = {
        let (gate, done) = (gate.clone(), done.clone());
        std::thread::spawn(move || {
            let mut immediate = 0_usize;
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                if matches!(gate.seek_target_ms(100), Some(CutSeek::Now(..))) {
                    immediate += 1;
                }
            }
            immediate
        })
    };
    for _ in 0..ROUNDS {
        gate.begin_start("file:///again.flac", (5_000, 9_000));
    }
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let immediate = seeker.join().unwrap();

    assert_eq!(
        immediate, 0,
        "a seek found the new cut with no start pending"
    );
}

#[test]
fn the_epoch_wraps_instead_of_overflowing() {
    let last = StartEpoch(u64::MAX);
    assert_eq!(last.next(), StartEpoch(0));
    assert_ne!(last.next(), last);
}

#[test]
fn a_reset_makes_every_job_queued_before_it_stale() {
    // `stop`, and a start that failed, reset the gate: what a pipeline of the
    // stream they ended reports afterwards is nothing of the next one's.
    let (element, outgoing) = live_pipeline_in_null();
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///old.flac", (1_000, 4_000));
    let outgoing_epoch = gate.lock().epoch;
    gate.reset();
    assert_ne!(gate.lock().epoch, outgoing_epoch, "a reset kept the epoch");
    gate.begin("file:///new.flac", (5_000, 9_000), None);
    {
        let mut state = gate.lock();
        state.pending_start_ms = Some(5_000);
        state.seek_in_flight = true;
    }

    gate.finish_start(&element, &outgoing.queue(), outgoing_epoch, Some(1_000));
    gate.seek_refused(&outgoing.queue(), outgoing_epoch);

    let state = gate.lock();
    assert_eq!(state.pending_start_ms, Some(5_000));
    assert!(state.seek_in_flight);
}

#[test]
fn a_reset_leaves_the_gate_at_its_default_but_for_the_epoch() {
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin_start("file:///old.flac", (1_000, 4_000));
    gate.reset();
    let state = gate.lock();
    assert!(state.active.is_none());
    assert_eq!(state.pending_start_ms, None);
    assert_eq!(state.start_seek_ms, None);
    assert!(!state.seek_in_flight);
}

#[test]
fn a_seek_asked_for_carries_the_gates_epoch() {
    let gate = SegmentGate::new(Arc::new(|_| {}), Arc::new(AtomicU64::new(0)));
    gate.begin("file:///playing.flac", (1_000, 4_000), Some(60_000));
    let epoch = gate.lock().epoch;
    assert_eq!(gate.seek_target_ms(500), Some(CutSeek::Now(1_500, epoch)));
}
