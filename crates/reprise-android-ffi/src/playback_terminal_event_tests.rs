use super::test_support::{
    recording_session, PortCall, FAILING_PLAY_URI, SYNCHRONOUS_BUFFERING_URI,
};
use crate::playback::{AndroidPlaybackState, AndroidPlayerEvent};
use crate::AndroidRepeatMode;

// UX FB-6: a faulted queue item produces one availability notice and starts
// the next bounded candidate instead of ending playback.
#[test]
fn fb_6_fault_on_a_multi_track_queue_advances_and_keeps_playing() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![7, 8, 9],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(
        23,
        AndroidPlayerEvent::Error {
            message: "ERROR_CODE_IO_UNSPECIFIED: Source error".to_owned(),
        },
    );

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Playing);
    assert_eq!(snapshot.current_index, Some(1));
    assert_eq!(snapshot.current_track_id, Some(8));
    assert_eq!(snapshot.automatic_advance_count, 0);
    assert_eq!(
        snapshot.error.as_deref(),
        Some("Track unavailable — skipped")
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
        ]
    );
}

// UX FB-6: a wholly unplayable repeating queue stops after one bounded pass
// instead of cycling forever.
#[test]
fn fb_6_every_faulting_track_stops_at_the_latched_bound() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![7, 8, 9],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.session.set_repeat(AndroidRepeatMode::All).unwrap();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    for _ in 0..3 {
        bridge.emit(
            23,
            AndroidPlayerEvent::Error {
                message: "decoder failed".to_owned(),
            },
        );
    }

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Stopped);
    assert_eq!(
        snapshot.error.as_deref(),
        Some("Playback stopped — too many unplayable tracks")
    );
}

// UX FB-6: faulting the final queue item still stops because no successor
// exists, while preserving the user-facing fault policy instead of backend text.
#[test]
fn fb_6_fault_on_the_last_track_stops_at_queue_exhaustion() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![7, 8],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            1,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(
        23,
        AndroidPlayerEvent::Error {
            message: "decoder failed".to_owned(),
        },
    );

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Stopped);
    assert_eq!(snapshot.current_index, None);
    assert_eq!(snapshot.current_track_id, None);
    assert_eq!(
        snapshot.error.as_deref(),
        Some("Track unavailable — skipped")
    );
    assert_eq!(fixture.calls.lock().unwrap().as_slice(), &[PortCall::Stop]);
}

// UX PLAY-5b: once a replacement track demonstrably plays, it ends the prior
// background-fault run and a later fault gets a fresh bounded skip attempt.
#[test]
fn play_5b_successful_start_resets_the_consecutive_fault_run() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![7, 8],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.session.set_repeat(AndroidRepeatMode::All).unwrap();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(
        23,
        AndroidPlayerEvent::Error {
            message: "first fault".to_owned(),
        },
    );
    assert_eq!(fixture.session.snapshot().unwrap().current_index, Some(1));

    bridge.emit(
        23,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Playing,
        },
    );
    assert_eq!(fixture.session.snapshot().unwrap().error, None);

    bridge.emit(
        23,
        AndroidPlayerEvent::Error {
            message: "later fault".to_owned(),
        },
    );

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Playing);
    assert_eq!(snapshot.current_index, Some(0));
    assert_eq!(
        snapshot.error.as_deref(),
        Some("Track unavailable — skipped")
    );
}

#[test]
fn buffering_emitted_synchronously_while_starting_is_preserved() {
    let fixture = recording_session();

    fixture
        .session
        .play_tracks(vec![7], vec![SYNCHRONOUS_BUFFERING_URI.to_owned()], 0)
        .unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap().state,
        AndroidPlaybackState::Buffering,
    );
}

#[test]
fn buffering_after_a_failed_start_cannot_revive_a_stopped_snapshot() {
    let fixture = recording_session();

    fixture
        .session
        .play_tracks(vec![7], vec![FAILING_PLAY_URI.to_owned()], 0)
        .unwrap_err();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();
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
    assert_eq!(
        snapshot.error.as_deref(),
        Some("Playback stopped — too many unplayable tracks")
    );
}
