use super::*;

#[test]
fn mtp_49_unrememberable_device_is_usable_without_persisting_the_volatile_uri() {
    run(async {
        let (_temp, conn) = fixture();
        let root_uri = "mtp://[usb:001,013]/";
        let device = DeviceDescriptor {
            id: root_uri.into(),
            persistent_id: None,
            name: "Unknown Android phone".into(),
            root_uri: root_uri.into(),
            reconnectable: false,
            icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        };
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 0)",
                [],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position)
                 VALUES (10, 1, 0)",
                [],
            )
            .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![device], 1));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let view = runtime.devices().into_iter().next().unwrap();
        assert!(view.page.controls.editable, "the device remains usable");
        assert!(!view.rememberable);
        assert_eq!(
            view.memory_status.as_deref(),
            Some("This device can be used now but cannot be remembered")
        );
        runtime
            .set_transfer_profile(
                root_uri,
                reprise_core::device_sync::TransferProfile::Original,
            )
            .unwrap();
        runtime
            .set_playlist_selected(root_uri, SelectionSource::Playlist(10), true)
            .unwrap();
        runtime.sync_now(root_uri).unwrap();
        settle().await;
        assert!(
            backend
                .state
                .copy_order
                .borrow()
                .iter()
                .any(|(device_id, _)| device_id == root_uri),
            "an unrememberable phone still performs the requested transfer"
        );
        let persisted: i64 = crate::test_db::connection(&conn)
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM device_settings WHERE device_serial = ?1) +
                   (SELECT COUNT(*) FROM device_sync_targets WHERE device_serial = ?1) +
                   (SELECT COUNT(*) FROM device_files WHERE device_serial = ?1) +
                   (SELECT COUNT(*) FROM device_playlists WHERE device_serial = ?1) +
                   (SELECT COUNT(*) FROM sync_runs WHERE device_serial = ?1)",
                [root_uri],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            persisted, 0,
            "the volatile MTP URI must never be used as a memory key"
        );
    });
}
