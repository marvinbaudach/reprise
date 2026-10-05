//! A live ReplayGain change re-feeds the next track while a hand-off is already
//! under way. The re-fed gain must reach the hand-off that is in flight, not
//! only the slot a hand-off that has not started yet would read.

use super::*;
use crate::player_effects::linear_gain;
use crate::player_pipeline::{build_playbin, path_to_uri};

const STALE_GAIN_DB: f64 = -3.0;
const REFRESHED_GAIN_DB: f64 = 5.0;
const NEXT_PATH: &str = "/music/next.flac";

fn quiet_player() -> Player {
    Player::new(Box::new(|_| {})).unwrap()
}

fn next_item(gain_db: f64) -> PlaybackItem<'static> {
    PlaybackItem {
        path: NEXT_PATH,
        gain_db,
    }
}

fn pending(player: &Player) -> Option<f64> {
    *player
        .pending_gain
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn queued_uri(player: &Player) -> Option<String> {
    player
        .next_uri
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|queued| queued.uri.clone())
}

#[test]
fn play_20a_a_refed_gain_replaces_the_pending_gain_of_the_handoff_in_flight() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let player = quiet_player();
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .set_property("uri", path_to_uri(NEXT_PATH).unwrap());
    *player
        .pending_gain
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(STALE_GAIN_DB);

    player.set_next(Some(next_item(REFRESHED_GAIN_DB)));

    assert_eq!(pending(&player), Some(REFRESHED_GAIN_DB));
    assert_eq!(
        queued_uri(&player),
        None,
        "the track already handed off must not be queued a second time"
    );
}

#[test]
fn play_20a_a_different_next_track_leaves_the_pending_gain_alone() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let player = quiet_player();
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .set_property("uri", path_to_uri("/music/other.flac").unwrap());
    *player
        .pending_gain
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(STALE_GAIN_DB);

    player.set_next(Some(next_item(REFRESHED_GAIN_DB)));

    assert_eq!(pending(&player), Some(STALE_GAIN_DB));
    assert_eq!(queued_uri(&player), path_to_uri(NEXT_PATH).ok());
}

#[test]
fn play_20b_a_refed_gain_reaches_the_prebuilt_crossfade_secondary() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let player = quiet_player();
    let secondary = build_playbin(
        &AudioEffects::default(),
        player.next_uri.clone(),
        player.handoff_pending.clone(),
        player.transition.clone(),
        player.stream_generation.clone(),
        player.pending_gain.clone(),
    )
    .unwrap();
    secondary.set_property("uri", path_to_uri(NEXT_PATH).unwrap());
    crate::player_effects::set_playbin_track_gain(&secondary, STALE_GAIN_DB).unwrap();
    *player
        .incoming
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(secondary.clone());

    player.set_next(Some(next_item(REFRESHED_GAIN_DB)));

    let gain = secondary
        .property::<Option<gst::Element>>("audio-filter")
        .unwrap()
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-track-gain")
        .unwrap()
        .property::<f64>("volume");
    assert!((gain - linear_gain(REFRESHED_GAIN_DB)).abs() < 1e-6);
    assert_eq!(queued_uri(&player), None);
}
