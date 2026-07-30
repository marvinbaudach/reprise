//! `MTP-23`: the content phase's failure and cancellation contract for the
//! two additive targets (podcast episodes, YouTube audio).
//!
//! An external review found two P1 defects in `run_content_phase`
//! (`device_sync_content_transfer.rs`):
//!
//! 1. A failed content copy did not stop the phases after it — the second
//!    copy target and both removal phases still ran. Combined with cap
//!    eviction (oldest-first), a failure to copy a new episode could still
//!    delete the older resident episode it was meant to replace: the device
//!    loses a file and gains nothing.
//! 2. Cancelling mid content-phase only broke the inner loops. With no
//!    recorded failure, the phase fell through to the mirror's original
//!    `Completed` outcome, so the run log closed as a success and reconnect
//!    resumability was cleared — the remaining work was silently forgotten.
//!
//! Both tests below reproduce the scenario from scratch rather than calling
//! `run_content_phase` directly, so they exercise the exact path a real sync
//! takes (`sync_now` → the mirror machine → the content phase → `finish_sync`).

use super::*;

fn podcast_settings(device_id: &str) -> DeviceSettings {
    DeviceSettings {
        device_serial: device_id.into(),
        device_name: format!("Phone {device_id}"),
        // No library sources selected: the mirror has nothing to copy, so
        // every `replace_track` call in these tests is unambiguously a
        // content-phase (podcast/YouTube) copy, never a music-mirror one.
        selection: DeviceSelection::Sources(Vec::new()),
        profile: reprise_core::device_sync::TransferProfile::default(),
        opus_bitrate: 0,
        ratings_back: false,
        remove_deleted: true,
        sync_automatically: false,
        // These tests seed episodes that already have a local file, so the
        // preparation phase (`MTP-42`) has nothing to do either way. Off, so
        // the run under test is unambiguously the transfer, never a download.
        prepare_before_sync: false,
    }
}

/// Polls `receiver` for up to one second instead of awaiting it directly.
/// An unbounded `.recv().await` here would hang the whole test binary if the
/// content phase ever stopped reaching the copy call this gate is meant to
/// observe — turning a regression into a stall no CI would report, exactly
/// the failure mode `17ae47a1` fixed for the podcast download queue. Bounded
/// polling on the GLib main loop keeps this test single-threaded like every
/// other test in this module.
async fn recv_soon(receiver: &async_channel::Receiver<String>) -> String {
    const ATTEMPTS: usize = 500;
    for _ in 0..ATTEMPTS {
        if let Ok(value) = receiver.try_recv() {
            return value;
        }
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
    }
    panic!(
        "did not observe a content copy start within {}ms — the content phase \
         never reached the gated replace_track call this test depends on",
        ATTEMPTS * 2
    );
}

/// Defect 1: a failed podcast/YouTube copy must stop the run before either
/// removal phase touches the device, or a failure to add a new episode can
/// still delete the older one it was meant to replace.
#[test]
fn mtp_23_a_failed_content_copy_must_stop_the_later_removal_phases() {
    run(async {
        let (downloads, conn) = fixture();
        let rss_path = downloads.path().join("rss.mp3");
        std::fs::write(&rss_path, b"rss audio").unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'rss', 'https://example.test/rss', 'RSS Show', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'New Episode', 'https://example.test/rss.mp3',
                         ?1, 9, 1)",
                [rss_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        save_settings(&conn, &podcast_settings("a")).unwrap();

        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_replace_track_error("device refused the write (simulated full storage)"),
        );
        // Already resident from an earlier sync, unsubscribed since — with
        // `remove_deleted = true` this is exactly the kind of removal the
        // cap-eviction case (`MTP-39`/`MTP-25`) pairs with a copy failure to
        // produce real data loss: the new episode never lands, but the old
        // one this run planned to evict still would, unless the guard holds.
        backend.state.podcast_files.replace(vec![ManagedDeviceFile {
            relative_path: "Old Show/99-Old.mp3".into(),
            size_bytes: 4,
        }]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let page = runtime.devices().remove(0).page;
        assert!(
            page.controls.can_start,
            "the fixture must plan both a copy and a removal for this test to mean anything"
        );

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(
            backend.state.managed_deleted.borrow().is_empty(),
            "a failed content copy must not let either removal phase run — the device \
             would lose the resident episode without ever gaining the new one, got {:?}",
            backend.state.managed_deleted.borrow()
        );
        assert!(
            backend.state.managed_copies.borrow().is_empty(),
            "the copy was made to fail, so nothing should have actually landed"
        );
    });
}

/// Defect 2: cancelling while the content phase is running must not be
/// reported as a completed run. Reaching the mirror's `Finished` outcome
/// only means the additive copies have not started yet — cancelling one of
/// them must still end the run as cancelled, not as the success the mirror
/// itself achieved.
#[test]
fn mtp_23_cancelling_during_the_content_phase_is_not_reported_as_completed() {
    run(async {
        let (downloads, conn) = fixture();
        let episode_path = downloads.path().join("episode.mp3");
        std::fs::write(&episode_path, b"rss audio").unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'rss', 'https://example.test/rss', 'RSS Show', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'New Episode', 'https://example.test/rss.mp3',
                         ?1, 9, 1)",
                [episode_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        save_settings(&conn, &podcast_settings("a")).unwrap();

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 5));
        let (started, releases) = backend.gate_copies(&["a"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        recv_soon(&started).await;
        runtime.cancel_current("a");
        releases["a"].send(()).await.unwrap();
        settle().await;

        let device = runtime.devices().remove(0);
        assert!(
            device.last_sync.is_none(),
            "cancelling mid content-phase must not be recorded as a verified, completed \
             sync — reconnect resumability depends on this staying unset"
        );
        assert_eq!(
            device.history[0].0.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Cancelled,
            "the run log must close this run as cancelled, not as the mirror's original \
             completed outcome, or the remaining episode is silently forgotten"
        );
        assert!(
            backend.state.managed_copies.borrow().is_empty(),
            "the copy was cancelled before it finished, so nothing should have landed"
        );
        assert!(
            device.page.changes.transfer_bytes > 0,
            "the episode this run never got to copy must still show as pending work"
        );
    });
}
