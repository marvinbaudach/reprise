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
    let finished = queue.ask(move |playbin| gate.finish_start(playbin, &job_queue, None));
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
