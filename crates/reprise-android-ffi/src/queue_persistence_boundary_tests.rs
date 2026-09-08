use super::*;

#[test]
fn a_fresh_session_restores_the_saved_order_and_position_paused() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Current", "First", "Second", "Third"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [
        track("Current"),
        track("First"),
        track("Second"),
        track("Third"),
    ];
    let session = session_in(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    assert!(session
        .move_upcoming_track(2, track("Third").id, 0)
        .unwrap());
    assert!(session
        .play_upcoming_track_now(0, track("Third").id)
        .unwrap());
    session.set_repeat(AndroidRepeatMode::All).unwrap();
    drop(session);

    let (restored, calls) = session_with_calls(directory.path());

    let snapshot = restored.snapshot().unwrap();
    assert_eq!(snapshot.state, AndroidPlaybackState::Paused);
    assert_eq!(snapshot.current_track_id, Some(track("Third").id));
    assert_eq!(snapshot.repeat, AndroidRepeatMode::All);
    let future = restored
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(
        future.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Third").id, track("First").id, track("Second").id],
    );
    assert!(
        !calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| matches!(call, PortCall::PlayUri(_))),
        "restoring must not start Media3",
    );

    restored.toggle_pause().unwrap();
    assert_eq!(
        restored.snapshot().unwrap().state,
        AndroidPlaybackState::Playing,
    );
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| matches!(call, PortCall::PlayUri(uri) if uri == &track("Third").path)));
}

#[test]
fn restore_discards_a_deleted_track_without_losing_the_surviving_queue() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Current", "Deleted", "Survivor"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [track("Current"), track("Deleted"), track("Survivor")];
    let session = session_in(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    drop(session);

    let database_path = directory.path().join(crate::DATABASE_FILE_NAME);
    let database = reprise_core::db::Db::open_ready(&database_path).unwrap();
    assert_eq!(
        reprise_core::queries::remove_tracks_matching_paths(
            &database,
            &[(
                track("Deleted").id,
                std::path::PathBuf::from(&track("Deleted").path),
            )],
        )
        .unwrap(),
        vec![track("Deleted").id],
    );
    drop(database);

    let restored = session_in(directory.path());

    assert_eq!(
        restored.snapshot().unwrap().current_track_id,
        Some(track("Current").id),
    );
    let future = restored
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(future.total, 2);
    assert_eq!(
        future.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Current").id, track("Survivor").id],
    );
}

#[test]
fn automatic_advance_is_saved_for_the_next_process() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [track("First"), track("Second"), track("Third")];
    let (session, _, bridge) = session_with_controls(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();

    bridge
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .emit(23, AndroidPlayerEvent::TrackFinished);
    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Second").id),
    );
    drop(session);

    let restored = session_in(directory.path());
    assert_eq!(
        restored.snapshot().unwrap().state,
        AndroidPlaybackState::Paused
    );
    assert_eq!(
        restored.snapshot().unwrap().current_track_id,
        Some(track("Second").id),
    );
}

// UX FB-6: a fault-driven skip persists the same queue cursor that the next
// process would observe after an ordinary automatic advance.
#[test]
fn fb_6_fault_advance_is_saved_for_the_next_process() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [track("First"), track("Second"), track("Third")];
    let (session, _, bridge) = session_with_controls(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();

    bridge.lock().unwrap().clone().unwrap().emit(
        23,
        AndroidPlayerEvent::Error {
            message: "decoder failed".to_owned(),
            missing: true,
        },
    );
    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Second").id),
    );
    drop(session);

    let restored = session_in(directory.path());
    assert_eq!(
        restored.snapshot().unwrap().state,
        AndroidPlaybackState::Paused
    );
    assert_eq!(
        restored.snapshot().unwrap().current_track_id,
        Some(track("Second").id),
    );
}

#[test]
fn queue_saves_leave_unrelated_desktop_session_fields_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Only"]);
    let database_path = directory.path().join(crate::DATABASE_FILE_NAME);
    let database = reprise_core::db::Db::open_ready(&database_path).unwrap();
    let mut desktop_state = reprise_core::library::session::load(&database);
    desktop_state.search = "desktop search survives".to_owned();
    desktop_state.window_width = 1777;
    reprise_core::library::session::save(&database, &desktop_state).unwrap();
    drop(database);

    let session = session_in(directory.path());
    session
        .play_tracks(vec![tracks[0].id], vec![tracks[0].path.clone()], 0)
        .unwrap();
    drop(session);

    let database = reprise_core::db::Db::open_ready(&database_path).unwrap();
    let saved = reprise_core::library::session::load(&database);
    assert_eq!(saved.search, "desktop search survives");
    assert_eq!(saved.window_width, 1777);
}

#[test]
fn manual_cursor_moves_and_shuffle_each_survive_a_fresh_session() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [track("First"), track("Second"), track("Third")];
    let session = session_in(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    session.next().unwrap();
    drop(session);

    let session = session_in(directory.path());
    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Second").id),
    );
    session.next().unwrap();
    drop(session);

    let session = session_in(directory.path());
    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Third").id),
        "a second manual cursor move must survive too; Previous is now runtime-only history and a fresh session intentionally has none",
    );
    session.set_shuffle(true).unwrap();
    drop(session);

    let restored = session_in(directory.path());
    assert!(restored.snapshot().unwrap().shuffled);
    assert_eq!(
        restored.snapshot().unwrap().current_track_id,
        Some(track("Third").id),
        "shuffle persistence must retain the current track",
    );
}
