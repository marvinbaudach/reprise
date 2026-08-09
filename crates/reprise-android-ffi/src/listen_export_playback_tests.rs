use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use reprise_core::device_sync::listen_report::{ListenReport, ListenReportAcknowledgement};

use super::test_support::{RecordingListener, RecordingPort};
use crate::playback::AndroidPlayerEvent;
use crate::{AndroidPlaybackSession, MusicLibrary};

#[test]
fn half_play_after_reinstall_keeps_its_device_path_in_the_export_journal() {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("sine.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac"),
        &track_path,
    )
    .unwrap();
    let database_path = directory.path().join("reprise.db");
    let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
    let track = reprise_core::queries::query_library_text_search(
        &database,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows
    .remove(0);
    assert_eq!(
        reprise_core::device_sync::mobile_import::device_path_for_track(&database, track.id)
            .unwrap()
            .as_deref(),
        Some("sine.flac"),
    );
    drop(database);

    // The selected sync folder outlives an app reinstall. Its acknowledgement
    // therefore belongs to the previous app-private export journal.
    let previous_install_acknowledgement = ListenReportAcknowledgement::new(1).encode();
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        ListenReport::decode(
            &library
                .prepare_listen_report(Some(previous_install_acknowledgement.clone()))
                .unwrap(),
        )
        .unwrap(),
        ListenReport::default(),
    );
    drop(library);

    let bridge = Arc::new(Mutex::new(None));
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::new(Mutex::new(Vec::new())),
            bridge: Arc::clone(&bridge),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    session
        .play_tracks(vec![track.id], vec![track.path], 0)
        .unwrap();
    bridge.lock().unwrap().clone().unwrap().emit(
        23,
        AndroidPlayerEvent::Position {
            position_ms: 600,
            duration_ms: 1_000,
        },
    );
    drop(session);

    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    let before_acknowledgement =
        ListenReport::decode(&library.prepare_listen_report(None).unwrap()).unwrap();
    assert_eq!(before_acknowledgement.listens.len(), 1);
    assert_eq!(before_acknowledgement.listens[0].sequence, 2);
    assert_eq!(before_acknowledgement.listens[0].device_path, "sine.flac");
    let report = ListenReport::decode(
        &library
            .prepare_listen_report(Some(previous_install_acknowledgement))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(report.listens.len(), 1);
    assert_eq!(report.listens[0].sequence, 2);
    assert_eq!(report.listens[0].device_path, "sine.flac");
}
