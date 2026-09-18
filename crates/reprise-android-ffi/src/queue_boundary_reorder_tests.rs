use super::*;

#[test]
fn queue_order_previous_then_next_round_trips_with_and_without_shuffle() {
    for shuffled in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let tracks = seed_tracks(directory.path(), &["Zero", "One", "Two", "Three", "Four"]);
        let session = session_in(directory.path());
        session
            .play_tracks(
                tracks.iter().map(|track| track.id).collect(),
                tracks.iter().map(|track| track.path.clone()).collect(),
                2,
            )
            .unwrap();
        if shuffled {
            session.set_shuffle(true).unwrap();
        }
        let before = session.snapshot().unwrap();

        session.previous_in_queue_order().unwrap();
        session.next().unwrap();

        let after = session.snapshot().unwrap();
        assert_eq!(
            after.current_track_id, before.current_track_id,
            "queue-order navigation must round-trip when shuffled={shuffled}",
        );
        assert_eq!(after.current_index, before.current_index);
    }
}

#[test]
fn live_deleted_upcoming_track_is_pruned_and_the_last_window_terminates() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(
        directory.path(),
        &["Current", "First", "Deleted", "Survivor"],
    );
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [
        track("Current"),
        track("First"),
        track("Deleted"),
        track("Survivor"),
    ];
    let session = session_in(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();

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

    let window = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();

    assert_eq!(window.total, 2);
    assert_eq!(
        window.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("First").id, track("Survivor").id],
    );
    assert!(
        !window.has_more,
        "the last page must terminate after pruning"
    );
}

#[test]
fn pruning_a_live_deleted_duplicate_keeps_the_loaded_current_slot() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Current", "Survivor"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let session = session_in(directory.path());
    session
        .play_tracks(
            vec![
                track("Current").id,
                track("Current").id,
                track("Survivor").id,
            ],
            vec![
                track("Current").path.clone(),
                track("Current").path.clone(),
                track("Survivor").path.clone(),
            ],
            0,
        )
        .unwrap();

    let database_path = directory.path().join(crate::DATABASE_FILE_NAME);
    let database = reprise_core::db::Db::open_ready(&database_path).unwrap();
    reprise_core::queries::remove_tracks_matching_paths(
        &database,
        &[(
            track("Current").id,
            std::path::PathBuf::from(&track("Current").path),
        )],
    )
    .unwrap();
    drop(database);

    let window = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();

    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Current").id),
        "pruning must not evict the already loaded current slot",
    );
    assert_eq!(window.total, 1);
    assert_eq!(
        window.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Survivor").id],
    );
    assert!(!window.has_more);
}

#[test]
fn an_exhausted_future_is_an_empty_window_not_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Only"]);
    let session = session_in(directory.path());
    session
        .play_tracks(vec![tracks[0].id], vec![tracks[0].path.clone()], 0)
        .unwrap();

    let future = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 200,
        })
        .unwrap();

    assert_eq!(future.total, 0);
    assert!(future.rows.is_empty());
    assert!(!future.has_more);
}

#[test]
fn moving_and_removing_identity_checked_rows_changes_the_next_window() {
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
    session.flush_queue_persistence();
    let database =
        reprise_core::db::Db::open_ready(&directory.path().join(crate::DATABASE_FILE_NAME))
            .unwrap();
    let saved = reprise_core::library::session::load(&database).queue;
    drop(database);
    let mut saved_queue = reprise_core::queue::Queue::new();
    saved_queue.restore_snapshot(saved).unwrap();
    assert_eq!(
        saved_queue.ids_in_order(),
        vec![
            track("Current").id,
            track("Third").id,
            track("First").id,
            track("Second").id,
        ],
        "moving a row must persist before any later edit",
    );
    assert!(session.remove_upcoming_track(1, track("First").id).unwrap());
    drop(session);

    let restored = session_in(directory.path());
    let future = restored
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(future.total, 3);
    assert_eq!(
        future.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Current").id, track("Third").id, track("Second").id],
    );
}

#[test]
fn moving_an_upcoming_track_downward_changes_the_next_window() {
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
        .move_upcoming_track(0, track("First").id, 2)
        .unwrap());

    let window = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(
        window.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Second").id, track("Third").id, track("First").id,],
    );
}

#[test]
fn stale_position_after_removal_is_reported_without_touching_the_new_occupant() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(
        directory.path(),
        &["Current", "Remove", "Later", "Occupant"],
    );
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [
        track("Current"),
        track("Remove"),
        track("Later"),
        track("Occupant"),
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
        .remove_upcoming_track(0, track("Remove").id)
        .unwrap());
    assert!(
        !session
            .move_upcoming_track(1, track("Later").id, 0)
            .unwrap(),
        "move must reject the identity that occupied this position before removal",
    );
    assert!(
        !session.remove_upcoming_track(1, track("Later").id).unwrap(),
        "remove must reject the identity that occupied this position before removal",
    );
    assert!(
        !session
            .play_upcoming_track_now(1, track("Later").id)
            .unwrap(),
        "the row that used to be at position 1 must be rejected after renumbering",
    );

    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Current").id),
    );
    let future = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(
        future.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Later").id, track("Occupant").id],
        "a stale action is a no-op, including for the row now at that position",
    );
}

#[test]
fn previous_in_queue_order_at_the_first_position_is_a_no_op() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let (session, calls) = session_with_calls(directory.path());
    session
        .play_tracks(
            tracks.iter().map(|track| track.id).collect(),
            tracks.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    let before = session.snapshot().unwrap();
    assert_eq!(before.current_index, Some(0));
    calls.lock().unwrap().clear();

    session.previous_in_queue_order().unwrap();

    let after = session.snapshot().unwrap();
    assert_eq!(after.current_index, Some(0));
    assert_eq!(after.current_track_id, before.current_track_id);
    assert!(
        calls.lock().unwrap().is_empty(),
        "the first position must not ask the backend to start anything: {:?}",
        calls.lock().unwrap(),
    );
}

#[test]
fn upcoming_tracks_at_the_first_position_starts_with_the_current_track() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let session = session_in(directory.path());
    session
        .play_tracks(
            tracks.iter().map(|track| track.id).collect(),
            tracks.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    let current_track_id = session.snapshot().unwrap().current_track_id.unwrap();
    assert_eq!(current_track_id, tracks[0].id);

    let window = session
        .upcoming_tracks(WindowRange {
            offset: -2,
            limit: 3,
        })
        .unwrap();

    // The negative offset asks for two rows before the current one, but
    // there is nothing before position 0: the window clamps instead of
    // shifting, so it starts with the current track and reaches only as far
    // as the clamped span allows (here, one row past it).
    assert_eq!(
        window.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![tracks[0].id, tracks[1].id],
        "the window must start with the current track, not a row before it",
    );
}
