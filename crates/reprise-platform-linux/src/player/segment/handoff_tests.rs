//! The staged hand-off's own decisions, driven straight through the gate
//! without a pipeline (PLAY-23a): the cases a real pipeline only reaches by a
//! race — an end-of-stream overtaking the watcher, a buffer from before a seek
//! staging the hand-off again before the flush lands.

use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{channel, Receiver};

use super::super::{BoundaryVerdict, SegmentGate, SegmentHandle};
use super::*;
use crate::gapless::QueuedTrack;
use crate::player_effects::linear_gain;

const URI: &str = "file:///album.flac";
const FIRST: (i64, i64) = (1_000, 3_000);
const SECOND: (i64, i64) = (3_000, 5_000);
const FILE_MS: i64 = 10_000;
const SECOND_GAIN_DB: f64 = 6.0;
const OUTGOING_GAIN: f64 = 1.0;

fn gate() -> (SegmentHandle, Receiver<PlayerEvent>) {
    let (tx, events) = channel();
    let gate = SegmentGate::new(
        Arc::new(move |event| {
            let _ = tx.send(event);
        }),
        Arc::new(AtomicU64::new(0)),
    );
    (gate, events)
}

fn gain() -> gst::Element {
    gst::init().unwrap();
    gst::ElementFactory::make("volume").build().unwrap()
}

fn next(segment: (i64, i64)) -> QueuedTrack {
    QueuedTrack {
        uri: URI.to_owned(),
        gain_db: SECOND_GAIN_DB,
        segment: Some(segment),
    }
}

fn at(ms: u64) -> Option<gst::ClockTime> {
    Some(gst::ClockTime::from_mseconds(ms))
}

/// Plays the first track with the second armed and feeds the boundary buffer.
fn staged(gate: &SegmentGate, gain: &gst::Element) -> u64 {
    gate.begin(URI, FIRST, Some(FILE_MS));
    gate.route_next(Some(&next(SECOND)));
    match gate.on_buffer(at(SECOND.0 as u64), gain) {
        BoundaryVerdict::HandOff(epoch) => epoch,
        _ => panic!("the boundary buffer must stage the hand-off"),
    }
}

fn advances(events: &Receiver<PlayerEvent>) -> usize {
    events
        .try_iter()
        .filter(|event| matches!(event, PlayerEvent::AdvancedToNext))
        .count()
}

fn tick(gate: &SegmentGate, file_position_ms: i64) -> (i64, i64) {
    gate.send_tick(file_position_ms, FILE_MS, || (0, 0))
}

#[test]
fn play_23a_an_end_of_stream_announces_a_staged_hand_off_first_and_once() {
    let (gate, events) = gate();
    let gain = gain();
    staged(&gate, &gain);

    assert_eq!(advances(&events), 0, "staging announces nothing");
    assert_eq!(
        tick(&gate, 2_990),
        (1_990, 2_000),
        "until the boundary is heard the outgoing track's clock runs on"
    );
    let volume = gain.property::<f64>("volume");
    assert!(
        (volume - linear_gain(SECOND_GAIN_DB)).abs() < 1e-6,
        "the next track's gain applies from the boundary buffer on, got {volume}"
    );

    gate.complete_pending_handoff();
    assert_eq!(advances(&events), 1);
    assert_eq!(tick(&gate, 3_010), (10, 2_000));
    gate.complete_pending_handoff();
    assert_eq!(advances(&events), 0, "a hand-off is announced only once");
}

#[test]
fn play_23a_a_hand_off_staged_before_a_seek_lands_is_never_announced() {
    let (gate, events) = gate();
    let gain = gain();
    staged(&gate, &gain);

    assert!(matches!(
        gate.seek_target_ms(500),
        Some(super::super::CutSeek::Now(1_500))
    ));
    // A buffer already on its way before the flush stages the hand-off again.
    let BoundaryVerdict::HandOff(restaged) = gate.on_buffer(at(3_020), &gain) else {
        panic!("a buffer past the boundary stages the re-armed hand-off");
    };
    assert!(
        !gate.complete_handoff(restaged),
        "the watcher must keep waiting while the seek is in flight"
    );
    assert_eq!(advances(&events), 0);

    gate.seek_landed();
    assert!(matches!(
        gate.on_buffer(at(1_500), &gain),
        BoundaryVerdict::Pass
    ));
    assert!(gate.complete_handoff(restaged), "the hand-off is gone");
    assert_eq!(advances(&events), 0, "the seek kept the outgoing track");
    assert!((gain.property::<f64>("volume") - OUTGOING_GAIN).abs() < 1e-9);
    assert_eq!(tick(&gate, 1_500), (500, 2_000));
}

#[test]
fn play_23a_a_different_next_track_lets_the_outgoing_track_end_at_the_probe() {
    let (gate, events) = gate();
    let gain = gain();
    let epoch = staged(&gate, &gain);

    gate.route_next(Some(&next((6_000, 7_000))));

    assert!(matches!(
        gate.on_buffer(at(3_040), &gain),
        BoundaryVerdict::EndOfTrack
    ));
    assert!(gate.complete_handoff(epoch));
    gate.complete_pending_handoff();
    assert_eq!(
        advances(&events),
        0,
        "the withdrawn hand-off is never announced"
    );
}
