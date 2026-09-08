use super::*;

#[test]
fn first_replacement_after_upgrade_records_adoption_and_cleans_the_stale_spelling() {
    run(async {
        let (temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let planned = "Artist/Unknown Album/00 Track 1.opus";
        let adopted = "ARTIST/Unknown Album/00 Track 1.opus";
        reprise_core::device_sync::settings::upsert_device_file(
            &conn,
            &reprise_core::device_sync::DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: temp.path().join("1.flac").to_string_lossy().into_owned(),
                source_size: 100,
                source_mtime: 0,
                device_path: planned.into(),
                device_size: 100,
                profile_fingerprint: "legacy-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.return_copy_at(planned, adopted);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (_subscription, completed) =
            signal_when(&runtime, |state| state.devices[0].last_sync.is_some());

        runtime.sync_now("a").unwrap();
        completed.recv().await.unwrap();

        assert!(
            backend
                .state
                .deleted
                .borrow()
                .iter()
                .any(|path| path == planned),
            "the machine must emit cleanup for the stale pre-upgrade spelling"
        );
        let files = reprise_core::device_sync::settings::load_device_files(&conn, "a").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].device_path, adopted);
    });
}
