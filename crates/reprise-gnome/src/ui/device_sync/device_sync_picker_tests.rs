use super::*;

#[test]
fn picker_snapshot_reads_the_same_group_and_episode_flags_as_live_sync() {
    run(async {
        let (_downloads, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1);
                 INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, published_at,
                  downloaded_path, downloaded_bytes, wanted_on_device, first_seen_at)
                 VALUES (1, 10, 'old', 'Old pinned', 'https://example.test/old', 100,
                         '/tmp/old.webm', 1000, 1, 1),
                        (2, 10, 'new', 'New automatic', 'https://example.test/new', 200,
                         '/tmp/new.webm', 2000, 0, 1);",
            )
            .unwrap();
        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::YOUTUBE_MODULE, true)
            .unwrap();
        reprise_core::podcasts::phone_sync::set_device_enabled(&conn, 10, "a", true).unwrap();
        reprise_core::library::settings::set_setting(
            &conn,
            reprise_core::podcasts::config::LATEST_PER_CHANNEL_DEFAULT_KEY,
            "1",
        )
        .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;

        let snapshot = runtime
            .picker_snapshot("a", reprise_core::device_sync::SyncTargetKind::YoutubeAudio)
            .unwrap();
        let PickerSnapshot::Episodes { groups, .. } = snapshot else {
            panic!("YouTube must use the grouped episode picker");
        };

        assert_eq!(groups.len(), 1);
        assert!(groups[0].enabled);
        assert_eq!(
            groups[0]
                .episodes
                .iter()
                .map(|episode| (episode.id, episode.selected, episode.pinned))
                .collect::<Vec<_>>(),
            [(2, true, false), (1, true, true)],
            "the latest rule and the persistent explicit flag feed one projection"
        );
    });
}

#[test]
fn picker_save_writes_the_existing_selection_flags_without_a_picker_store() {
    run(async {
        let (_downloads, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1);
                 INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, wanted_on_device, first_seen_at)
                 VALUES (1, 10, 'episode', 'Episode', 'https://example.test/episode', 0, 1);",
            )
            .unwrap();
        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::YOUTUBE_MODULE, true)
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;
        runtime
            .save_picker(
                "a",
                PickerSave {
                    playlist_changes: vec![(reprise_core::device_sync::EVERYTHING_SOURCE, true)],
                    group_changes: vec![(10, true)],
                    episode_pin_changes: vec![(1, true)],
                    latest_per_channel: Some(7),
                    keep_smart_updated: Some(false),
                },
            )
            .unwrap();

        let device = runtime.devices().remove(0);
        assert_eq!(
            device.settings.selection,
            reprise_core::device_sync::DeviceSelection::EntireLibrary
        );
        assert_eq!(
            reprise_core::podcasts::phone_sync::selected_device_ids(&conn, 10).unwrap(),
            ["a"]
        );
        assert_eq!(
            reprise_core::podcasts::wanted_on_device::wanted_on_device(&conn, 1).unwrap(),
            Some(true)
        );
        assert_eq!(
            reprise_core::podcasts::config::load(&conn)
                .unwrap()
                .latest_per_channel_default,
            7
        );
        assert!(
            !reprise_core::library::settings::get_bool(&conn, KEEP_SMART_UPDATED_KEY, true)
                .unwrap()
        );
    });
}

#[test]
fn smart_playlist_toggle_freezes_and_then_resumes_live_evaluation() {
    run(async {
        let (_downloads, conn) = fixture();
        let smart_id = reprise_core::library::playlists::create_smart(
            &conn,
            "All tracks",
            "[]",
            "title",
            "asc",
            None,
        )
        .unwrap();
        let source = SelectionSource::Smart(smart_id);
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;
        runtime
            .set_playlist_selected("a", source.clone(), true)
            .unwrap();
        runtime
            .save_picker(
                "a",
                PickerSave {
                    keep_smart_updated: Some(false),
                    ..Default::default()
                },
            )
            .unwrap();

        crate::test_db::connection(&conn)
            .execute(
                "UPDATE smart_playlists SET limit_count = 1 WHERE id = ?1",
                [smart_id],
            )
            .unwrap();
        runtime.recompute_delta("a").unwrap();
        assert_eq!(
            runtime.devices().remove(0).page.unique_track_count,
            4,
            "switching updates off keeps the originally evaluated smart-playlist copy"
        );

        reprise_core::library::settings::set_bool(&conn, KEEP_SMART_UPDATED_KEY, true).unwrap();
        runtime.recompute_delta("a").unwrap();
        assert_eq!(
            runtime.devices().remove(0).page.unique_track_count,
            1,
            "switching updates back on resumes the smart playlist's live evaluation"
        );
    });
}
