//! Leaving the current track on purpose advances, or stops at the end.

use super::test_support::{recording_session, PortCall, SessionFixture};
use super::AndroidPlaybackState;

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
