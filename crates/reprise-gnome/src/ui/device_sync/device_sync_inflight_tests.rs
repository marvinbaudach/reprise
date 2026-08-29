//! What happens to a device-sync run that is already in flight: cancellation,
//! stale progress from a superseded run, settings changes mid-run, resuming
//! after a reconnect, and the transfer-rate baseline.
//!
//! Split out of `device_sync_planned_tests.rs` when the dev merge pushed that
//! file past the 800-line gate. Every test drives the recording
//! `FakeBackend`, so none of them needs a phone.

use super::*;

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
        assert_eq!(device.page.changes.additions, 2);
        assert!(backend.state.copy_order.borrow().is_empty());
    });
}

#[test]
fn stale_progress_from_a_cancelled_run_does_not_update_its_replacement() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 40));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.set_devices(&[]);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.set_devices(&[descriptor("a", true)]);
        for _ in 0..100 {
            if backend.state.copy_attempts.get() == 2 {
                break;
            }
            gtk4::glib::timeout_future(Duration::from_millis(1)).await;
        }
        assert_eq!(backend.state.copy_attempts.get(), 2);
        gtk4::glib::timeout_future(Duration::from_millis(10)).await;

        assert!(
            matches!(
                runtime.devices()[0].sync_phase,
                PlannedSyncPhase::Syncing {
                    unit_bytes_done: 50,
                    unit_bytes_total: 100,
                    ..
                }
            ),
            "replacement phase: {:?}",
            runtime.devices()[0].sync_phase
        );
        settle().await;
    });
}

#[test]
fn settings_updates_are_rejected_before_persistence_while_syncing() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 40));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        runtime.sync_now("a").unwrap();

        let mut changed = runtime.devices()[0].settings.clone();
        changed.opus_bitrate = 192;
        let result = runtime.update_settings(changed);

        assert_eq!(result, Err("device synchronization is active".into()));
        assert!(matches!(
            runtime.devices()[0].sync_phase,
            PlannedSyncPhase::Syncing { .. }
        ));
        let persisted =
            reprise_core::device_sync::settings::load_or_create_settings(&conn, "a", "Phone a")
                .unwrap();
        assert_eq!(persisted.opus_bitrate, 0);
        runtime.cancel_current("a");
        settle().await;
    });
}

#[test]
fn unrelated_presence_events_do_not_reproject_a_device_mid_transfer() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        disable_auto_start(&conn, "b");
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("a", true), descriptor("b", true)],
            0,
        ));
        let (started, releases) = backend.gate_copies(&["a"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        runtime.sync_now("a").unwrap();
        assert_eq!(started.recv().await.unwrap(), "a");

        let before = runtime
            .devices()
            .into_iter()
            .find(|device| device.id == "a")
            .unwrap();
        reprise_core::library::playlist_membership::add_unique_tracks(&conn, 10, &[2]).unwrap();

        backend.set_devices(&[descriptor("a", true)]);

        let after = runtime
            .devices()
            .into_iter()
            .find(|device| device.id == "a")
            .unwrap();
        assert_eq!(after.page, before.page);
        assert_eq!(after.managed_track_count, before.managed_track_count);
        assert_eq!(after.size_on_device_bytes, before.size_on_device_bytes);
        assert_eq!(
            after.verified_managed_track_count,
            before.verified_managed_track_count
        );

        runtime.cancel_current("a");
        releases["a"].send(()).await.unwrap();
        settle().await;
    });
}

#[test]
fn mtp_5_reconnect_resumes_planned_sync_from_the_remaining_delta() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        crate::test_db::connection(&conn)
            .execute(
                "UPDATE tracks SET rating = 5, play_count = 31 WHERE id = 1",
                [],
            )
            .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.set_devices(&[]);
        gtk4::glib::timeout_future(Duration::from_millis(30)).await;
        assert!(!runtime.devices()[0].connected);
        assert!(runtime.devices()[0].last_sync.is_none());
        assert_eq!(
            runtime.devices()[0].storage,
            DeviceStorageSnapshot::default(),
            "unplug must clear measurements without losing the resumable run"
        );

        backend.set_devices(&[descriptor("a", true)]);
        settle().await;

        let device = runtime.devices().remove(0);
        assert!(device.connected);
        assert!(device.last_sync.is_some());
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.page.changes.replacements, 0);
        assert_eq!(backend.state.copy_order.borrow().len(), 1);
        let metadata = backend
            .state
            .managed_copy_contents
            .borrow()
            .iter()
            .find(|(_, path, _)| path == reprise_core::device_sync::track_metadata_list::FILE_NAME)
            .map(|(_, _, bytes)| {
                reprise_core::device_sync::track_metadata_list::TrackMetadataList::decode(bytes)
                    .unwrap()
            });
        let metadata = metadata.expect("a resumed listener sync must keep its metadata cargo");
        assert_eq!(metadata.entries.len(), 1);
        assert_eq!(
            (metadata.entries[0].rating, metadata.entries[0].play_count),
            (5, 31)
        );
    });
}

#[test]
fn mtp_60_every_copy_restarts_the_transfer_rate_baseline() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 0));
        let (started, releases) = backend.gate_copies(&["a"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();

        started.recv().await.unwrap();
        let after_first = runtime.devices()[0].bytes_per_second;
        releases["a"].send(()).await.unwrap();

        started.recv().await.unwrap();
        let after_second = runtime.devices()[0].bytes_per_second;
        releases["a"].send(()).await.unwrap();

        assert!(after_first > 0, "the first copy must produce a rate");
        // Progress counts from zero for every track. A baseline left over from
        // the previous track silently discards every smaller sample, and the
        // displayed rate then freezes for the rest of the run.
        assert_ne!(
            after_first, after_second,
            "the second copy must be measured against its own baseline"
        );
        settle().await;
    });
}

#[test]
fn a_last_step_failure_keeps_each_successful_playlists_verified_timestamp() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO playlists (id, name, position) VALUES (11, 'Night', 1);
                 INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (11, 2, 0);",
            )
            .unwrap();
        let mut settings =
            reprise_core::device_sync::settings::load_or_create_settings(&conn, "a", "Phone a")
                .unwrap();
        settings.selection = DeviceSelection::Sources(vec![
            SelectionSource::Playlist(10),
            SelectionSource::Playlist(11),
        ]);
        save_settings(&conn, &settings).unwrap();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_track_metadata_replace_error("injected final-step failure"),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(runtime.devices()[0]
            .sync_error
            .as_ref()
            .is_some_and(|error| error.message.contains("injected final-step failure")));
        let playlists =
            reprise_core::device_sync::settings::load_device_playlists(&conn, "a").unwrap();
        assert_eq!(playlists.len(), 2);
        assert!(playlists
            .iter()
            .all(|playlist| playlist.last_synced_at.is_some()));
    });
}
