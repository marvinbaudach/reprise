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
        let element = element.clone();
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
