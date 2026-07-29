//! `MTP-33`, finding 2: `DeviceSettings::remove_deleted` ("Remove from
//! phone when deleted or unsubscribed here", design 7a) must actually gate
//! whether an unsubscribed podcast episode or YouTube track leaves the
//! device. Before this fix `device_sync_compact.rs::target_podcast_plan`
//! called `build_podcast_plan` with a hard-coded `true`, so the switch
//! rendered and persisted but no production planning path ever read it —
//! the third persisted-but-never-read control found on this branch.

use super::*;

fn podcast_settings(device_id: &str, remove_deleted: bool) -> DeviceSettings {
    DeviceSettings {
        device_serial: device_id.into(),
        device_name: format!("Phone {device_id}"),
        selection: DeviceSelection::Sources(Vec::new()),
        profile: reprise_core::device_sync::TransferProfile::default(),
        opus_bitrate: 0,
        ratings_back: false,
        remove_deleted,
        sync_automatically: false,
        prepare_before_sync: true,
    }
}

#[test]
fn mtp_33_remove_deleted_off_keeps_an_unsubscribed_episode_on_the_device() {
    run(async {
        let (downloads, conn) = fixture();
        let rss_path = downloads.path().join("rss.mp3");
        std::fs::write(&rss_path, b"rss audio").unwrap();
        conn.borrow()
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'rss', 'https://example.test/rss', 'RSS Show', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'Episode', 'https://example.test/rss.mp3',
                         ?1, 9, 1)",
                [rss_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        save_settings(&conn.borrow(), &podcast_settings("a", false)).unwrap();

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        // Already on the device from an earlier sync, but its show is no
        // longer subscribed — a `remove_deleted = true` plan would remove
        // it (see `pod_12_planned_sync_copies_selected_rss_and_youtube_
        // each_to_its_own_target` in `device_sync_planned_tests.rs`, which
        // proves exactly that for the switch's default-on state).
        backend.state.podcast_files.replace(vec![ManagedDeviceFile {
            relative_path: "Old Show/99-Old.mp3".into(),
            size_bytes: 4,
        }]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(
            backend.state.managed_deleted.borrow().is_empty(),
            "remove_deleted = false must keep the unsubscribed episode on the phone, got {:?}",
            backend.state.managed_deleted.borrow()
        );
    });
}
