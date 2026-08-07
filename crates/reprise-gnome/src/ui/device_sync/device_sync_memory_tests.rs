use super::*;

#[test]
fn mtp_49_unrememberable_device_records_its_run_without_persisting_device_state() {
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
        let mut legacy_settings = reprise_core::device_sync::settings::load_or_create_settings(
            &conn,
            root_uri,
            "Legacy transport row",
        )
        .unwrap();
        legacy_settings.sync_automatically = false;
        reprise_core::device_sync::settings::save_settings(&conn, &legacy_settings).unwrap();
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
        let (_subscription, completed) =
            signal_when(&runtime, |state| state.devices[0].last_sync.is_some());
        runtime.sync_now(root_uri).unwrap();
        let completed_before_timeout =
            futures_lite::future::race(async { completed.recv().await.is_ok() }, async {
                gtk4::glib::timeout_future(Duration::from_secs(2)).await;
                false
            })
            .await;
        assert!(
            completed_before_timeout,
            "the unrememberable sync must complete without a durable verification write"
        );
        assert!(
            backend
                .state
                .copy_order
                .borrow()
                .iter()
                .any(|(device_id, _)| device_id == root_uri),
            "an unrememberable phone still performs the requested transfer"
        );
        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.device_serial, root_uri);
        assert_eq!(
            recorded.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Completed
        );
        assert!(recorded.finished_at.is_some());

        let (device_settings, device_files, device_playlists, device_targets, last_verified_at): (
            i64,
            i64,
            i64,
            i64,
            Option<i64>,
        ) = crate::test_db::connection(&conn)
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM device_settings WHERE device_serial = ?1),
                   (SELECT COUNT(*) FROM device_files WHERE device_serial = ?1),
                   (SELECT COUNT(*) FROM device_playlists WHERE device_serial = ?1),
                   (SELECT COUNT(*) FROM device_sync_targets WHERE device_serial = ?1),
                   (SELECT last_verified_at FROM device_settings WHERE device_serial = ?1)",
                [root_uri],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(device_settings, 1, "the legacy fixture row remains present");
        assert_eq!(device_files, 0, "inventory stays identity-bound");
        assert_eq!(device_playlists, 0, "playlist memory stays identity-bound");
        assert_eq!(device_targets, 0, "target memory stays identity-bound");
        assert_eq!(
            last_verified_at, None,
            "verification timestamps stay identity-bound"
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

/// The refresh in `adopt_detected_device_name` is only safe because a
/// placeholder can never be *stored* deliberately. Typing one into the rename
/// dialog therefore means "go back to the detected name", exactly like leaving
/// the field empty — otherwise a user who called their phone "Unknown" would
/// have that silently overwritten on the next reconnect.
#[test]
fn renaming_a_device_to_a_placeholder_restores_the_detected_name() {
    run(async {
        let (_temp, conn) = fixture();
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

        for placeholder in ["mtp", "  Unknown Device  ", "MTP", ""] {
            runtime
                .rename_remembered_device("pixel", placeholder)
                .unwrap();
            assert_eq!(
                runtime.devices()[0].name,
                "Google Pixel 8",
                "{placeholder:?} means 'no name of my own'"
            );
        }

        // A real name still wins, and survives a reconnect.
        runtime
            .rename_remembered_device("pixel", "My phone")
            .unwrap();
        let runtime =
            DeviceSyncRuntime::with_backend(&conn, Rc::new(FakeBackend::new(vec![detected], 1)));
        settle().await;
        assert_eq!(runtime.devices()[0].name, "My phone");
    });
}
