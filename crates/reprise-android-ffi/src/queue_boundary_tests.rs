use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{PortCall, RecordingListener, RecordingPort};
use crate::playback::{AndroidPlaybackState, AndroidPlayerEvent, PlaybackEventBridge};
use crate::{AndroidPlaybackSession, WindowRange};

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
fn negative_upcoming_offset_is_the_first_page_not_an_empty_contradiction() {
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
            offset: -1,
            limit: 2,
        })
        .unwrap();

    assert_eq!(window.total, 3);
    assert_eq!(
        window.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("First").id, track("Second").id],
    );
    assert!(window.has_more);
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
    assert_eq!(future.total, 1);
    assert_eq!(
        future.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![track("Survivor").id],
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
    session.previous().unwrap();
    drop(session);

    let session = session_in(directory.path());
    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("First").id),
    );
    session.set_shuffle(true).unwrap();
    drop(session);

    let restored = session_in(directory.path());
    assert!(restored.snapshot().unwrap().shuffled);
    assert_eq!(
        restored.snapshot().unwrap().current_track_id,
        Some(track("First").id),
        "shuffle persistence must retain the current track",
    );
}
