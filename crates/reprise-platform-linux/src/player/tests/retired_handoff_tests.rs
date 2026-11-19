//! A pipeline that has been retired takes no part in the gapless hand-off.
//!
//! Every pipeline carries the `about-to-finish` handler, and a retired one
//! keeps it until its `Null` lands, which can be a long time when its queue
//! thread is stuck in a job. The successor the frontend feeds after a skip is
//! fed for the pipeline that replaced it; the retired one must neither take it
//! nor touch the shared hand-off state.

use super::segment_support::Harness;
use super::start_watchdog_tests::{current_playbin, local_track};
use super::*;
use crate::gapless::QueuedTrack;

const SUCCESSOR_URI: &str = "file:///music/successor.flac";
const SUCCESSOR_GAIN_DB: f64 = -4.5;

fn feed_successor(player: &Player) {
    *player
        .next_uri
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(QueuedTrack {
        uri: SUCCESSOR_URI.into(),
        gain_db: SUCCESSOR_GAIN_DB,
        segment: None,
    });
}

fn about_to_finish(element: &gst::Element) {
    element.emit_by_name::<()>("about-to-finish", &[]);
}

fn fed_uri(player: &Player) -> Option<String> {
    player
        .next_uri
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|queued| queued.uri.clone())
}

fn pending_gain(player: &Player) -> Option<f64> {
    *player
        .pending_gain
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn a_retired_pipeline_leaves_the_successor_fed_for_its_replacement_alone() {
    let harness = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let player = &harness.player;
    player.play(item(&path)).unwrap();
    let retired = current_playbin(player);
    // The skip: `stop` retires the pipeline, and the frontend feeds the track
    // that follows what it plays next.
    player.stop().unwrap();
    let current = current_playbin(player);
    assert_ne!(
        current, retired,
        "stop left the retired pipeline in the slot"
    );
    feed_successor(player);

    about_to_finish(&retired);

    assert_eq!(
        fed_uri(player).as_deref(),
        Some(SUCCESSOR_URI),
        "a retired pipeline took the successor"
    );
    assert_eq!(
        pending_gain(player),
        None,
        "a retired pipeline set the gain"
    );
    assert!(
        !player.handoff_pending.load(Ordering::SeqCst),
        "a retired pipeline staged a hand-off"
    );

    // The control arm: the same signal on the pipeline in service is taken, so
    // the silence above is the retirement's and not a signal that never
    // reached the handler.
    about_to_finish(&current);

    assert_eq!(fed_uri(player), None);
    assert_eq!(pending_gain(player), Some(SUCCESSOR_GAIN_DB));
    assert!(player.handoff_pending.load(Ordering::SeqCst));
}
