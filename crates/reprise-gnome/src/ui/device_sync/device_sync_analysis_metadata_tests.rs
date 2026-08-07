//! M13 desktop transfer coverage for Core-owned mobile metadata formats.

use super::*;
use reprise_core::device_sync::analysis_sidecar::AnalysisSidecar;
use reprise_core::device_sync::settings::{upsert_device_file, DeviceFileRecord};
use reprise_core::device_sync::track_metadata_list::{
    TrackMetadataList, FILE_NAME as TRACK_METADATA_FILE_NAME,
};
use reprise_core::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};
use reprise_core::waveform::TrackRenderData;

fn source() -> TrackSourceFingerprint {
    TrackSourceFingerprint {
        mtime_seconds: 0,
        size_bytes: 0,
        device: None,
        inode: None,
    }
}

fn seed_render_data(conn: &Db, track_id: i64, cell: u8, peak: u8) {
    reprise_core::db::set_track_render_data(
        conn,
        track_id,
        source(),
        &TrackRenderData {
            waveform_peaks: vec![peak; 4],
            spectrogram: TrackSpectrogram::from_cells(vec![cell; 24]).unwrap(),
        },
    )
    .unwrap();
}

fn copied_analysis(
    backend: &FakeBackend,
    device_path: &str,
) -> reprise_core::device_sync::analysis_sidecar::AnalysisSidecar {
    let copies = backend.state.managed_copy_contents.borrow();
    let bytes = &copies
        .iter()
        .find(|(_, path, _)| path == device_path)
        .unwrap_or_else(|| panic!("missing generated sidecar {device_path}: {copies:?}"))
        .2;
    AnalysisSidecar::decode(bytes).unwrap()
}

fn register_synced_track(conn: &Db, temp: &tempfile::TempDir, track_id: i64) -> ManagedDeviceFile {
    let source_path = temp.path().join(format!("{track_id}.flac"));
    let metadata = std::fs::metadata(&source_path).unwrap();
    let source_mtime = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let device_path = format!("Artist/Unknown Album/00 Track {track_id}.opus");
    upsert_device_file(
        conn,
        &DeviceFileRecord {
            device_serial: "a".into(),
            track_id,
            source_path: source_path.to_string_lossy().into_owned(),
            source_size: metadata.len(),
            source_mtime,
            device_path: device_path.clone(),
            device_size: 100,
            profile_fingerprint: "opus-vbr-160-v1".into(),
            pinned: false,
        },
    )
    .unwrap();
    ManagedDeviceFile {
        relative_path: device_path,
        size_bytes: 100,
    }
}

#[test]
fn synced_audio_without_sidecars_plans_one_analysis_copy_per_track() {
    run(async {
        let (temp, conn) = fixture();
        seed_render_data(&conn, 1, 7, 3);
        seed_render_data(&conn, 2, 8, 5);
        select_road_playlist(&conn, &[1, 2]);
        let managed_files = [
            register_synced_track(&conn, &temp, 1),
            register_synced_track(&conn, &temp, 2),
        ];
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.managed_files.replace(managed_files.to_vec());

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(device.page.changes.additions, 2);
        assert_eq!(device.page.changes.replacements, 0);

        runtime.sync_now("a").unwrap();
        settle().await;

        let copied_paths = backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            copied_paths
                .iter()
                .filter(|path| path.ends_with(".reprise-analysis"))
                .count(),
            2
        );
        assert!(
            copied_paths.iter().all(|path| !path.ends_with(".opus")),
            "missing analyses must not cause audio copies: {copied_paths:?}"
        );
    });
}

#[test]
fn unreadable_analysis_for_one_track_does_not_block_the_other_tracks_plan() {
    run(async {
        let (temp, conn) = fixture();
        seed_render_data(&conn, 1, 7, 3);
        seed_render_data(&conn, 2, 8, 5);
        crate::test_db::connection(&conn)
            .execute(
                "UPDATE track_spectrograms SET data = 'abcdefghijklmnopqrstuvwx' WHERE track_id = 1",
                [],
            )
            .unwrap();
        select_road_playlist(&conn, &[1, 2]);
        let managed_files = [
            register_synced_track(&conn, &temp, 1),
            register_synced_track(&conn, &temp, 2),
        ];
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.managed_files.replace(managed_files.to_vec());

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(device.page.changes.additions, 1);
        assert_eq!(device.page.changes.replacements, 0);

        runtime.sync_now("a").unwrap();
        settle().await;

        let copied_paths = backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        assert!(copied_paths
            .iter()
            .any(|path| path.ends_with("Track 2.reprise-analysis")));
        assert!(
            copied_paths.iter().all(|path| !path.contains("Track 1")),
            "the unreadable analysis must be skipped without copying its audio: {copied_paths:?}"
        );
    });
}

#[test]
fn analysis_sidecar_is_written_beside_its_transcoded_track_with_the_database_fingerprint() {
    run(async {
        let (_temp, conn) = fixture();
        seed_render_data(&conn, 1, 7, 3);
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let audio_path = "Artist/Unknown Album/00 Track 1.opus";
        let sidecar_path = "Artist/Unknown Album/00 Track 1.reprise-analysis";
        assert!(backend
            .state
            .managed_copies
            .borrow()
            .contains(&("/Music/Reprise".into(), audio_path.into())));
        let sidecar = copied_analysis(&backend, sidecar_path);
        assert_eq!(sidecar.source, source());
        assert_eq!(sidecar.spectrogram.cells(), &[7; 24]);
        assert_eq!(sidecar.waveform_peaks, vec![3; 4]);
    });
}

#[test]
fn a_partial_sync_never_rewrites_the_untouched_tracks_analysis() {
    run(async {
        let (temp, conn) = fixture();
        seed_render_data(&conn, 1, 7, 3);
        seed_render_data(&conn, 2, 8, 5);
        select_road_playlist(&conn, &[1, 2]);
        let synced_audio = register_synced_track(&conn, &temp, 2);
        let sidecar_size = AnalysisSidecar::for_track(&conn, 2)
            .unwrap()
            .unwrap()
            .encode()
            .unwrap()
            .len() as u64;
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.managed_files.replace(vec![
            synced_audio,
            ManagedDeviceFile {
                relative_path: "Artist/Unknown Album/00 Track 2.reprise-analysis".into(),
                size_bytes: sidecar_size,
            },
        ]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let copied_paths = backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        assert!(copied_paths
            .iter()
            .any(|path| path.ends_with("Track 1.reprise-analysis")));
        assert!(
            copied_paths.iter().all(|path| !path.contains("Track 2")),
            "the unchanged audio and its analysis must both stay untouched: {copied_paths:?}"
        );
    });
}

fn copied_track_metadata(backend: &FakeBackend) -> Option<TrackMetadataList> {
    backend
        .state
        .managed_copy_contents
        .borrow()
        .iter()
        .find(|(_, path, _)| path == TRACK_METADATA_FILE_NAME)
        .map(|(_, _, bytes)| TrackMetadataList::decode(bytes).unwrap())
}

#[test]
fn automatic_sync_writes_analysis_only_while_listener_sync_writes_analysis_and_real_judgements() {
    run(async {
        let (_automatic_files, automatic_db) = fixture();
        seed_render_data(&automatic_db, 1, 7, 3);
        select_road_playlist(&automatic_db, &[1]);
        let mut automatic_settings = reprise_core::device_sync::settings::load_or_create_settings(
            &automatic_db,
            "a",
            "Phone a",
        )
        .unwrap();
        automatic_settings.sync_automatically = true;
        save_settings(&automatic_db, &automatic_settings).unwrap();
        let automatic_backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let _automatic_runtime =
            DeviceSyncRuntime::with_backend(&automatic_db, automatic_backend.clone());
        settle().await;

        assert!(automatic_backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.reprise-analysis")));
        assert_eq!(
            copied_track_metadata(&automatic_backend),
            None,
            "MTP-30's cable-triggered sync must not overwrite phone judgements"
        );

        let (_listener_files, listener_db) = fixture();
        seed_render_data(&listener_db, 1, 8, 5);
        select_road_playlist(&listener_db, &[1]);
        crate::test_db::connection(&listener_db)
            .execute(
                "UPDATE tracks SET rating = 4, play_count = 27 WHERE id = 1",
                [],
            )
            .unwrap();
        let listener_backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let listener_runtime =
            DeviceSyncRuntime::with_backend(&listener_db, listener_backend.clone());
        settle().await;
        listener_runtime.sync_now("a").unwrap();
        settle().await;

        assert!(listener_backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.reprise-analysis")));
        let list = copied_track_metadata(&listener_backend).expect("listener-started list");
        assert_eq!(list.entries.len(), 1);
        assert_eq!(
            list.entries[0].device_path,
            "Artist/Unknown Album/00 Track 1.opus"
        );
        assert_eq!(
            (list.entries[0].rating, list.entries[0].play_count),
            (4, 27),
            "the desktop rating must remain four, not be flattened to a phone heart"
        );
    });
}
