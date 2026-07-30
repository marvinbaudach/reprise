use super::*;
use reprise_core::agent_device_sync::{
    agent_device_sync_request, read_agent_device_sync_state, AgentDeviceSyncCommand,
    AgentDeviceSyncState, AgentDeviceSyncStorageAccess,
};
use std::sync::{Arc, Mutex};

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
        assert_eq!(device.storage.free_bytes, Some(1_000_000));
        assert_eq!(device.storage.total_bytes, Some(2_000_000));
        assert_eq!(
            device.storage.target_name.as_deref(),
            Some("Internal shared storage")
        );
        assert_eq!(device.storage.reprise_music_bytes, 0);
        assert_eq!(device.storage.other_music_bytes, 0);
        assert!(device.scan_error.is_none());
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
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
        assert_eq!(device.page.changes.additions, 2);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert_eq!(device.page.unique_track_count, 2);
    });
}

#[test]
fn planned_paths_preserve_existing_collision_slots_across_selection_changes() {
    run(async {
        let (_temp, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute(
                "UPDATE tracks SET title = 'Same', album = 'Album', album_artist = 'Artist', track_no = 1 WHERE id IN (1, 2, 3)",
                [],
            )
            .unwrap();
        select_road_playlist(&conn, &[1, 2]);
        for (track_id, path, pinned) in [
            (2, "Artist/Album/01 Same.flac", false),
            (3, "Artist/Album/01 Same (2).flac", true),
        ] {
            reprise_core::device_sync::settings::upsert_device_file(
                &conn,
                &reprise_core::device_sync::DeviceFileRecord {
                    device_serial: "a".into(),
                    track_id,
                    source_path: format!("/library/{track_id}.flac"),
                    source_size: 100,
                    source_mtime: 0,
                    device_path: path.into(),
                    device_size: 100,
                    profile_fingerprint: "legacy-v1".into(),
                    pinned,
                },
            )
            .unwrap();
        }
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (_subscription, completed) =
            signal_when(&runtime, |state| state.devices[0].last_sync.is_some());
        runtime.sync_now("a").unwrap();
        completed.recv().await.unwrap();
        let paths = backend
            .state
            .copy_order
            .borrow()
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();

        assert!(paths.iter().any(|path| path == "Artist/Album/01 Same.opus"));
        assert!(paths
            .iter()
            .any(|path| path == "Artist/Album/01 Same (3).opus"));
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
        assert!(playlist.contains("Artist/Unknown Album/00 Track 1.opus"));
        assert!(playlist.contains("Artist/Unknown Album/00 Track 2.opus"));
        assert_eq!(
            reprise_core::device_sync::settings::load_device_files(&conn, "a")
                .unwrap()
                .len(),
            2
        );
        let device = runtime.devices().remove(0);
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.page.changes.replacements, 0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert!(device.last_sync.is_some());
    });
}

/// `MTP-38`, finding 1: once the folder browser (`MTP-31`) has persisted a
/// storage for the Playlists target, `sync_now`'s actual transfer must
/// carry that `storage_id` through to the backend — not silently
/// reconstruct storage from the device root the way the pre-fix transfer
/// layer did (which always guessed "prefer internal" regardless of what
/// `set_target_folder` had just saved).
#[test]
fn mtp_38_sync_now_routes_the_playlists_transfer_through_its_persisted_storage() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime
            .set_target_folder(
                "a",
                SyncTargetKind::Playlists,
                Some(StorageId(7)),
                "/Music/Reprise".to_string(),
            )
            .unwrap();
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let recorded = backend.state.transfer_storage_ids.borrow().clone();
        assert!(
            !recorded.is_empty(),
            "the sync must have reached at least one transfer call"
        );
        assert!(
            recorded
                .iter()
                .all(|(target_path, storage_id)| target_path != "/Music/Reprise"
                    || *storage_id == Some(StorageId(7))),
            "every Playlists-target transfer must carry the persisted storage_id, got {recorded:?}"
        );
    });
}

#[test]
fn mtp_5_partial_cleanup_failure_blocks_every_planned_write() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_cleanup_error("injected cleanup failure"),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (_subscription, completed) = signal_when(&runtime, |state| {
            state.devices[0].sync_phase == PlannedSyncPhase::Idle
                && (state.devices[0].sync_error.is_some() || state.devices[0].last_sync.is_some())
        });

        runtime.sync_now("a").unwrap();
        completed.recv().await.unwrap();

        assert!(backend.state.copy_order.borrow().is_empty());
        assert!(backend.state.playlists.borrow().is_empty());
        assert!(backend.state.deleted.borrow().is_empty());
        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_none());
        let failure = device.sync_error.unwrap();
        assert!(failure.message.contains("injected cleanup failure"));
        assert!(failure.failed_tracks.is_empty());
    });
}

#[test]
fn known_read_only_target_is_rejected_at_the_runtime_boundary() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_storage_access(DeviceStorageAccess::ReadOnly),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        assert_eq!(
            runtime.sync_now("a"),
            Err(SyncStartError::Planning(
                "device storage is read-only".into()
            ))
        );
        assert!(backend.state.copy_order.borrow().is_empty());
    });
}

#[test]
fn mtp_47_cancelling_the_active_device_never_opens_or_queues_the_inert_device() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        save_road_settings(&conn, "b");
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("a", true), descriptor("b", true)],
            0,
        ));
        let (started, releases) = backend.gate_copies(&["a"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        assert_eq!(runtime.sync_now("b"), Err(SyncStartError::UnknownDevice));
        assert_eq!(started.recv().await.unwrap(), "a");

        runtime.cancel_current("a");
        releases["a"].send(()).await.unwrap();
        settle().await;

        assert_eq!(backend.state.max_total.get(), 1);
        assert!(
            reprise_core::device_sync::settings::load_device_files(&conn, "b")
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn replacement_inventory_is_committed_before_the_old_device_path_is_deleted() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_file(
            &conn,
            &reprise_core::device_sync::DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: "/old/library/1.flac".into(),
                source_size: 100,
                source_mtime: 0,
                device_path: "Old/Track 1.flac".into(),
                device_size: 100,
                profile_fingerprint: "legacy-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let paths_at_delete = Rc::new(RefCell::new(Vec::new()));
        let observed_paths = paths_at_delete.clone();
        let observed_conn = conn.clone();
        backend.observe_deletes(Rc::new(move |_| {
            let current =
                reprise_core::device_sync::settings::load_device_files(&observed_conn, "a")
                    .unwrap()
                    .into_iter()
                    .find(|file| file.track_id == 1)
                    .unwrap()
                    .device_path;
            observed_paths.borrow_mut().push(current);
        }));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (_subscription, completed) =
            signal_when(&runtime, |state| state.devices[0].last_sync.is_some());

        runtime.sync_now("a").unwrap();
        completed.recv().await.unwrap();

        assert_eq!(
            paths_at_delete.borrow().as_slice(),
            ["Artist/Unknown Album/00 Track 1.opus"]
        );
    });
}

#[test]
fn failed_replacement_inventory_preserves_the_old_device_path() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_file(
            &conn,
            &reprise_core::device_sync::DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: "/old/library/1.flac".into(),
                source_size: 100,
                source_mtime: 0,
                device_path: "Old/Track 1.flac".into(),
                device_size: 100,
                profile_fingerprint: "legacy-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "CREATE TRIGGER reject_replacement_inventory
                 BEFORE INSERT ON device_files
                 WHEN NEW.device_serial = 'a'
                   AND NEW.device_path <> 'Old/Track 1.flac'
                 BEGIN
                   SELECT RAISE(FAIL, 'injected inventory failure');
                 END;",
            )
            .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let (_subscription, completed) = signal_when(&runtime, |state| {
            state.devices[0].sync_phase == PlannedSyncPhase::Idle
                && state.devices[0].sync_error.is_some()
        });

        runtime.sync_now("a").unwrap();
        completed.recv().await.unwrap();

        assert!(backend.state.deleted.borrow().is_empty());
        let files = reprise_core::device_sync::settings::load_device_files(&conn, "a").unwrap();
        assert_eq!(files[0].device_path, "Old/Track 1.flac");
        assert_eq!(
            runtime.devices()[0]
                .sync_error
                .as_ref()
                .unwrap()
                .failed_tracks,
            [1]
        );
    });
}

#[test]
fn planned_transcodes_finish_before_each_corresponding_device_copy_starts() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let operations = backend
            .state
            .planned_operations
            .borrow()
            .iter()
            .map(|(_, operation)| *operation)
            .collect::<Vec<_>>();
        assert_eq!(operations, ["transcode", "copy", "transcode", "copy"]);
    });
}

#[test]
fn missing_selected_transcode_capability_blocks_before_any_managed_deletion_or_copy() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_file(
            &conn,
            &reprise_core::device_sync::DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 3,
                source_path: "/library/3.flac".into(),
                source_size: 100,
                source_mtime: 1,
                device_path: "Old/Three.mp3".into(),
                device_size: 100,
                profile_fingerprint: "legacy-opus-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_transcode_probe_error("opusenc is missing"),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        assert_eq!(
            runtime.sync_now("a"),
            Err(SyncStartError::Planning("opusenc is missing".into()))
        );
        assert!(backend.state.deleted.borrow().is_empty());
        assert!(backend.state.copy_order.borrow().is_empty());
        assert!(backend.state.planned_operations.borrow().is_empty());
    });
}

#[test]
fn agent_bridge_reports_the_compact_mirror_page_and_applies_multi_source_configuration() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let state = Arc::new(Mutex::new(AgentDeviceSyncState::default()));
        let (sender, receiver) = async_channel::unbounded();
        runtime.bind_agent_device_sync(&state, receiver);

        let snapshot = read_agent_device_sync_state(&state);
        assert_eq!(snapshot.devices[0].name, "Phone a");
        assert_eq!(
            snapshot.devices[0].profile,
            reprise_core::device_sync::TransferProfile::Opus160
        );
        assert_eq!(snapshot.devices[0].unique_track_count, 2);
        assert_eq!(snapshot.devices[0].changes.additions, 2);
        assert_eq!(
            snapshot.devices[0].storage.current.free_bytes,
            Some(1_000_000)
        );
        assert_eq!(
            snapshot.devices[0].storage.access,
            AgentDeviceSyncStorageAccess::Unknown
        );
        assert!(snapshot.devices[0].controls.can_start);
        assert!(snapshot.devices[0]
            .playlists
            .iter()
            .any(|playlist| playlist.source == SelectionSource::Playlist(10)
                && playlist.selected
                && playlist.entry_count == 2));
        assert!(snapshot.devices[0]
            .playlists
            .iter()
            .any(|playlist| playlist.source == SelectionSource::Smart(3) && !playlist.selected));

        let (request, reply) = agent_device_sync_request(AgentDeviceSyncCommand::Configure {
            device_name: "Phone a".into(),
            sources: vec![SelectionSource::Playlist(10), SelectionSource::Smart(3)],
            profile: reprise_core::device_sync::TransferProfile::Original,
        });
        sender.send(request).await.unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        assert_eq!(reply.try_recv(), Ok(Ok(())));
        assert!(runtime.devices()[0].settings.remove_deleted);
        assert_eq!(
            runtime.devices()[0].settings.profile,
            reprise_core::device_sync::TransferProfile::Original
        );
        assert_eq!(runtime.devices()[0].settings.opus_bitrate, 0);
        assert_eq!(
            runtime.devices()[0].settings.selection,
            DeviceSelection::Sources(vec![
                SelectionSource::Playlist(10),
                SelectionSource::Smart(3),
            ])
        );

        let (request, reply) = agent_device_sync_request(AgentDeviceSyncCommand::Configure {
            device_name: "Phone a".into(),
            sources: vec![SelectionSource::Playlist(10), SelectionSource::Playlist(10)],
            profile: reprise_core::device_sync::TransferProfile::Mp3(
                reprise_core::device_sync::Mp3Quality::Kbps256,
            ),
        });
        sender.send(request).await.unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        assert!(reply
            .try_recv()
            .unwrap()
            .unwrap_err()
            .contains("duplicates"));
        assert_eq!(
            runtime.devices()[0].settings.profile,
            reprise_core::device_sync::TransferProfile::Original
        );

        let (request, reply) = agent_device_sync_request(AgentDeviceSyncCommand::Start {
            device_name: "Missing phone".into(),
        });
        sender.send(request).await.unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        assert!(reply
            .try_recv()
            .unwrap()
            .unwrap_err()
            .contains("absent, disconnected, or ambiguous"));

        let (request, reply) = agent_device_sync_request(AgentDeviceSyncCommand::Eject {
            device_name: "Phone a".into(),
        });
        sender.send(request).await.unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        assert_eq!(reply.try_recv(), Ok(Ok(())));
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        assert_eq!(backend.state.ejected.borrow().as_slice(), ["a"]);
    });
}

#[test]
fn planned_sync_refreshes_available_space_after_finishing() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        assert_eq!(runtime.devices()[0].storage.free_bytes, Some(1_000_000));
        backend.set_available_bytes(Some(900_000));

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(runtime.devices()[0].storage.free_bytes, Some(900_000));
    });
}

#[test]
fn sync_now_mirrors_the_selection_without_legacy_pin_exceptions() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        for (track_id, path, pinned) in [(3, "Old/Three.flac", false), (4, "Keep/Four.flac", true)]
        {
            reprise_core::device_sync::settings::upsert_device_file(
                &conn,
                &reprise_core::device_sync::DeviceFileRecord {
                    device_serial: "a".into(),
                    track_id,
                    source_path: format!("/library/{track_id}.flac"),
                    source_size: 100,
                    source_mtime: 1,
                    device_path: path.into(),
                    device_size: 100,
                    profile_fingerprint: "legacy-v1".into(),
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
            ["Old/Three.flac", "Keep/Four.flac"]
        );
        let ids = reprise_core::device_sync::settings::load_device_files(&conn, "a")
            .unwrap()
            .into_iter()
            .map(|file| file.track_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [1]);
    });
}

#[test]
fn insufficient_space_is_projected_as_a_device_warning() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1).with_available_bytes(Some(50_000)),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        assert!(matches!(
            runtime.sync_now("a"),
            Err(SyncStartError::InsufficientSpace {
                required_bytes: 171_272,
                available_bytes: 50_000,
            })
        ));
        let device = runtime.devices().remove(0);
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
        assert!(device
            .sync_error
            .is_some_and(|error| error.message.contains("only 50000 bytes are available")));
    });
}
