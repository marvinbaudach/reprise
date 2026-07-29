//! `MTP-41` live wiring: before this fix `query_candidates_for_device`
//! selected every downloaded episode with no played filter at all, and
//! `device_sync_runtime.rs` hard-coded `files_waiting_for_download` to `0`
//! (`selection::select_episodes` was never called from the live pipeline).
//! These tests prove the fix end to end, through the same `sync_now`/
//! `category_readings` surface the rest of this test suite already uses —
//! not just the pure `selection::select_episodes` unit tests, which already
//! passed before the live wiring existed and therefore could not have
//! caught this.

use super::*;

fn insert_enabled_rss_show(conn: &Rc<RefCell<Connection>>, subscription_id: i64, device_id: &str) {
    conn.borrow()
        .execute(
            "INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
             VALUES (?1, 'rss', ?2, 'RSS Show', 0, 1, 1)",
            rusqlite::params![
                subscription_id,
                format!("https://example.test/{subscription_id}")
            ],
        )
        .unwrap();
    conn.borrow()
        .execute(
            "INSERT INTO podcast_subscription_devices (subscription_id, device_id)
             VALUES (?1, ?2)",
            rusqlite::params![subscription_id, device_id],
        )
        .unwrap();
}

#[test]
fn mtp_41_a_played_downloaded_episode_is_not_copied_while_an_unplayed_one_from_the_same_show_is() {
    run(async {
        let (downloads, conn) = fixture();
        let unplayed_path = downloads.path().join("unplayed.mp3");
        let played_path = downloads.path().join("played.mp3");
        std::fs::write(&unplayed_path, b"unplayed-audio").unwrap();
        std::fs::write(&played_path, b"played-audio").unwrap();
        insert_enabled_rss_show(&conn, 10, "a");
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'Unplayed', 'https://example.test/u.mp3', ?1, 1)",
                [unplayed_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path, played_at,
                  first_seen_at)
                 VALUES (101, 10, 'rss-101', 'Played', 'https://example.test/p.mp3', ?1, 1, 1)",
                [played_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(
            backend.state.managed_copies.borrow().as_slice(),
            [(
                "/Podcasts/Reprise".to_string(),
                "RSS Show/100-Unplayed.mp3".to_string()
            )],
            "a played, downloaded episode must never be copied even though its show is \
             enabled and it has a local file — the device page states 'Unplayed downloads \
             only'"
        );
    });
}

#[test]
fn mtp_41_a_wanted_missing_episode_counts_as_waiting_and_is_never_copyable() {
    run(async {
        let (_downloads, conn) = fixture();
        insert_enabled_rss_show(&conn, 10, "a");
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, wanted_on_device, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'Wanted', 'https://example.test/w.mp3', 1, 1)",
                [],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        // `SyncTargetKind::ALL` order is [Playlists, YoutubeAudio,
        // PodcastEpisodes] — index 2 is the podcast category (`MTP-37`).
        match device.category_readings[2] {
            reprise_core::device_sync::CategoryReading::Diff(diff) => {
                assert_eq!(
                    diff.files_waiting_for_download, 1,
                    "a wanted-but-missing episode (`wanted_on_device`, `MTP-40`) must reach \
                     the podcast category's waiting count instead of vanishing from it"
                );
                assert_eq!(
                    diff.files_to_copy, 0,
                    "it must never be treated as copyable while it has no local file"
                );
            }
            other => panic!("expected a computed podcast diff, got {other:?}"),
        }

        let balance = reprise_core::device_sync::aggregate_balance(&device.category_readings);
        assert_eq!(balance.files_to_copy, 0);
        assert_eq!(balance.files_waiting_for_download, 1);
        assert!(
            balance.has_work(),
            "a waiting episode must never let the overall balance read as nothing pending \
             (`MTP-22`) — that is exactly what let `MTP-30` report 'Up to date' before this fix"
        );
    });
}
