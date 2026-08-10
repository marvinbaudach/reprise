use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{PortCall, RecordingListener, RecordingPort};
use crate::playback::PlaybackEventBridge;
use crate::AndroidPlaybackSession;

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
                artist: Some("ID Boundary Artist".to_owned()),
                album: Some("ID Boundary Album".to_owned()),
                album_artist: Some("ID Boundary Artist".to_owned()),
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

#[test]
fn id_only_play_resolves_live_paths_and_preserves_the_requested_start() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::new(Mutex::new(None::<Arc<PlaybackEventBridge>>)),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    calls.lock().unwrap().clear();
    let requested = vec![tracks[2].id, tracks[0].id, tracks[1].id];

    session.play_track_ids(requested.clone(), 1).unwrap();

    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.current_track_id, Some(requested[1]));
    assert_eq!(
        snapshot.current_track_uri.as_deref(),
        Some(tracks[0].path.as_str())
    );
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            PortCall::PlayUri(tracks[0].path.clone()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some(tracks[1].path.clone())),
        ],
    );
}
