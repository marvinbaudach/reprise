use super::*;

fn add_remembered_playlist(conn: &Rc<Db>, device_id: &str) {
    crate::test_db::connection(conn.as_ref())
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 0)",
            [],
        )
        .unwrap();
    crate::test_db::connection(conn.as_ref())
        .execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (10, 1, 0)",
            [],
        )
        .unwrap();
    let mut settings = reprise_core::device_sync::settings::load_or_create_settings(
        conn,
        device_id,
        "Remembered phone",
    )
    .unwrap();
    settings.selection = DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]);
    settings.sync_automatically = false;
    reprise_core::device_sync::settings::save_settings(conn, &settings).unwrap();
}

#[test]
fn remembered_device_projects_saved_playlists_on_startup_without_storage_measurements() {
    run(async {
        let (_temp, conn) = fixture();
        add_remembered_playlist(&conn, "remembered");
        let backend = Rc::new(FakeBackend::new(Vec::new(), 1));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        let remembered = runtime.devices().remove(0);

        assert_eq!(backend.state.inspection_roots.borrow().len(), 0);
        assert_eq!(remembered.storage, DeviceStorageSnapshot::default());
        assert_eq!(
            remembered.page.storage.current.knowledge,
            reprise_core::device_sync::StorageKnowledge::CapacityUnknown
        );
        assert!(remembered.page.blockers.is_empty());
        let road = remembered
            .page
            .playlists
            .iter()
            .find(|row| row.name.as_deref() == Some("Road"))
            .unwrap();
        assert!(road.selected);
    });
}

#[test]
fn unplugging_discards_live_storage_and_scan_inventory_but_keeps_library_projection() {
    run(async {
        let (_temp, conn) = fixture();
        add_remembered_playlist(&conn, "a");
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.managed_files.replace(vec![ManagedDeviceFile {
            relative_path: "Artist/Album/Track.opus".into(),
            size_bytes: 123,
        }]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let connected = runtime.devices().remove(0);
        assert_eq!(connected.storage.total_bytes, Some(2_000_000));
        assert_eq!(connected.content_row.item_count, 1);

        backend.set_devices(&[]);
        let remembered = runtime.devices().remove(0);

        assert_eq!(remembered.storage, DeviceStorageSnapshot::default());
        assert_eq!(remembered.content_row.item_count, 0);
        assert!(remembered
            .page
            .playlists
            .iter()
            .any(|row| row.name.as_deref() == Some("Road") && row.selected));
    });
}

#[test]
fn mtp_50_runtime_lists_active_then_remembered_without_a_diff_and_supports_local_memory_actions() {
    run(async {
        let (_temp, conn) = fixture();
        disable_auto_start(&conn, "active");
        reprise_core::device_sync::settings::load_or_create_settings(
            &conn,
            "pixel-anna",
            "Pixel 7a",
        )
        .unwrap();
        reprise_core::device_sync::settings::rename_device(&conn, "pixel-anna", "Pixel 7a (Anna)")
            .unwrap();
        reprise_core::device_sync::settings::record_device_verification(
            &conn,
            "pixel-anna",
            1_753_612_496,
            2_400_000_000,
        )
        .unwrap();
        let mut target =
            reprise_core::device_sync::load_or_create_target(&conn, "pixel-anna").unwrap();
        target.storage_id = Some(reprise_core::device_sync::StorageId(7));
        target.path = "/Music/Anna".into();
        reprise_core::device_sync::save_target(&conn, "pixel-anna", &target).unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("active", true)], 1));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;

        let devices = runtime.devices();
        assert_eq!(
            devices
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            ["active", "pixel-anna"]
        );
        let remembered = &devices[1];
        assert_eq!(remembered.name, "Pixel 7a (Anna)");
        assert!(!remembered.connected);
        assert_eq!(
            remembered.session_state,
            reprise_core::device_sync::DeviceSessionState::Remembered
        );
        assert_eq!(remembered.last_sync.unwrap().timestamp(), 1_753_612_496);
        assert_eq!(remembered.size_on_device_bytes, Some(2_400_000_000));
        assert_eq!(remembered.content_row.target_path, "/Music/Anna");
        assert!(
            !reprise_core::device_sync::aggregate_balance(&[remembered.target_reading]).has_work(),
            "an absent device must never project a guessed diff"
        );
        assert!(!remembered.page.controls.can_start);

        runtime
            .rename_remembered_device("pixel-anna", "Anna's phone")
            .unwrap();
        assert_eq!(runtime.devices()[1].name, "Anna's phone");
        runtime.forget_remembered_device("pixel-anna").unwrap();
        assert_eq!(
            runtime
                .devices()
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            ["active"]
        );
    });
}

#[test]
fn an_empty_local_name_resets_a_connected_device_to_its_detected_name() {
    run(async {
        let (_temp, conn) = fixture();
        disable_auto_start(&conn, "active");
        let backend = Rc::new(FakeBackend::new(vec![descriptor("active", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;

        runtime
            .rename_remembered_device("active", "My phone")
            .unwrap();
        assert_eq!(runtime.devices()[0].name, "My phone");
        runtime.rename_remembered_device("active", "   ").unwrap();
        assert_eq!(runtime.devices()[0].name, "Phone active");
    });
}

#[test]
fn mtp_50_when_the_owner_disconnects_the_new_active_device_moves_above_history() {
    run(async {
        let (_temp, conn) = fixture();
        disable_auto_start(&conn, "a");
        disable_auto_start(&conn, "b");
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("a", true), descriptor("b", true)],
            1,
        ));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        backend.set_devices(&[descriptor("b", true)]);
        settle().await;

        let devices = runtime.devices();
        assert_eq!(
            devices
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"],
            "the sole active connection must remain the first sidebar card"
        );
        assert_eq!(
            devices[0].session_state,
            reprise_core::device_sync::DeviceSessionState::Active
        );
        assert_eq!(
            devices[1].session_state,
            reprise_core::device_sync::DeviceSessionState::Remembered
        );
    });
}
