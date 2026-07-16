use super::*;

#[test]
fn device_view_projects_descriptor_scan_and_idle_state() {
    run(async {
        let (_temp, conn) = fixture();
        let descriptor = descriptor("a", true);
        let expected_icon = descriptor.icon.clone();
        let backend = Rc::new(FakeBackend::new(vec![descriptor], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let device = runtime.devices().remove(0);
        assert_eq!(device.id, "a");
        assert_eq!(device.name, "Phone a");
        assert_eq!(device.icon, expected_icon);
        assert!(device.connected);
        assert_eq!(device.available_bytes, Some(1_000_000));
        assert!(device.contents.files.is_empty());
        assert!(!device.scanning);
        assert!(device.scan_error.is_none());
        assert_eq!(device.snapshot.phase, SyncPhase::Idle);
    });
}

#[test]
fn connected_device_computes_its_persisted_selection_delta() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let device = runtime.devices().remove(0);
        assert_eq!(
            device.settings.selection,
            DeviceSelection::Sources(vec![SelectionSource::Playlist(10)])
        );
        assert_eq!(device.delta.unwrap().to_copy, [1, 2]);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert_eq!(device.tracks.len(), 2);
        assert!(device
            .tracks
            .iter()
            .all(|track| track.status == DeviceTrackStatus::Queued));
    });
}

#[test]
fn sync_now_copies_the_selection_and_commits_the_device_inventory() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(backend.state.max_total.get(), 1);
        assert_eq!(backend.state.copy_order.borrow().len(), 2);
        let playlists = backend.state.playlists.borrow();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].1, "Road");
        let playlist = String::from_utf8(playlists[0].2.clone()).unwrap();
        assert!(playlist.contains("Artist/Unknown Album/00 Track 1.flac"));
        assert!(playlist.contains("Artist/Unknown Album/00 Track 2.flac"));
        assert_eq!(
            reprise_core::device_sync::settings::load_device_files(&conn.borrow(), "a")
                .unwrap()
                .len(),
            2
        );
        let device = runtime.devices().remove(0);
        assert!(device.delta.unwrap().to_copy.is_empty());
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert!(device.last_sync.is_some());
    });
}

#[test]
fn sync_now_removes_unselected_files_but_preserves_pins() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        for (track_id, path, pinned) in [(3, "Old/Three.flac", false), (4, "Keep/Four.flac", true)]
        {
            reprise_core::device_sync::settings::upsert_device_file(
                &conn.borrow(),
                &reprise_core::device_sync::DeviceFileRecord {
                    device_serial: "a".into(),
                    track_id,
                    device_path: path.into(),
                    size: 100,
                    mtime: 1,
                    pinned,
                },
            )
            .unwrap();
        }
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(
            backend.state.deleted.borrow().as_slice(),
            ["Old/Three.flac"]
        );
        let ids = reprise_core::device_sync::settings::load_device_files(&conn.borrow(), "a")
            .unwrap()
            .into_iter()
            .map(|file| file.track_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [1, 4]);
    });
}

#[test]
fn insufficient_space_is_projected_as_a_device_warning() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1).with_available_bytes(Some(150)),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        assert!(matches!(
            runtime.sync_now("a"),
            Err(SyncStartError::InsufficientSpace {
                required_bytes: 200,
                available_bytes: 150,
            })
        ));
        let device = runtime.devices().remove(0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert!(device
            .sync_error
            .is_some_and(|error| error.message.contains("only 150 bytes are available")));
    });
}

#[test]
fn cancelling_planned_sync_keeps_remaining_delta_without_failure() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        runtime.cancel_current("a");
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert!(device.last_sync.is_none());
        assert!(device.sync_error.is_none());
        assert_eq!(device.delta.unwrap().to_copy, [1, 2]);
        assert!(backend.state.copy_order.borrow().is_empty());
    });
}

#[test]
fn reconnect_resumes_planned_sync_from_the_remaining_delta() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.set_devices(&[]);
        gtk4::glib::timeout_future(Duration::from_millis(30)).await;
        assert!(!runtime.devices()[0].connected);
        assert!(runtime.devices()[0].last_sync.is_none());

        backend.set_devices(&[descriptor("a", true)]);
        settle().await;

        let device = runtime.devices().remove(0);
        assert!(device.connected);
        assert!(device.last_sync.is_some());
        assert!(device.delta.unwrap().to_copy.is_empty());
        assert_eq!(backend.state.copy_order.borrow().len(), 1);
    });
}

#[test]
fn local_gio_sync_transcodes_lossless_selection_to_opus() {
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
        save_settings(
            &conn.borrow(),
            &DeviceSettings {
                device_serial: crate::ui::device_sync_smoke::DEVICE_ID.into(),
                device_name: "Android Smoke Device".into(),
                selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(20)]),
                opus_bitrate: 96,
                ratings_back: false,
                remove_deleted: true,
            },
        )
        .unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let backend = Rc::new(
            crate::ui::device_sync_smoke::SmokeDeviceBackend::for_root(device_root.path()).unwrap(),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime
            .sync_now(crate::ui::device_sync_smoke::DEVICE_ID)
            .unwrap();
        for _ in 0..1_000 {
            if runtime.devices()[0].last_sync.is_some() {
                break;
            }
            gtk4::glib::timeout_future(Duration::from_millis(5)).await;
        }

        let output = device_root
            .path()
            .join("Music/Reprise/Artist/Album/01 Encoded.opus");
        assert!(std::fs::read(output).unwrap().starts_with(b"OggS"));
        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_some(), "device state: {device:?}");
        assert!(device.sync_error.is_none());
    });
}

fn select_road_playlist(conn: &Rc<RefCell<Connection>>, ids: &[i64]) {
    conn.borrow()
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 0)",
            [],
        )
        .unwrap();
    for (position, track_id) in ids.iter().enumerate() {
        conn.borrow()
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (10, ?1, ?2)",
                rusqlite::params![track_id, position],
            )
            .unwrap();
    }
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: "a".into(),
            device_name: "Phone a".into(),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
        },
    )
    .unwrap();
}
