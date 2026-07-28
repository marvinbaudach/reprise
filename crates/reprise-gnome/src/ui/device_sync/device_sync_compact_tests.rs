use super::*;
use reprise_core::device_sync::settings::{
    load_device_playlists, load_or_create_settings, upsert_device_file, upsert_device_playlist,
};
use reprise_core::device_sync::{
    DeviceFileRecord, DevicePlaylistRecord, MirrorBlocker, StorageProjectionState, TransferProfile,
};

struct FailingCopyBackend {
    device: DeviceDescriptor,
    playlist_writes: Rc<Cell<usize>>,
}

impl DeviceBackend for FailingCopyBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        vec![self.device.clone()]
    }

    fn subscribe_devices(&self, _callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {}

    fn inspect(
        &self,
        _root_uri: String,
        _targets: [reprise_core::device_sync::SyncTarget; 3],
    ) -> TestFuture<DeviceStorageInspection> {
        Box::pin(async {
            Ok(DeviceStorageInspection {
                snapshot: DeviceStorageSnapshot {
                    free_bytes: Some(1_000_000),
                    total_bytes: Some(2_000_000),
                    ..Default::default()
                },
                managed_files: Vec::new(),
                podcast_files: Vec::new(),
                youtube_files: Vec::new(),
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        _device_id: String,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<reprise_core::device_sync::StorageId>,
        _source_path: PathBuf,
        _relative_target: String,
        _expected_size: u64,
        _cancellable: gio::Cancellable,
        _progress: Rc<dyn Fn(u64, u64)>,
    ) -> TestFuture<CopyOutcome> {
        Box::pin(async { Err("injected copy failure".into()) })
    }

    fn replace_playlist(
        &self,
        _device_id: String,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<reprise_core::device_sync::StorageId>,
        _name: String,
        _contents: Vec<u8>,
    ) -> TestFuture<()> {
        let writes = self.playlist_writes.clone();
        Box::pin(async move {
            writes.set(writes.get() + 1);
            Ok(())
        })
    }
}

fn add_playlist(conn: &Rc<RefCell<Connection>>, id: i64, name: &str, track_ids: &[i64]) {
    conn.borrow()
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, id],
        )
        .unwrap();
    for (position, track_id) in track_ids.iter().enumerate() {
        conn.borrow()
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, track_id, position as i64],
            )
            .unwrap();
    }
}

fn save_sources(conn: &Rc<RefCell<Connection>>, device_id: &str, sources: Vec<SelectionSource>) {
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(sources),
            profile: TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            // `MTP-30`: these tests drive `sync_now` manually and must not
            // race an automatic start on connect.
            sync_automatically: false,
        },
    )
    .unwrap();
}

#[test]
fn compact_page_projects_profile_playlist_sizes_deduplicated_delta_and_storage() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Road", &[1, 2, 1]);
        add_playlist(&conn, 11, "Mix", &[2, 3]);
        save_sources(
            &conn,
            "a",
            vec![SelectionSource::Playlist(10), SelectionSource::Playlist(11)],
        );
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let page = runtime.devices().remove(0).page;
        assert_eq!(page.profile_options, TransferProfile::ALL);
        assert_eq!(page.profile, TransferProfile::Opus160);
        assert_eq!(page.unique_track_count, 3);
        assert_eq!(page.target_bytes, 256_908);
        assert_eq!(page.changes.additions, 3);
        assert_eq!(page.changes.replacements, 0);
        assert_eq!(page.changes.removals, 0);
        assert_eq!(page.changes.transfer_bytes, 256_908);
        assert_eq!(page.storage.state, StorageProjectionState::Fits);
        assert_eq!(
            page.storage
                .after_sync
                .as_ref()
                .and_then(|composition| composition.free_bytes),
            Some(743_092)
        );
        assert_eq!(
            page.playlists
                .iter()
                .filter(|row| row.selected)
                .map(|row| (
                    row.name.as_deref(),
                    row.selected,
                    row.entry_count,
                    row.unique_track_count,
                    row.target_bytes,
                ))
                .collect::<Vec<_>>(),
            [
                (Some("Mix"), true, 2, 2, 171_272),
                (Some("Road"), true, 3, 2, 171_272),
            ]
        );
        assert!(page
            .playlists
            .iter()
            .any(|row| row.smart && row.name.as_deref() == Some("Recently added")));
        assert!(page.controls.editable);
        assert!(page.controls.can_start);
        assert!(!page.controls.can_cancel);
    });
}

#[test]
fn mtp_14_playlists_are_selectable_while_device_storage_is_still_being_checked() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Road", &[1, 2]);
        save_sources(&conn, "a", Vec::new());
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let (inspection_started, release_inspection) = backend.gate_next_inspection();

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        inspection_started.recv().await.unwrap();

        let device = runtime.devices().remove(0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::ComputingDelta);
        assert_eq!(
            device
                .page
                .playlists
                .iter()
                .map(|row| row.name.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("Recently added"),
                Some("Recently played"),
                Some("Road"),
                Some("Top rated")
            ]
        );
        assert!(device.page.controls.editable);
        assert!(!device.page.controls.can_start);

        release_inspection.send(()).await.unwrap();
    });
}

#[test]
fn profile_and_playlist_changes_persist_and_recompute_the_page_immediately() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Road", &[1, 2]);
        add_playlist(&conn, 11, "Mix", &[2, 3]);
        save_sources(
            &conn,
            "a",
            vec![SelectionSource::Playlist(10), SelectionSource::Playlist(11)],
        );
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime
            .set_transfer_profile("a", TransferProfile::Original)
            .unwrap();
        let page = runtime.devices().remove(0).page;
        assert_eq!(page.profile, TransferProfile::Original);
        assert_eq!(page.target_bytes, 300);

        runtime
            .set_playlist_selected("a", SelectionSource::Playlist(11), false)
            .unwrap();
        let page = runtime.devices().remove(0).page;
        assert_eq!(page.unique_track_count, 2);
        assert_eq!(page.target_bytes, 200);
        assert!(
            !page
                .playlists
                .iter()
                .find(|row| row.source == SelectionSource::Playlist(11))
                .unwrap()
                .selected
        );

        let persisted = load_or_create_settings(&conn.borrow(), "a", "Phone a").unwrap();
        assert_eq!(persisted.profile, TransferProfile::Original);
        assert_eq!(
            persisted.selection,
            DeviceSelection::Sources(vec![SelectionSource::Playlist(10)])
        );
    });
}

#[test]
fn mtp_16_transfer_profile_survives_runtime_recreation_for_the_same_device() {
    run(async {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("reprise.db");
        {
            let conn = Rc::new(RefCell::new(Connection::open(&database).unwrap()));
            reprise_core::db::migrate(&conn.borrow()).unwrap();
            let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
            let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
            gtk4::glib::timeout_future(Duration::from_millis(2)).await;

            runtime
                .set_transfer_profile("a", TransferProfile::Original)
                .unwrap();
            assert_eq!(runtime.devices()[0].page.profile, TransferProfile::Original);
        }

        let conn = Rc::new(RefCell::new(Connection::open(&database).unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let restored = runtime.devices().remove(0);
        assert_eq!(restored.settings.profile, TransferProfile::Original);
        assert_eq!(restored.page.profile, TransferProfile::Original);
    });
}

#[test]
fn missing_or_empty_selected_playlists_block_start_without_planning_deletions() {
    run(async {
        let (_temp, conn) = fixture();
        save_sources(&conn, "a", vec![SelectionSource::Playlist(99)]);
        upsert_device_file(
            &conn.borrow(),
            &DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: "/old/one.flac".into(),
                source_size: 100,
                source_mtime: 1,
                device_path: "Old/One.mp3".into(),
                device_size: 100,
                profile_fingerprint: "legacy-v1".into(),
                pinned: true,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let page = runtime.devices().remove(0).page;
        assert_eq!(
            page.blockers,
            [MirrorBlocker::MissingPlaylist(SelectionSource::Playlist(
                99
            ))]
        );
        assert_eq!(page.changes.removals, 0);
        assert!(!page.controls.can_start);
        assert!(matches!(
            runtime.sync_now("a"),
            Err(SyncStartError::Planning(_))
        ));
        assert!(backend.state.deleted.borrow().is_empty());

        runtime
            .set_playlist_selected("a", SelectionSource::Playlist(99), false)
            .unwrap();
        let page = runtime.devices().remove(0).page;
        assert_eq!(page.blockers, [MirrorBlocker::NoPlaylistsSelected]);
        assert_eq!(page.changes.removals, 0);
        assert!(!page.controls.can_start);
    });
}

#[test]
fn unavailable_selected_tracks_are_retained_and_written_to_the_playlist() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Offline", &[1]);
        conn.borrow()
            .execute(
                "UPDATE tracks SET missing_since = 1, missing_reason = 'unmounted' WHERE id = 1",
                [],
            )
            .unwrap();
        save_sources(&conn, "a", vec![SelectionSource::Playlist(10)]);
        upsert_device_file(
            &conn.borrow(),
            &DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: "/library/one.flac".into(),
                source_size: 100,
                source_mtime: 1,
                device_path: "Artist/Album/01 Track 1.mp3".into(),
                device_size: 100,
                profile_fingerprint: "mp3-cbr-256-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let page = runtime.devices().remove(0).page;
        assert_eq!(page.playlists[0].unavailable_count, 1);
        assert_eq!(page.changes.retained_unavailable, 1);
        assert_eq!(page.changes.removals, 0);
        assert_eq!(page.target_bytes, 100);
        assert!(page.controls.can_start);

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(backend.state.deleted.borrow().is_empty());
        assert!(backend.state.copy_order.borrow().is_empty());
        let playlists = backend.state.playlists.borrow();
        assert_eq!(playlists.len(), 1);
        assert!(String::from_utf8_lossy(&playlists[0].2).contains("Artist/Album/01 Track 1.mp3"));
    });
}

#[test]
fn deleted_library_rows_are_removed_instead_of_being_retained_as_unavailable() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Road", &[1]);
        conn.borrow()
            .execute("UPDATE tracks SET removed_at = 1 WHERE id = 1", [])
            .unwrap();
        save_sources(&conn, "a", vec![SelectionSource::Playlist(10)]);
        upsert_device_file(
            &conn.borrow(),
            &DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: "/library/one.flac".into(),
                source_size: 100,
                source_mtime: 1,
                device_path: "Artist/Album/01 Track 1.mp3".into(),
                device_size: 100,
                profile_fingerprint: "mp3-cbr-256-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let page = runtime.devices().remove(0).page;
        assert_eq!(page.changes.retained_unavailable, 0);
        assert_eq!(page.changes.removals, 1);

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(
            backend.state.deleted.borrow().as_slice(),
            ["Artist/Album/01 Track 1.mp3"]
        );
    });
}

#[test]
fn successful_playlist_rename_writes_inventory_before_removing_the_old_m3u() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Renamed Road", &[1]);
        save_sources(&conn, "a", vec![SelectionSource::Playlist(10)]);
        upsert_device_playlist(
            &conn.borrow(),
            &DevicePlaylistRecord {
                device_serial: "a".into(),
                source: SelectionSource::Playlist(10),
                source_name: "Old Road".into(),
                device_path: "Old Road.m3u8".into(),
                last_synced_at: None,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(backend
            .state
            .playlists
            .borrow()
            .iter()
            .any(|(_, name, _)| name == "Renamed Road"));
        assert!(backend
            .state
            .deleted
            .borrow()
            .iter()
            .any(|path| path == "Old Road.m3u8"));
        let inventory = load_device_playlists(&conn.borrow(), "a").unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].source_name, "Renamed Road");
        assert_eq!(inventory[0].device_path, "Renamed Road.m3u8");
    });
}

#[test]
fn a_failed_track_copy_does_not_publish_a_playlist_with_dead_new_paths() {
    run(async {
        let (temp, conn) = fixture();
        let mp3 = temp.path().join("copy.mp3");
        std::fs::write(&mp3, vec![1_u8; 100]).unwrap();
        conn.borrow()
            .execute(
                "UPDATE tracks SET path = ?1, bitrate_kbps = 128 WHERE id = 1",
                [mp3.to_string_lossy().as_ref()],
            )
            .unwrap();
        add_playlist(&conn, 10, "Road", &[1]);
        save_sources(&conn, "a", vec![SelectionSource::Playlist(10)]);
        let writes = Rc::new(Cell::new(0));
        let backend = Rc::new(FailingCopyBackend {
            device: descriptor("a", true),
            playlist_writes: writes.clone(),
        });
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(writes.get(), 0);
        assert!(runtime.devices()[0].sync_error.is_some());
    });
}

#[test]
fn active_device_controls_lock_only_that_device_and_offer_cancel() {
    run(async {
        let (_temp, conn) = fixture();
        add_playlist(&conn, 10, "Road", &[1]);
        save_sources(&conn, "a", vec![SelectionSource::Playlist(10)]);
        save_sources(&conn, "b", vec![SelectionSource::Playlist(10)]);
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("a", true), descriptor("b", true)],
            0,
        ));
        let (started, releases) = backend.gate_copies(&["a"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        assert_eq!(started.recv().await.unwrap(), "a");
        let devices = runtime.devices();
        let a = devices.iter().find(|device| device.id == "a").unwrap();
        let b = devices.iter().find(|device| device.id == "b").unwrap();
        assert!(!a.page.controls.editable);
        assert!(!a.page.controls.can_start);
        assert!(a.page.controls.can_cancel);
        assert!(b.page.controls.editable);
        assert!(b.page.controls.can_start);
        assert!(!b.page.controls.can_cancel);

        runtime.cancel_current("a");
        releases["a"].send(()).await.unwrap();
        settle().await;
    });
}
