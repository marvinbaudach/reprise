use super::*;

#[test]
fn mtp_48_runtime_opens_exactly_one_session_and_never_scans_the_inert_device() {
    run(async {
        let (_temp, conn) = fixture();
        disable_auto_start(&conn, "a");
        select_road_playlist(&conn, &[1]);
        save_road_settings(&conn, "b");
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("a", true), descriptor("b", true)],
            1,
        ));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let devices = runtime.devices();
        assert_eq!(devices.len(), 2, "the waiting device remains listed");
        assert_eq!(
            backend.state.inspection_roots.borrow().as_slice(),
            ["mtp://a"],
            "only the first detected device may open and inspect an MTP session"
        );
        let inert = devices.iter().find(|device| device.id == "b").unwrap();
        assert!(inert.page.controls.editable);
        assert!(!inert.page.controls.can_start);
        assert!(!inert.page.controls.can_eject);
        assert!(inert
            .page
            .playlists
            .iter()
            .any(|row| row.name.as_deref() == Some("Road") && row.selected));
        assert!(
            runtime.sync_now("b").is_err(),
            "an inert row must never expose a working sync action"
        );
    });
}

#[test]
fn reconnect_recompute_clears_the_library_stale_marker() {
    run(async {
        let (_temp, conn) = fixture();
        disable_auto_start(&conn, "a");
        let connected = descriptor("a", true);
        let backend = Rc::new(FakeBackend::new(vec![connected.clone()], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        backend.set_devices(&[]);
        runtime.mark_all_devices_stale();
        reprise_core::library::playlists::create(&conn, "Written while disconnected").unwrap();

        backend.set_devices(&[connected]);
        settle().await;
        assert!(runtime.devices()[0]
            .page
            .playlists
            .iter()
            .any(|playlist| playlist.name.as_deref() == Some("Written while disconnected")));

        reprise_core::library::playlists::create(&conn, "Written after reconnect").unwrap();
        let snapshot = runtime.picker_snapshot_fresh("a").unwrap();
        assert!(snapshot
            .rows
            .iter()
            .all(|playlist| playlist.name != "Written after reconnect"));
    });
}
