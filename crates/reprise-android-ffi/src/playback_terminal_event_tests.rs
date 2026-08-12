use super::test_support::recording_session;
use crate::playback::{AndroidPlaybackState, AndroidPlayerEvent};

#[test]
fn buffering_from_the_finished_stream_cannot_revive_a_stopped_snapshot() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(vec![7], vec!["content://provider/song.flac".to_owned()], 0)
        .unwrap();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(23, AndroidPlayerEvent::TrackFinished);
    bridge.emit(
        23,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Buffering,
        },
    );

    assert_eq!(
        fixture.session.snapshot().unwrap().state,
        AndroidPlaybackState::Stopped,
    );
}

#[test]
fn buffering_from_the_failed_stream_cannot_revive_a_stopped_snapshot() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(vec![7], vec!["content://provider/song.flac".to_owned()], 0)
        .unwrap();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(
        23,
        AndroidPlayerEvent::Error {
            message: "decoder failed".to_owned(),
        },
    );
    bridge.emit(
        23,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Buffering,
        },
    );

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Stopped);
    assert_eq!(snapshot.error.as_deref(), Some("decoder failed"));
}
