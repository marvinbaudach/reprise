use super::*;

#[test]
fn mtp_49_runtime_lists_active_then_remembered_without_a_diff_and_supports_local_memory_actions() {
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
        let mut target = reprise_core::device_sync::load_or_create_targets(&conn, "pixel-anna")
            .unwrap()[0]
            .clone();
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
        assert_eq!(remembered.content_rows[0].target_path, "/Music/Anna");
        assert!(
            !reprise_core::device_sync::aggregate_balance(&remembered.category_readings).has_work(),
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
fn mtp_49_when_the_owner_disconnects_the_new_active_device_moves_above_history() {
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
