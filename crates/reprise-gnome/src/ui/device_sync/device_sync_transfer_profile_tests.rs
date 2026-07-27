use super::*;
use reprise_core::device_sync::{Mp3Quality, TransferProfile};

fn save_smoke_profile(conn: &Rc<RefCell<Connection>>, playlist_id: i64, profile: TransferProfile) {
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: crate::ui::device_sync_smoke::DEVICE_ID.into(),
            device_name: crate::ui::device_sync_smoke::DEVICE_NAME.into(),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(playlist_id)]),
            profile,
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
        },
    )
    .unwrap();
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
    for _ in 0..1_000 {
        if runtime.devices()[0].sync_phase != PlannedSyncPhase::ComputingDelta {
            break;
        }
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
    }
    assert_ne!(
        runtime.devices()[0].sync_phase,
        PlannedSyncPhase::ComputingDelta,
        "storage inspection must settle before the transfer starts"
    );
    (device_root, runtime)
}

async fn run_to_completion(runtime: &Rc<DeviceSyncRuntime>) -> DeviceView {
    runtime
        .sync_now(crate::ui::device_sync_smoke::DEVICE_ID)
        .unwrap();
    for _ in 0..1_000 {
        if runtime.devices()[0].last_sync.is_some() {
            break;
        }
        gtk4::glib::timeout_future(Duration::from_millis(5)).await;
    }
    runtime.devices().remove(0)
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
