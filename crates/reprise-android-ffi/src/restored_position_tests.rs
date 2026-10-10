//! A restored paused song keeps its position and answers a seek before any play.

use super::*;

const PAUSED_AT_MS: i64 = 54_000;
const SEEKED_TO_MS: i64 = 30_000;
const GENERATION: u64 = 23;

fn pause_at(
    session: &AndroidPlaybackSession,
    bridge: &Arc<Mutex<Option<Arc<PlaybackEventBridge>>>>,
    position_ms: i64,
    duration_ms: i64,
) {
    bridge.lock().unwrap().clone().unwrap().emit(
        GENERATION,
        AndroidPlayerEvent::Position {
            position_ms,
            duration_ms,
        },
    );
    session.toggle_pause().unwrap();
    assert_eq!(
        session.snapshot().unwrap().state,
        AndroidPlaybackState::Paused,
        "the fixture only means something once the song is paused",
    );
}

fn restore_paused_song(
    directory: &Path,
    duration_ms: i64,
) -> (reprise_core::models::Track, AndroidPlaybackSession) {
    let tracks = seed_tracks(directory, &["Paused"]);
    let track = tracks[0].clone();
    let (session, _, bridge) = session_with_controls(directory);
    session
        .play_tracks(vec![track.id], vec![track.path.clone()], 0)
        .unwrap();
    pause_at(&session, &bridge, PAUSED_AT_MS, duration_ms);
    drop(session);
    (track, session_in(directory))
}

#[test]
fn a_restored_paused_song_keeps_its_position_and_duration() {
    let directory = tempfile::tempdir().unwrap();
    let (track, restored) = restore_paused_song(directory.path(), 200_000);

    let snapshot = restored.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Paused);
    assert_eq!(snapshot.current_track_id, Some(track.id));
    assert_eq!(snapshot.position_ms, PAUSED_AT_MS);
    assert_eq!(
        snapshot.duration_ms, track.duration_ms,
        "a zero duration disables the seek bar until the first play",
    );
    assert!(snapshot.duration_ms > 0);
}

#[test]
fn a_seek_before_the_first_play_moves_the_position_without_the_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (_, restored) = restore_paused_song(directory.path(), 200_000);
    drop(restored);
    let (session, calls) = session_with_calls(directory.path());

    session.seek_to(SEEKED_TO_MS).unwrap();

    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.position_ms, SEEKED_TO_MS);
    assert!(
        !calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| matches!(call, PortCall::SeekTo(_))),
        "nothing is loaded in Media3 yet, so a backend seek would be lost",
    );
}

#[test]
fn the_first_play_after_a_restored_seek_starts_at_the_seeked_position() {
    let directory = tempfile::tempdir().unwrap();
    let (track, restored) = restore_paused_song(directory.path(), 200_000);
    drop(restored);
    let (session, calls) = session_with_calls(directory.path());

    session.seek_to(SEEKED_TO_MS).unwrap();
    session.toggle_pause().unwrap();

    let calls = calls.lock().unwrap();
    let started = calls
        .iter()
        .position(|call| matches!(call, PortCall::PlayPath(uri, _) if uri == &track.path))
        .expect("the first play starts the restored song");
    assert!(
        calls[started..].contains(&PortCall::SeekTo(SEEKED_TO_MS)),
        "the song must begin at the position the user chose, not at 0:00",
    );
}

#[test]
fn a_seek_while_paused_is_remembered_for_the_next_restore() {
    let directory = tempfile::tempdir().unwrap();
    let (_, restored) = restore_paused_song(directory.path(), 200_000);
    restored.seek_to(SEEKED_TO_MS).unwrap();
    drop(restored);

    let again = session_in(directory.path());

    assert_eq!(again.snapshot().unwrap().position_ms, SEEKED_TO_MS);
}

#[test]
fn replaying_the_song_from_the_start_forgets_the_old_position() {
    let directory = tempfile::tempdir().unwrap();
    let (track, restored) = restore_paused_song(directory.path(), 200_000);
    restored
        .play_tracks(vec![track.id], vec![track.path.clone()], 0)
        .unwrap();
    drop(restored);

    let again = session_in(directory.path());

    assert_eq!(
        again.snapshot().unwrap().position_ms,
        0,
        "a song started again from 0:00 must not come back at the old pause",
    );
}

#[test]
fn a_pause_reported_by_media3_is_remembered_for_the_next_restore() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Paused"]);
    let (session, _, bridge) = session_with_controls(directory.path());
    session
        .play_tracks(vec![tracks[0].id], vec![tracks[0].path.clone()], 0)
        .unwrap();
    let bridge = bridge.lock().unwrap().clone().unwrap();
    bridge.emit(
        GENERATION,
        AndroidPlayerEvent::Position {
            position_ms: PAUSED_AT_MS,
            duration_ms: 200_000,
        },
    );
    bridge.emit(
        GENERATION,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Paused,
        },
    );
    drop(session);

    let again = session_in(directory.path());

    assert_eq!(again.snapshot().unwrap().position_ms, PAUSED_AT_MS);
}

#[test]
fn resuming_a_song_that_already_counted_as_played_does_not_count_it_again() {
    use reprise_core::device_sync::listen_report::ListenReport;

    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Counted"]);
    let (session, _, bridge) = session_with_controls(directory.path());
    session
        .play_tracks(vec![tracks[0].id], vec![tracks[0].path.clone()], 0)
        .unwrap();
    // Far enough into the song that Core counts it as played.
    pause_at(&session, &bridge, 600, 1_000);
    drop(session);

    let (restored, _, bridge) = session_with_controls(directory.path());
    restored.toggle_pause().unwrap();
    bridge.lock().unwrap().clone().unwrap().emit(
        GENERATION,
        AndroidPlayerEvent::Position {
            position_ms: 650,
            duration_ms: 1_000,
        },
    );
    drop(restored);

    let library = library_in(directory.path());
    let report = ListenReport::decode(&library.prepare_listen_report(None).unwrap()).unwrap();
    assert_eq!(
        report.listens.len(),
        1,
        "the song was counted before the pause; resuming it is the same listen",
    );
}

#[test]
fn a_song_that_played_to_its_end_does_not_come_back_mid_song() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Finished"]);
    let (session, _, bridge) = session_with_controls(directory.path());
    session
        .play_tracks(vec![tracks[0].id], vec![tracks[0].path.clone()], 0)
        .unwrap();
    pause_at(&session, &bridge, PAUSED_AT_MS, 200_000);
    session.toggle_pause().unwrap();
    bridge
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .emit(GENERATION, AndroidPlayerEvent::TrackFinished);
    drop(session);

    let again = session_in(directory.path());

    assert_eq!(again.snapshot().unwrap().position_ms, 0);
}
