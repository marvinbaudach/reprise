use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{library_in, PortCall, RecordingListener, RecordingPort};
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
        library_in(directory),
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
    let target_id = tracks[4].id;
    let source_position = future
        .rows
        .iter()
        .position(|row| row.id == target_id)
        .unwrap();
    // Promoting the known flat-index-4 track puts it at queue index 1. This
    // explicit divergence keeps the regression deterministic for every shuffle.
    assert!(session
        .play_upcoming_track_now(u64::try_from(source_position).unwrap(), target_id)
        .unwrap());

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
    assert_eq!(current_position, Some(1));
    assert_eq!(snapshot.current_index, current_position);

    session.set_shuffle(false).unwrap();
    let linear_position = tracks
        .iter()
        .position(|track| track.id == target_id)
        .and_then(|position| u64::try_from(position).ok());
    assert_eq!(session.snapshot().unwrap().current_index, linear_position);
}

#[path = "queue_persistence_boundary_tests.rs"]
mod persistence_tests;

#[path = "queue_boundary_reorder_tests.rs"]
mod reorder_tests;
