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

        assert!(matches!(
            runtime.devices()[0].sync_phase,
            PlannedSyncPhase::Syncing {
                bytes_done: 50,
                bytes_total: 85_636,
                ..
            }
        ));
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
fn mtp_5_reconnect_resumes_planned_sync_from_the_remaining_delta() {
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
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.page.changes.replacements, 0);
        assert_eq!(backend.state.copy_order.borrow().len(), 1);
    });
}

#[test]
fn mtp_15_every_copy_restarts_the_transfer_rate_baseline() {
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
