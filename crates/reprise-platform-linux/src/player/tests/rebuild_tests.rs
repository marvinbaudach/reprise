//! A start after a failed start plays, and at the user's volume: every start is
//! a fresh pipeline (see `Player::play`), carrying what the user asked for, not
//! the element's factory defaults.

use super::*;

/// Slack for comparing an `f64` that round-trips through a GObject property.
const VOLUME_EPSILON: f64 = 1e-9;

const USER_VOLUME: f64 = 0.3;

fn live_volume(player: &Player) -> f64 {
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .element()
        .property::<f64>("volume")
}

const MISSING: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/does-not-exist.flac"
);
const VALID: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");

#[test]
fn a_fresh_pipeline_for_a_start_plays_at_the_users_volume() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    player.set_volume(USER_VOLUME);

    player.play(item(VALID)).unwrap();

    let volume = live_volume(&player);
    let _ = player.stop();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
    assert!(
        (volume - USER_VOLUME).abs() < VOLUME_EPSILON,
        "the pipeline of a start must carry the user's volume, got {volume}"
    );
}

#[test]
fn a_start_after_a_failed_start_plays_at_the_users_volume() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    player.set_volume(USER_VOLUME);

    assert!(player.play(item(MISSING)).is_err());
    let after_failure = live_volume(&player);
    player
        .play(item(VALID))
        .expect("a valid file must play after a failed start");
    let volume = live_volume(&player);
    let _ = player.stop();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
    assert!(
        (after_failure - USER_VOLUME).abs() < VOLUME_EPSILON,
        "the idle pipeline left by a failed start must carry the user's volume, got {after_failure}"
    );
    assert!(
        (volume - USER_VOLUME).abs() < VOLUME_EPSILON,
        "the start after a failed start must carry the user's volume, got {volume}"
    );
}
