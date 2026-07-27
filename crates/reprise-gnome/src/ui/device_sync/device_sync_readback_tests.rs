use super::*;

#[test]
fn mtp_10_success_stays_finishing_until_device_contents_are_verified() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (inspection_started, release_inspection) = backend.gate_next_inspection();

        runtime.sync_now("a").unwrap();
        inspection_started.recv().await.unwrap();

        let device = runtime.devices().remove(0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Finishing);
        assert!(device.last_sync.is_none());
        assert_eq!(device.verified_managed_track_count, None);
        assert_eq!(
            device.page.controls,
            reprise_core::device_sync::SyncPageControls::default()
        );
        assert_eq!(runtime.sync_now("a"), Err(SyncStartError::Busy));
        assert_eq!(
            runtime.update_settings(device.settings),
            Err("device synchronization is active".into())
        );
        assert_eq!(
            reprise_core::device_sync::settings::load_device_playlists(&conn.borrow(), "a")
                .unwrap()
                .remove(0)
                .last_synced_at,
            None,
            "playlist publication alone must not claim a verified sync"
        );

        let (_subscription, verified) =
            signal_when(&runtime, |state| state.devices[0].last_sync.is_some());
        release_inspection.send(()).await.unwrap();
        verified.recv().await.unwrap();

        let device = runtime.devices().remove(0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert_eq!(device.verified_managed_track_count, Some(0));
        assert!(
            reprise_core::device_sync::settings::load_device_playlists(&conn.borrow(), "a")
                .unwrap()
                .remove(0)
                .last_synced_at
                .is_some()
        );
    });
}

#[test]
fn mtp_10_failed_readback_never_claims_a_successful_sync() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_playlist(
            &conn.borrow(),
            &reprise_core::device_sync::DevicePlaylistRecord {
                device_serial: "a".into(),
                source: SelectionSource::Playlist(10),
                source_name: "Road".into(),
                device_path: "Road.m3u8".into(),
                last_synced_at: Some(1_700_000_000),
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.fail_next_inspection("phone was locked");
        let (_subscription, finished) = signal_when(&runtime, |state| {
            state.devices[0].sync_phase == PlannedSyncPhase::Idle
                && state.devices[0].scan_error.is_some()
        });

        runtime.sync_now("a").unwrap();
        finished.recv().await.unwrap();

        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_none());
        assert_eq!(device.verified_managed_track_count, None);
        assert_eq!(device.scan_error.as_deref(), Some("phone was locked"));
        assert!(device
            .sync_error
            .as_ref()
            .is_some_and(|error| error.message.contains("could not verify device contents")));
        assert_eq!(
            reprise_core::device_sync::settings::load_device_playlists(&conn.borrow(), "a")
                .unwrap()
                .remove(0)
                .last_synced_at,
            Some(1_700_000_000)
        );
    });
}

#[test]
fn failed_playlist_timestamp_write_never_claims_a_successful_sync() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (inspection_started, release_inspection) = backend.gate_next_inspection();

        runtime.sync_now("a").unwrap();
        inspection_started.recv().await.unwrap();
        conn.borrow()
            .execute_batch(
                "CREATE TRIGGER reject_playlist_sync_timestamp
                 BEFORE UPDATE OF last_synced_at ON device_playlists
                 BEGIN
                   SELECT RAISE(FAIL, 'injected timestamp failure');
                 END;",
            )
            .unwrap();
        let (_subscription, finished) = signal_when(&runtime, |state| {
            state.devices[0].sync_phase == PlannedSyncPhase::Idle
                && state.devices[0].sync_error.is_some()
        });

        release_inspection.send(()).await.unwrap();
        finished.recv().await.unwrap();

        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_none());
        assert_eq!(device.verified_managed_track_count, None);
        assert!(device.sync_error.as_ref().is_some_and(|error| error
            .message
            .contains("could not record verified playlist synchronization")));
        assert_eq!(
            reprise_core::device_sync::settings::load_device_playlists(&conn.borrow(), "a")
                .unwrap()
                .remove(0)
                .last_synced_at,
            None
        );
    });
}
