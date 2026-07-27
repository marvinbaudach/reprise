use super::*;

#[test]
fn playlist_publication_failure_preserves_paths_referenced_by_the_previous_snapshot() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_file(
            &conn.borrow(),
            &reprise_core::device_sync::DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 3,
                source_path: "/library/3.flac".into(),
                source_size: 100,
                source_mtime: 1,
                device_path: "Old/Three.mp3".into(),
                device_size: 100,
                profile_fingerprint: "legacy-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_playlist_error("injected playlist failure"),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (_subscription, completed) = signal_when(&runtime, |state| {
            state.devices[0].sync_phase == PlannedSyncPhase::Idle
                && state.devices[0].sync_error.is_some()
        });

        runtime.sync_now("a").unwrap();
        completed.recv().await.unwrap();

        assert!(
            backend.state.deleted.borrow().is_empty(),
            "old paths must survive until every playlist snapshot is published"
        );
        assert!(
            reprise_core::device_sync::settings::load_device_files(&conn.borrow(), "a")
                .unwrap()
                .iter()
                .any(|file| file.track_id == 3)
        );
    });
}

#[test]
fn cancellation_during_playlist_publication_skips_later_playlist_deletions() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_playlist(
            &conn.borrow(),
            &reprise_core::device_sync::DevicePlaylistRecord {
                device_serial: "a".into(),
                source: SelectionSource::Smart(99),
                source_name: "Old".into(),
                device_path: "Old.m3u8".into(),
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let (playlist_started, release_playlist) = backend.gate_playlist();
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        playlist_started.recv().await.unwrap();
        runtime.cancel_current("a");
        release_playlist.send(()).await.unwrap();
        settle().await;

        assert!(!backend
            .state
            .deleted
            .borrow()
            .iter()
            .any(|path| path == "Old.m3u8"));
        assert!(runtime.devices()[0].last_sync.is_none());
    });
}
