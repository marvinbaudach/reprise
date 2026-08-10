use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use reprise_core::queries;

use super::test_support::{PortCall, RecordingListener, RecordingPort};
use crate::playback::{AndroidPlaybackState, PlaybackEventBridge};
use crate::{AndroidPlaybackSession, TrashAction, WindowRange};

struct SelectiveTrash {
    failures: HashSet<String>,
}

impl TrashAction for SelectiveTrash {
    fn trash(&self, uri: String) -> Option<String> {
        if self.failures.contains(&uri) {
            return Some("device refused deletion".to_owned());
        }
        std::fs::remove_file(uri)
            .err()
            .map(|error| error.to_string())
    }
}

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
                artist: Some("Trash Artist".to_owned()),
                album: Some("Trash Album".to_owned()),
                album_artist: Some("Trash Artist".to_owned()),
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
    queries::query_library_text_search(
        &database,
        "",
        queries::WindowRange {
            offset: 0,
            limit: 500,
        },
    )
    .unwrap()
    .rows
}

fn session_with_calls(directory: &Path) -> (AndroidPlaybackSession, Arc<Mutex<Vec<PortCall>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let bridge = Arc::new(Mutex::new(None::<Arc<PlaybackEventBridge>>));
    let session = AndroidPlaybackSession::new(
        directory.to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge,
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    (session, calls)
}

fn successful_trash() -> Box<dyn TrashAction> {
    Box::new(SelectiveTrash {
        failures: HashSet::new(),
    })
}

#[test]
fn trash_tracks_reports_partial_failure_and_keeps_failed_database_rows() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Removed", "Refused"]);
    let removed = tracks
        .iter()
        .find(|track| track.title == "Removed")
        .unwrap();
    let refused = tracks
        .iter()
        .find(|track| track.title == "Refused")
        .unwrap();
    let (session, _) = session_with_calls(directory.path());

    let report = session
        .trash_tracks(
            vec![removed.id, refused.id],
            Box::new(SelectiveTrash {
                failures: HashSet::from([refused.path.clone()]),
            }),
        )
        .unwrap();

    assert_eq!(report.removed_ids, vec![removed.id]);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].track_id, refused.id);
    assert_eq!(report.failures[0].uri, refused.path);
    assert_eq!(report.failures[0].error, "device refused deletion");
    let database =
        reprise_core::db::Db::open_ready(&directory.path().join(crate::DATABASE_FILE_NAME))
            .unwrap();
    assert_eq!(
        queries::track_source_path(&database, removed.id).unwrap(),
        None
    );
    assert_eq!(
        queries::track_source_path(&database, refused.id).unwrap(),
        Some(Path::new(&refused.path).to_path_buf())
    );
}

#[test]
fn an_id_whose_row_is_already_gone_is_reported_rather_than_dropped() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Survivor"]);
    let survivor = &tracks[0];
    let vanished = survivor.id + 10_000;
    let (session, _) = session_with_calls(directory.path());

    let report = session
        .trash_tracks(vec![vanished, survivor.id], successful_trash())
        .unwrap();

    assert_eq!(report.removed_ids, vec![survivor.id]);
    assert_eq!(
        report.failures.len(),
        1,
        "a batch must account for every requested id, not quietly under-report",
    );
    assert_eq!(report.failures[0].track_id, vanished);
    assert_eq!(
        report.failures[0].uri, "",
        "there is no file to name for a row that no longer exists",
    );
    assert_eq!(
        report.failures[0].error,
        "this track was already gone from the library",
    );
}

#[test]
fn trashing_the_playing_track_advances_plays_and_removes_it_from_upcoming() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Playing", "Next", "Tail"]);
    let track = |title: &str| tracks.iter().find(|track| track.title == title).unwrap();
    let ordered = [track("Playing"), track("Next"), track("Tail")];
    let (session, calls) = session_with_calls(directory.path());
    session
        .play_tracks(
            ordered.iter().map(|track| track.id).collect(),
            ordered.iter().map(|track| track.path.clone()).collect(),
            0,
        )
        .unwrap();
    calls.lock().unwrap().clear();

    let report = session
        .trash_tracks(vec![track("Playing").id], successful_trash())
        .unwrap();

    assert_eq!(report.removed_ids, vec![track("Playing").id]);
    assert!(report.failures.is_empty());
    assert_eq!(
        session.snapshot().unwrap().current_track_id,
        Some(track("Next").id)
    );
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| matches!(call, PortCall::PlayUri(uri) if uri == &track("Next").path)));
    let visible_ids = session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.id)
        .collect::<Vec<_>>();
    assert_eq!(visible_ids, vec![track("Tail").id]);
    assert!(!visible_ids.contains(&track("Playing").id));
}

#[test]
fn trashing_the_last_playing_track_stops_playback() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["Only"]);
    let only = &tracks[0];
    let (session, calls) = session_with_calls(directory.path());
    session
        .play_tracks(vec![only.id], vec![only.path.clone()], 0)
        .unwrap();
    calls.lock().unwrap().clear();

    let report = session
        .trash_tracks(vec![only.id], successful_trash())
        .unwrap();

    assert_eq!(report.removed_ids, vec![only.id]);
    assert_eq!(
        session.snapshot().unwrap().state,
        AndroidPlaybackState::Stopped
    );
    assert_eq!(session.snapshot().unwrap().current_track_id, None);
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| matches!(call, PortCall::Stop)));
    assert!(session
        .upcoming_tracks(WindowRange {
            offset: 0,
            limit: 10,
        })
        .unwrap()
        .rows
        .is_empty());
}
