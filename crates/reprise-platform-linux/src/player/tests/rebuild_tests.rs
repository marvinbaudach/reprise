//! A rebuilt playbin (the wedged-pipeline recovery in `Player::play`) must
//! carry what the user asked for, not the element's factory defaults.

use super::*;

/// Slack for comparing an `f64` that round-trips through a GObject property.
const VOLUME_EPSILON: f64 = 1e-9;

const USER_VOLUME: f64 = 0.3;

fn live_volume(player: &Player) -> f64 {
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .property::<f64>("volume")
}

#[test]
fn a_rebuilt_playbin_plays_at_the_users_volume() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    player.set_volume(USER_VOLUME);

    player.rebuild_playbin().unwrap();

    let volume = live_volume(&player);
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
    assert!(
        (volume - USER_VOLUME).abs() < VOLUME_EPSILON,
        "the rebuilt playbin must carry the user's volume, got {volume}"
    );
}

#[test]
fn a_failed_play_that_rebuilds_the_pipeline_keeps_the_users_volume() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    player.set_volume(USER_VOLUME);

    let missing_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/does-not-exist.flac"
    );
    let _ = player.play(item(missing_path));

    let volume = live_volume(&player);
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
    assert!(
        (volume - USER_VOLUME).abs() < VOLUME_EPSILON,
        "a play that rebuilt the pipeline must keep the user's volume, got {volume}"
    );
}
