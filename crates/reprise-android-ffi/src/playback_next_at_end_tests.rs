//! PLAY-8b: a manual Next on the last track of the play order with Repeat off
//! does nothing, while leaving a track on purpose (deleting it) still stops.

use super::test_support::{recording_session, PortCall, SessionFixture};
use super::AndroidPlaybackState;
use crate::AndroidRepeatMode;

fn play_three_tracks_from(start_index: u64) -> SessionFixture {
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
            start_index,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    fixture
}

#[test]
fn play_8b_next_on_the_last_track_with_repeat_off_is_a_no_op() {
    let fixture = play_three_tracks_from(2);
    let before = fixture.session.snapshot().unwrap();
    assert_eq!(before.current_index, Some(2));

    fixture.session.next().unwrap();

    let after = fixture.session.snapshot().unwrap();
    assert_eq!(after.state, before.state);
    assert_eq!(after.current_index, Some(2));
    assert_eq!(after.current_track_id, Some(30));
    assert_eq!(after.position_ms, before.position_ms);
    assert!(
        fixture.calls.lock().unwrap().is_empty(),
        "the backend must not be asked to stop or start: {:?}",
        fixture.calls.lock().unwrap(),
    );
}

#[test]
fn play_8b_next_on_the_last_track_with_repeat_all_wraps() {
    let fixture = play_three_tracks_from(2);
    fixture.session.set_repeat(AndroidRepeatMode::All).unwrap();

    fixture.session.next().unwrap();

    let after = fixture.session.snapshot().unwrap();
    assert_eq!(after.state, AndroidPlaybackState::Playing);
    assert_eq!(after.current_index, Some(0));
    assert_eq!(after.current_track_id, Some(10));
}

#[test]
fn play_8b_next_after_a_back_step_to_the_last_track_still_returns_through_history() {
    let fixture = play_three_tracks_from(1);
    fixture.session.next().unwrap();
    fixture.session.previous_in_queue_order().unwrap();
    fixture.session.previous().unwrap();
    let on_the_last_track = fixture.session.snapshot().unwrap();
    assert_eq!(on_the_last_track.current_track_id, Some(30));
    assert_eq!(on_the_last_track.current_index, Some(2));

    fixture.session.next().unwrap();

    let after = fixture.session.snapshot().unwrap();
    assert_eq!(
        after.current_track_id,
        Some(20),
        "PLAY-14: Next returns to the item the back-step left, even from the last track",
    );
    assert_eq!(after.state, AndroidPlaybackState::Playing);
}

#[test]
fn skip_current_or_stop_on_the_last_track_stops() {
    let fixture = play_three_tracks_from(2);

    fixture.session.skip_current_or_stop().unwrap();

    let after = fixture.session.snapshot().unwrap();
    assert_eq!(after.state, AndroidPlaybackState::Stopped);
    assert_eq!(after.current_index, None);
    assert_eq!(fixture.calls.lock().unwrap().as_slice(), &[PortCall::Stop]);
}

#[test]
fn skip_current_or_stop_before_the_end_advances() {
    let fixture = play_three_tracks_from(0);

    fixture.session.skip_current_or_stop().unwrap();

    let after = fixture.session.snapshot().unwrap();
    assert_eq!(after.state, AndroidPlaybackState::Playing);
    assert_eq!(after.current_track_id, Some(20));
}
