use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{PortCall, RecordingListener, RecordingPort};
use crate::playback::{AndroidPlaybackState, AndroidPlayerEvent, PlaybackEventBridge};
use crate::{AndroidPlaybackSession, AndroidRepeatMode, WindowRange};

type TestSessionControls = (
    AndroidPlaybackSession,
    Arc<Mutex<Vec<PortCall>>>,
    Arc<Mutex<Option<Arc<PlaybackEventBridge>>>>,
);

fn seed_tracks(directory: &Path, titles: &[&str]) -> Vec<reprise_core::models::Track> {
    let music = directory.join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../android/app/src/main/assets/sine.flac");
    for (index, title) in titles.iter().enumerate() {
        let path = music.join(format!("{index}.flac"));
        std::fs::copy(&fixture, &path).unwrap();
        reprise_core::library::tag_edit::apply_patch_to_file(
            &path,
            &reprise_core::library::tag_edit::TagPatch {
                title: Some((*title).to_owned()),
                artist: Some("Boundary Artist".to_owned()),
                album: Some("Boundary Album".to_owned()),
                album_artist: Some("Boundary Artist".to_owned()),
                year: Some(Some(2026)),
                track_no: Some(Some((index + 1) as u32)),
                genre: Some("Test".to_owned()),
            },
        )
        .unwrap();
    }
    let database_path = directory.join(crate::DATABASE_FILE_NAME);
    let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
    reprise_core::queries::query_library_text_search(
        &database,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 500,
        },
    )
    .unwrap()
    .rows
}

fn session_in(directory: &Path) -> AndroidPlaybackSession {
    session_with_calls(directory).0
}

fn session_with_calls(directory: &Path) -> (AndroidPlaybackSession, Arc<Mutex<Vec<PortCall>>>) {
    let (session, calls, _) = session_with_controls(directory);
    (session, calls)
}

fn session_with_controls(directory: &Path) -> TestSessionControls {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let bridge = Arc::new(Mutex::new(None));
    AndroidPlaybackSession::new(
        directory.to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::clone(&bridge),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .map(|session| (session, calls, bridge))
    .unwrap()
}

#[test]
fn explicit_enqueue_resolves_live_ids_persists_order_and_starts_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Current", "Next", "Tail"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let (session, calls) = session_with_calls(directory.path());
    calls.lock().unwrap().clear();

    assert_eq!(
        session
            .queue_tracks_last(vec![track("Current").id, track("Tail").id, i64::MAX])
            .unwrap(),
        2,
    );
    assert_eq!(
        session.queue_tracks_next(vec![track("Next").id]).unwrap(),
        1,
    );
    let queued_ids = |session: &AndroidPlaybackSession| {
        session
            .upcoming_tracks(WindowRange {
                offset: 0,
                limit: 10,
            })
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        queued_ids(&session),
        vec![track("Current").id, track("Next").id, track("Tail").id]
    );
    assert_eq!(
        session.snapshot().unwrap().state,
        AndroidPlaybackState::Stopped
    );
    assert!(!calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| matches!(call, PortCall::PlayUri(_))));
    assert_eq!(
        calls.lock().unwrap().last(),
        Some(&PortCall::SetNext(Some(track("Next").path.clone())))
    );

    drop(session);
    assert_eq!(
        queued_ids(&session_in(directory.path())),
        vec![track("Current").id, track("Next").id, track("Tail").id]
    );
}

#[test]
fn enqueueing_into_an_exhausted_session_revives_it_and_shows_the_pick() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Played", "Picked"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let (session, calls, bridge) = session_with_controls(directory.path());
    session
        .play_tracks(
            vec![track("Played").id],
            vec![track("Played").path.clone()],
            0,
        )
        .unwrap();
    bridge
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .emit(23, AndroidPlayerEvent::TrackFinished);
    let future = |session: &AndroidPlaybackSession| {
        session
            .upcoming_tracks(WindowRange {
                offset: 0,
                limit: 10,
            })
            .unwrap()
    };
    assert_eq!(
        session.snapshot().unwrap().state,
        AndroidPlaybackState::Stopped,
        "the fixture only means something while the session really is exhausted",
    );
    assert!(future(&session).rows.is_empty());
    calls.lock().unwrap().clear();

    assert_eq!(
        session.queue_tracks_last(vec![track("Picked").id]).unwrap(),
        1,
    );

    let revived = future(&session);
    assert_eq!(revived.total, 1);
    assert_eq!(
        revived.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Picked").id],
        "an explicit pick must not evaporate into a queue that ran off its end",
    );
    assert!(
        !calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| matches!(call, PortCall::PlayUri(_))),
        "reviving the queue is not permission to start playing",
    );

    drop(session);
    assert_eq!(
        future(&session_in(directory.path()))
            .rows
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        vec![track("Picked").id],
        "the revived position has to survive the process that raised it",
    );
}

#[test]
fn stopped_queue_view_and_play_now_share_the_current_row_offset() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let (session, calls) = session_with_calls(directory.path());
    calls.lock().unwrap().clear();
    let ids = tracks.iter().map(|track| track.id).collect::<Vec<_>>();

    assert_eq!(session.queue_tracks_last(ids.clone()).unwrap(), 3);
    let visible = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    assert_eq!(
        visible.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        ids
    );
    assert!(session.play_upcoming_track_now(0, ids[0]).unwrap());
    assert_eq!(session.snapshot().unwrap().current_track_id, Some(ids[0]));
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| matches!(call, PortCall::PlayUri(uri) if uri == &tracks[0].path)));
}

#[test]
fn upcoming_window_excludes_the_current_track_and_counts_beyond_the_page() {
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

    let window = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 2,
        })
        .unwrap();

    assert_eq!(
        window.total, 3,
        "total is the complete future, not the page"
    );
    assert_eq!(
        window.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("First").id, track("Second").id],
    );
    assert!(window.has_more);
}

#[test]
fn forward_window_total_is_independent_of_non_negative_offset() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Current", "First", "Second", "Third"]);
    let session = session_in(directory.path());
    session
        .play_tracks(
            tracks.iter().map(|track| track.id).collect(),
            tracks.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();

    for offset in [0, 1, 2, 3, 20] {
        let window = session
            .upcoming_tracks(WindowRange { offset, limit: 1 })
            .unwrap();
        assert_eq!(
            window.total, 3,
            "forward total must remain the complete future at offset {offset}",
        );
    }
}

#[test]
fn signed_window_reaches_both_sides_and_clamps_without_shifting_at_the_ends() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Zero", "One", "Two", "Three", "Four"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [
        track("Zero"),
        track("One"),
        track("Two"),
        track("Three"),
        track("Four"),
    ];
    let session = session_in(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            2,
        )
        .unwrap();

    let middle = session
        .upcoming_tracks(WindowRange {
            // Offsets stay relative to the upcoming boundary. The current
            // row is -1, so -3 begins two positions before the cursor.
            offset: -3,
            limit: 5,
        })
        .unwrap();
    assert_eq!(
        middle.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        ordered.iter().map(|track| track.id).collect::<Vec<_>>(),
    );
    assert_eq!(session.snapshot().unwrap().current_index, Some(2));

    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    let first = session
        .upcoming_tracks(WindowRange {
            offset: -3,
            limit: 5,
        })
        .unwrap();
    assert_eq!(
        first.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Zero").id, track("One").id, track("Two").id],
        "clamping the left edge must not pull extra rows in from the right",
    );

    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            4,
        )
        .unwrap();
    let last = session
        .upcoming_tracks(WindowRange {
            offset: -3,
            limit: 5,
        })
        .unwrap();
    assert_eq!(
        last.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Two").id, track("Three").id, track("Four").id],
    );
    assert!(
        last.rows.len() <= usize::try_from(last.total).unwrap(),
        "a signed window must never return more rows than its advertised total",
    );
}

#[test]
fn signed_window_and_current_index_follow_the_same_shuffled_order() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Zero", "One", "Two", "Three", "Four"]);
    let session = session_in(directory.path());
    session
        .play_tracks(
            tracks.iter().map(|track| track.id).collect(),
            tracks.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    session.set_shuffle(true).unwrap();
    let future = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap();
    let (source_position, target_id) = future
        .rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.id != tracks[4].id)
        .map(|(position, row)| (position, row.id))
        .unwrap();
    assert!(session
        .move_upcoming_track(u64::try_from(source_position).unwrap(), target_id, 3)
        .unwrap());
    assert!(session.play_upcoming_track_now(3, target_id).unwrap());

    let snapshot = session.snapshot().unwrap();
    let window = session
        .upcoming_tracks(WindowRange {
            offset: -5,
            limit: 5,
        })
        .unwrap();
    let ids = window.rows.iter().map(|row| row.id).collect::<Vec<_>>();

    let current_position = ids
        .iter()
        .position(|track_id| Some(*track_id) == snapshot.current_track_id)
        .and_then(|position| u64::try_from(position).ok());
    assert_eq!(snapshot.current_index, current_position);

    session.set_shuffle(false).unwrap();
    let linear_position = tracks
        .iter()
        .position(|track| track.id == target_id)
        .and_then(|position| u64::try_from(position).ok());
    assert_eq!(session.snapshot().unwrap().current_index, linear_position);
}

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

#[path = "queue_persistence_boundary_tests.rs"]
mod persistence_tests;
