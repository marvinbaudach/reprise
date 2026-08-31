use super::test_support::{recording_session, PortCall, SessionFixture};
use super::AndroidPlayerEvent;

fn play_three_tracks() -> SessionFixture {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 20, 30],
            vec![
                "content://track/10".into(),
                "content://track/20".into(),
                "content://track/30".into(),
            ],
            0,
        )
        .unwrap();
    fixture
}

#[test]
fn queue_order_previous_and_history_previous_diverge_after_a_direct_jump() {
    let queue_order = play_three_tracks();
    queue_order
        .session
        .play_tracks(
            vec![10, 20, 30],
            vec![
                "content://track/10".into(),
                "content://track/20".into(),
                "content://track/30".into(),
            ],
            2,
        )
        .unwrap();
    queue_order.session.previous_in_queue_order().unwrap();

    let history = play_three_tracks();
    history
        .session
        .play_tracks(
            vec![10, 20, 30],
            vec![
                "content://track/10".into(),
                "content://track/20".into(),
                "content://track/30".into(),
            ],
            2,
        )
        .unwrap();
    history.session.previous().unwrap();

    assert_eq!(
        queue_order.session.snapshot().unwrap().current_track_id,
        Some(20)
    );
    assert_eq!(
        history.session.snapshot().unwrap().current_track_id,
        Some(10)
    );
}

#[test]
fn play_14_previous_returns_to_what_actually_played_under_shuffle() {
    let fixture = play_three_tracks();
    fixture.session.set_shuffle(true).unwrap();
    fixture.session.next().unwrap();
    let first = fixture.session.snapshot().unwrap().current_track_id;
    fixture.session.next().unwrap();
    let second = fixture.session.snapshot().unwrap().current_track_id;
    assert_ne!(first, second);

    fixture.session.previous().unwrap();

    assert_eq!(fixture.session.snapshot().unwrap().current_track_id, first);
}

#[test]
fn play_14_previous_within_three_seconds_steps_back_but_later_seeks_to_zero() {
    let fixture = play_three_tracks();
    fixture.session.next().unwrap();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();
    bridge.emit(
        24,
        AndroidPlayerEvent::Position {
            position_ms: 2_000,
            duration_ms: 60_000,
        },
    );
    fixture.session.previous().unwrap();
    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(10)
    );

    fixture.session.next().unwrap();
    bridge.emit(
        24,
        AndroidPlayerEvent::Position {
            position_ms: 4_000,
            duration_ms: 60_000,
        },
    );
    fixture.calls.lock().unwrap().clear();
    fixture.session.previous().unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(20)
    );
    assert_eq!(fixture.session.snapshot().unwrap().position_ms, 0);
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SeekTo(0)]
    );
}

#[test]
fn play_14_previous_with_an_empty_history_does_not_restart_the_backend() {
    let fixture = play_three_tracks();
    fixture.calls.lock().unwrap().clear();

    fixture.session.previous().unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(10)
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SeekTo(0)]
    );
}

#[test]
fn play_14_next_after_a_back_step_returns_to_the_track_it_left() {
    let fixture = play_three_tracks();
    fixture.session.next().unwrap();
    fixture.session.next().unwrap();
    fixture.session.previous().unwrap();
    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(20)
    );

    fixture.session.next().unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(30)
    );
}

#[test]
fn play_14_history_survives_a_context_replacement() {
    let fixture = play_three_tracks();
    fixture
        .session
        .play_tracks(vec![90], vec!["content://track/90".into()], 0)
        .unwrap();

    fixture.session.previous().unwrap();

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.current_track_id, Some(10));
    assert_eq!(
        snapshot.current_track_uri.as_deref(),
        Some("content://track/10")
    );
    assert_eq!(
        snapshot.current_index, None,
        "the replaced context did not move"
    );
}

#[test]
fn play_14_a_gapless_handoff_is_recorded_as_playback() {
    let fixture = play_three_tracks();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();
    bridge.emit(24, AndroidPlayerEvent::AdvancedToNext);
    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(20)
    );

    fixture.session.previous().unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap().current_track_id,
        Some(10)
    );
}
