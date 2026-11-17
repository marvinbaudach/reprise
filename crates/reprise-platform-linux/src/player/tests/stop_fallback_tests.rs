//! A `stop` that cannot build an idle pipeline still stops.
//!
//! The fallback keeps the playing pipeline in the slot and tells it, on its own
//! queue, to go to `Null`. What the player says it was asked for has to follow:
//! a pipeline asked for `Playing` would let the next pause toggle bring the
//! stopped track back.

use super::segment_support::Harness;
use super::start_watchdog_tests::{current_playbin, local_track};
use super::*;
use crate::player::pipeline_queue::Asked;
use crate::player::pipeline_swap::test_seam;

#[test]
fn a_stop_that_cannot_build_an_idle_pipeline_leaves_a_pause_toggle_nothing_to_resume() {
    let harness = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = local_track(&dir);
    let player = &harness.player;
    player.play(item(&path)).unwrap();
    let playing = current_playbin(player);
    assert_eq!(
        player.playbin.lock().unwrap().asked(),
        Asked::Playing,
        "the start asked for Playing"
    );

    test_seam::fail_next_builds(1);
    player.stop().unwrap();

    // The control arm: the fallback ran, so the pipeline is still in the slot.
    assert_eq!(current_playbin(player), playing, "stop built an idle one");
    assert_eq!(
        player.playbin.lock().unwrap().asked(),
        Asked::Idle,
        "a stopped pipeline is still asked to play"
    );
    assert_eq!(
        player.toggle_pause().unwrap(),
        PlaybackState::Stopped,
        "a pause toggle after the stop resumed the stopped track"
    );
}
