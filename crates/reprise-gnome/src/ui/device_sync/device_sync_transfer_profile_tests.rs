use super::*;
use reprise_core::device_sync::{Mp3Quality, SyncPageWarning, TransferProfile};

fn save_profile(
    conn: &Rc<RefCell<Connection>>,
    device_id: &str,
    device_name: &str,
    playlist_id: i64,
    profile: TransferProfile,
) {
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: device_name.into(),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(playlist_id)]),
            profile,
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            sync_automatically: true,
        },
    )
    .unwrap();
}

fn save_smoke_profile(conn: &Rc<RefCell<Connection>>, playlist_id: i64, profile: TransferProfile) {
    save_profile(
        conn,
        crate::ui::device_sync_smoke::DEVICE_ID,
        crate::ui::device_sync_smoke::DEVICE_NAME,
        playlist_id,
        profile,
    );
}

async fn wait_for_storage(runtime: &Rc<DeviceSyncRuntime>, expected_devices: usize) {
    for _ in 0..1_000 {
        let devices = runtime.devices();
        if devices.len() == expected_devices
            && devices
                .iter()
                .all(|device| device.sync_phase != PlannedSyncPhase::ComputingDelta)
        {
            return;
        }
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
    }
    panic!(
        "storage inspection must settle before the transfer starts: {:?}",
        runtime.devices()
    );
}

async fn smoke_runtime(
    conn: &Rc<RefCell<Connection>>,
) -> (tempfile::TempDir, Rc<DeviceSyncRuntime>) {
    let device_root = tempfile::tempdir().unwrap();
    let backend = Rc::new(
        crate::ui::device_sync_smoke::SimulatedMtpDeviceBackend::for_root(device_root.path())
            .unwrap(),
    );
    let runtime = DeviceSyncRuntime::with_backend(conn, backend);
    wait_for_storage(&runtime, 1).await;
    (device_root, runtime)
}

async fn wait_for_completion(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> DeviceView {
    for _ in 0..1_000 {
        if let Some(device) = runtime
            .devices()
            .into_iter()
            .find(|device| device.id == device_id && device.last_sync.is_some())
        {
            return device;
        }
        gtk4::glib::timeout_future(Duration::from_millis(5)).await;
    }
    panic!(
        "device sync must complete with verified readback: {device_id}: {:?}",
        runtime.devices()
    );
}

async fn run_to_completion(runtime: &Rc<DeviceSyncRuntime>) -> DeviceView {
    runtime
        .sync_now(crate::ui::device_sync_smoke::DEVICE_ID)
        .unwrap();
    wait_for_completion(runtime, crate::ui::device_sync_smoke::DEVICE_ID).await
}

#[test]
fn simulated_mtp_phone_transcodes_lossless_selection_to_opus_160() {
    run(async {
        let (sources, conn) = fixture();
        let wav = sources.path().join("transcode.wav");
        write_silent_wav(&wav);
        conn.borrow()
            .execute(
                "INSERT INTO tracks (id,path,title,artist,album,album_artist,track_no,duration_ms,added_at) \
                 VALUES (20,?1,'Encoded','Artist','Album','Artist',1,100,0)",
                [wav.to_string_lossy().as_ref()],
            )
            .unwrap();
        conn.borrow()
            .execute_batch(
                "INSERT INTO playlists (id, name, position) VALUES (20, 'Opus', 0);
                 INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (20, 20, 0);",
            )
            .unwrap();
        save_smoke_profile(&conn, 20, TransferProfile::Opus160);
        let (device_root, runtime) = smoke_runtime(&conn).await;
        let observed = Rc::new(RefCell::new(Vec::new()));
        let phases = observed.clone();
        let _subscription = runtime.subscribe(Rc::new(move |state| {
            if let Some(device) = state
                .devices
                .iter()
                .find(|device| device.id == crate::ui::device_sync_smoke::DEVICE_ID)
            {
                phases.borrow_mut().push(device.sync_phase.clone());
            }
        }));

        let device = run_to_completion(&runtime).await;
        let output = device_root
            .path()
            .join("Music/Reprise/Artist/Album/01 Encoded.opus");
        assert!(std::fs::read(output).unwrap().starts_with(b"OggS"));
        assert!(device.last_sync.is_some(), "device state: {device:?}");
        assert_eq!(device.verified_managed_track_count, Some(1));
        assert!(device.sync_error.is_none());
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.page.changes.replacements, 0);
        assert!(observed.borrow().iter().any(|phase| matches!(
            phase,
            PlannedSyncPhase::Syncing {
                step: SyncStep::Transcoding,
                current_track,
                ..
            } if current_track == "Encoded — Artist"
        )));
        assert!(observed.borrow().iter().any(|phase| matches!(
            phase,
            PlannedSyncPhase::Syncing {
                step: SyncStep::Copying,
                current_track,
                ..
            } if current_track == "Encoded — Artist"
        )));
    });
}

#[test]
fn simulated_mtp_phone_preserves_original_flac_bytes_and_extension() {
    run(async {
        let (sources, conn) = fixture();
        conn.borrow()
            .execute_batch(
                "INSERT INTO playlists (id, name, position) VALUES (21, 'Original', 0);
                 INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (21, 1, 0);",
            )
            .unwrap();
        save_smoke_profile(&conn, 21, TransferProfile::Original);
        let expected = std::fs::read(sources.path().join("1.flac")).unwrap();
        let (device_root, runtime) = smoke_runtime(&conn).await;

        let device = run_to_completion(&runtime).await;
        let output = device_root
            .path()
            .join("Music/Reprise/Artist/Unknown Album/00 Track 1.flac");
        assert_eq!(std::fs::read(output).unwrap(), expected);
        assert!(device.last_sync.is_some(), "device state: {device:?}");
        assert_eq!(device.verified_managed_track_count, Some(1));
        assert!(device.sync_error.is_none());
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.page.changes.replacements, 0);
    });
}

#[test]
fn simulated_mtp_phone_transcodes_lossless_selection_to_mp3_256() {
    run(async {
        let (sources, conn) = fixture();
        let wav = sources.path().join("fallback.wav");
        write_silent_wav(&wav);
        conn.borrow()
            .execute(
                "INSERT INTO tracks (id,path,title,artist,album,album_artist,track_no,duration_ms,added_at) \
                 VALUES (22,?1,'Fallback','Artist','Album','Artist',2,100,0)",
                [wav.to_string_lossy().as_ref()],
            )
            .unwrap();
        conn.borrow()
            .execute_batch(
                "INSERT INTO playlists (id, name, position) VALUES (22, 'Fallback', 0);
                 INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (22, 22, 0);",
            )
            .unwrap();
        save_smoke_profile(&conn, 22, TransferProfile::Mp3(Mp3Quality::Kbps256));
        let (device_root, runtime) = smoke_runtime(&conn).await;

        let device = run_to_completion(&runtime).await;
        let output = device_root
            .path()
            .join("Music/Reprise/Artist/Album/02 Fallback.mp3");
        assert!(std::fs::read(output).unwrap().starts_with(b"ID3"));
        assert!(device.last_sync.is_some(), "device state: {device:?}");
        assert_eq!(device.verified_managed_track_count, Some(1));
        assert!(device.sync_error.is_none());
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.page.changes.replacements, 0);
    });
}

#[test]
fn simulated_mtp_phones_sync_independently_in_parallel() {
    run(async {
        const FIRST_ID: &str = "simulated-phone-a";
        const SECOND_ID: &str = "simulated-phone-b";
        let (sources, conn) = fixture();
        conn.borrow()
            .execute_batch(
                "INSERT INTO playlists (id, name, position) VALUES (23, 'Parallel', 0);
                 INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (23, 1, 0);",
            )
            .unwrap();
        save_profile(
            &conn,
            FIRST_ID,
            "Simulated Phone A",
            23,
            TransferProfile::Original,
        );
        save_profile(
            &conn,
            SECOND_ID,
            "Simulated Phone B",
            23,
            TransferProfile::Original,
        );
        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let backend = Rc::new(
            crate::ui::device_sync_smoke::SimulatedMtpDeviceBackend::for_devices(vec![
                (
                    FIRST_ID.into(),
                    "Simulated Phone A".into(),
                    first_root.path().to_path_buf(),
                ),
                (
                    SECOND_ID.into(),
                    "Simulated Phone B".into(),
                    second_root.path().to_path_buf(),
                ),
            ])
            .unwrap(),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        wait_for_storage(&runtime, 2).await;

        runtime.sync_now(FIRST_ID).unwrap();
        runtime.sync_now(SECOND_ID).unwrap();
        assert!(
            runtime
                .devices()
                .iter()
                .all(|device| device.page.controls.can_cancel),
            "both device operations must be active before the main context advances"
        );

        let first = wait_for_completion(&runtime, FIRST_ID).await;
        let second = wait_for_completion(&runtime, SECOND_ID).await;
        let relative = "Music/Reprise/Artist/Unknown Album/00 Track 1.flac";
        let expected = std::fs::read(sources.path().join("1.flac")).unwrap();
        assert_eq!(
            std::fs::read(first_root.path().join(relative)).unwrap(),
            expected
        );
        assert_eq!(
            std::fs::read(second_root.path().join(relative)).unwrap(),
            expected
        );
        assert_eq!(first.verified_managed_track_count, Some(1));
        assert_eq!(second.verified_managed_track_count, Some(1));
    });
}

#[test]
fn mtp_17_simulated_mtp_phone_removes_every_untracked_file_from_managed_storage() {
    run(async {
        let (_sources, conn) = fixture();
        conn.borrow()
            .execute_batch(
                "INSERT INTO playlists (id, name, position) VALUES (24, 'Preserve', 0);
                 INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (24, 1, 0);",
            )
            .unwrap();
        save_smoke_profile(&conn, 24, TransferProfile::Original);
        let device_root = tempfile::tempdir().unwrap();
        let managed_root = device_root.path().join("Music/Reprise");
        let foreign_audio = managed_root.join("Foreign/Existing.aiff");
        let foreign_playlist = managed_root.join("Old playlist.m3u8");
        let foreign_note = managed_root.join("notes.txt");
        std::fs::create_dir_all(foreign_audio.parent().unwrap()).unwrap();
        std::fs::write(&foreign_audio, b"untracked device audio").unwrap();
        std::fs::write(&foreign_playlist, b"#EXTM3U\nForeign/Existing.aiff\n").unwrap();
        std::fs::write(&foreign_note, b"not part of the mirror").unwrap();
        let backend = Rc::new(
            crate::ui::device_sync_smoke::SimulatedMtpDeviceBackend::for_root(device_root.path())
                .unwrap(),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        wait_for_storage(&runtime, 1).await;

        let device = run_to_completion(&runtime).await;

        assert!(!foreign_audio.exists());
        assert!(!foreign_playlist.exists());
        assert!(!foreign_note.exists());
        assert!(managed_root.join("Preserve.m3u8").exists());
        assert_eq!(device.page.changes.removals, 0);
        assert_eq!(device.page.changes.playlist_removals, 0);
        assert!(!device
            .page
            .warnings
            .contains(&SyncPageWarning::UnsafeManagedItem));
        assert_eq!(device.verified_managed_track_count, Some(1));
    });
}
