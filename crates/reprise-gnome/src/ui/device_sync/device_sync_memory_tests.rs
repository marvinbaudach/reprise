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

#[test]
fn a_later_volume_projection_replaces_the_daemon_mount_name_and_identity() {
    run(async {
        let (_temp, conn) = fixture();
        let root_uri = "mtp://Google_Pixel_10_Pro_XL_59100DLCQ006SB/";
        let daemon_mount = DeviceDescriptor {
            id: root_uri.into(),
            persistent_id: None,
            name: "mtp".into(),
            root_uri: root_uri.into(),
            reconnectable: false,
            icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        };
        let volume = DeviceDescriptor {
            id: "59100DLCQ006SB".into(),
            persistent_id: Some("59100DLCQ006SB".into()),
            name: "Pixel 10 Pro XL".into(),
            root_uri: root_uri.into(),
            reconnectable: true,
            icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        };
        let backend = Rc::new(FakeBackend::new(vec![daemon_mount], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let first = runtime.devices();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, root_uri);
        assert_eq!(first[0].name, "mtp");
        assert!(!first[0].rememberable);
        assert!(first[0].page.controls.editable);

        backend.set_devices(&[volume]);
        settle().await;

        let corrected = runtime.devices();
        assert_eq!(corrected.len(), 1, "the first projection must be replaced");
        assert_eq!(corrected[0].id, "59100DLCQ006SB");
        assert_eq!(corrected[0].name, "Pixel 10 Pro XL");
        assert!(corrected[0].connected);
        assert!(corrected[0].rememberable);
        assert_eq!(corrected[0].memory_status, None);
        assert!(corrected[0].page.controls.editable);
    });
}

#[test]
fn a_stored_placeholder_adopts_a_better_detected_name_but_a_custom_name_wins() {
    run(async {
        let (_temp, conn) = fixture();
        reprise_core::device_sync::settings::load_or_create_settings(&conn, "pixel", "mtp")
            .unwrap();
        let detected = DeviceDescriptor {
            id: "pixel".into(),
            persistent_id: Some("pixel".into()),
            name: "Google Pixel 8".into(),
            root_uri: "mtp://pixel/".into(),
            reconnectable: true,
            icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        };

        let runtime = DeviceSyncRuntime::with_backend(
            &conn,
            Rc::new(FakeBackend::new(vec![detected.clone()], 1)),
        );
        settle().await;
        assert_eq!(runtime.devices()[0].name, "Google Pixel 8");
        let persisted =
            reprise_core::device_sync::settings::load_or_create_settings(&conn, "pixel", "ignored")
                .unwrap();
        assert_eq!(persisted.device_name, "Google Pixel 8");

        reprise_core::device_sync::settings::rename_device(&conn, "pixel", "My phone").unwrap();
        let runtime =
            DeviceSyncRuntime::with_backend(&conn, Rc::new(FakeBackend::new(vec![detected], 1)));
        settle().await;
        assert_eq!(runtime.devices()[0].name, "My phone");
    });
}
