use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{library_in, PortCall, RecordingListener, RecordingPort};
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

fn measure_loudness(directory: &Path, track_id: i64, integrated_lufs: f64) {
    let database_path = directory.join(crate::DATABASE_FILE_NAME);
    let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
    let source = reprise_core::db::track_source_fingerprint(&database, track_id)
        .unwrap()
        .unwrap();
    reprise_core::db::set_track_render_data(
        &database,
        track_id,
        source,
        &reprise_core::waveform::TrackRenderData {
            waveform_peaks: Vec::new(),
            spectrogram: reprise_core::spectrogram::TrackSpectrogram::empty(),
            loudness: Some(reprise_core::library::loudness::MeasuredLoudness {
                integrated_lufs,
                true_peak: 0.5,
            }),
            decoded_end_ms: None,
        },
    )
    .unwrap();
}

#[test]
fn set_next_carries_the_gain_resolved_from_the_phone_database() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second"]);
    measure_loudness(directory.path(), tracks[1].id, -21.0);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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

    session
        .play_track_ids(vec![tracks[0].id, tracks[1].id], 0)
        .unwrap();

    assert!(calls
        .lock()
        .unwrap()
        .contains(&PortCall::SetNext(Some(tracks[1].path.clone()), 3.0,)));
}

#[test]
fn an_id_without_a_live_path_is_skipped_and_the_start_still_names_its_track() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second"]);
    let vanished = tracks.iter().map(|track| track.id).max().unwrap() + 10_000;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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

    // Position 2 in the request is the second surviving track; a start index
    // read against the *resolved* list would start the wrong one.
    session
        .play_track_ids(vec![tracks[0].id, vanished, tracks[1].id], 2)
        .unwrap();

    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.current_track_id, Some(tracks[1].id));
    assert_eq!(
        snapshot.current_track_uri.as_deref(),
        Some(tracks[1].path.as_str())
    );
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            PortCall::PlayPath(tracks[1].path.clone(), 0.0),
            PortCall::CurrentGeneration,
            PortCall::SetNext(None, 0.0),
        ],
    );
}

#[test]
fn tapping_a_track_that_no_longer_resolves_is_refused_rather_than_shifted() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second"]);
    let vanished = tracks.iter().map(|track| track.id).max().unwrap() + 10_000;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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

    let refused = session.play_track_ids(vec![tracks[0].id, vanished, tracks[1].id], 1);

    let error = refused.expect_err("the tapped row no longer exists");
    assert!(
        format!("{error}").contains("no longer in the library"),
        "the surface has to be told which row it lost, not handed a neighbour: {error}",
    );
    assert_eq!(session.snapshot().unwrap().current_track_id, None);
    assert!(
        calls.lock().unwrap().is_empty(),
        "a refused tap must not touch the backend",
    );
}

#[test]
fn id_only_play_resolves_live_paths_and_preserves_the_requested_start() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second", "Third"]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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
            PortCall::PlayPath(tracks[0].path.clone(), 0.0),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some(tracks[1].path.clone()), 0.0),
        ],
    );
}

#[test]
fn a_track_whose_uri_is_not_its_database_path_still_gets_its_gain_by_id() {
    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second"]);
    measure_loudness(directory.path(), tracks[0].id, -21.0);
    measure_loudness(directory.path(), tracks[1].id, -15.0);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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

    // The phone plays provider URIs that are not the paths Core stores.
    session
        .play_tracks(
            vec![tracks[0].id, tracks[1].id],
            vec![
                "content://provider/document/first".to_owned(),
                "content://provider/document/second".to_owned(),
            ],
            0,
        )
        .unwrap();

    let recorded = calls.lock().unwrap();
    assert!(recorded.contains(&PortCall::PlayPath(
        "content://provider/document/first".to_owned(),
        3.0
    )));
    assert!(recorded.contains(&PortCall::SetNext(
        Some("content://provider/document/second".to_owned()),
        -3.0
    )));
}

fn set_mode(
    session: &AndroidPlaybackSession,
    mode: reprise_core::library::settings::ReplayGainMode,
) {
    let writer = session.library_writer();
    let writer = writer.lock().unwrap();
    reprise_core::library::settings::set_replay_gain_mode(&writer, mode).unwrap();
}

#[test]
fn play_21_a_mode_change_reaches_the_playing_track_and_the_pre_fed_one() {
    use reprise_core::library::settings::ReplayGainMode;

    let directory = tempfile::tempdir().unwrap();
    let tracks = seed_tracks(directory.path(), &["First", "Second"]);
    measure_loudness(directory.path(), tracks[0].id, -21.0);
    measure_loudness(directory.path(), tracks[1].id, -15.0);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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
    session
        .play_track_ids(vec![tracks[0].id, tracks[1].id], 0)
        .unwrap();
    calls.lock().unwrap().clear();

    set_mode(&session, ReplayGainMode::Off);
    session.reload_playback_settings().unwrap();
    set_mode(&session, ReplayGainMode::Track);
    session.reload_playback_settings().unwrap();

    let recorded = calls.lock().unwrap();
    let gains: Vec<_> = recorded
        .iter()
        .filter_map(|call| match call {
            PortCall::SetGains(current, next) => Some((*current, *next)),
            _ => None,
        })
        .collect();
    assert_eq!(gains, vec![(0.0, Some(0.0)), (3.0, Some(-3.0))]);
    // The tracks are not restarted or re-queued to take the new gain.
    assert!(!recorded.iter().any(|call| matches!(
        call,
        PortCall::PlayPath(..) | PortCall::PlayUri(..) | PortCall::SetNext(..)
    )));
}

#[test]
fn a_mode_change_with_nothing_playing_touches_no_gain() {
    let directory = tempfile::tempdir().unwrap();
    seed_tracks(directory.path(), &["First"]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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

    set_mode(
        &session,
        reprise_core::library::settings::ReplayGainMode::Off,
    );
    session.reload_playback_settings().unwrap();

    assert!(!calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| matches!(call, PortCall::SetGains(..))));
}
