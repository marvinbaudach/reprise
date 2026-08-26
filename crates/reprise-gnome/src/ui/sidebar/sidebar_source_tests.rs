use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_1a_podcast_and_radio_rows_are_gated_ordered_and_live_counted() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();

    rebuild(&shared, None, "source defaults");
    assert!(find_row(&shared, &ViewSource::Podcasts).is_none());
    assert!(find_row(&shared, &ViewSource::Youtube).is_none());
    assert!(find_row(&shared, &ViewSource::Radio).is_some());
    {
        let conn = &shared.conn;
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, true)
            .unwrap();
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::YOUTUBE_MODULE, true)
            .unwrap();
        crate::test_db::connection(conn)
            .execute(
                "INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at)
             VALUES ('rss', 'https://example.test/feed', 'Show', 0, 1)",
                [],
            )
            .unwrap();
        // SRC-1a: the fixture makes the two readings disagree. The show's one
        // episode is played and the second show has none, so an unplayed count
        // would badge nothing where two subscriptions stand; the channel
        // carries two unplayed videos and a second, unsubscribed channel, so
        // there the episode count would badge two where one channel is
        // followed.
        crate::test_db::connection(conn)
            .execute(
                "INSERT INTO podcast_episodes
               (subscription_id, guid, title, audio_url, position_ms, first_seen_at, played_at)
             VALUES (1, 'episode', 'Episode', 'https://example.test/episode.mp3', 0, 1, 2)",
                [],
            )
            .unwrap();
        crate::test_db::connection(conn)
            .execute(
                "INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at)
             VALUES ('rss', 'https://example.test/second', 'Second Show', 0, 1)",
                [],
            )
            .unwrap();
        crate::test_db::connection(conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at)
             VALUES ('youtube', 'https://youtube.test/@channel', 'Channel', 0, 1);
             INSERT INTO podcast_episodes
               (subscription_id, guid, title, audio_url, position_ms, first_seen_at)
             VALUES (3, 'video', 'Video', 'https://youtube.test/watch?v=video', 0, 1),
                    (3, 'video2', 'Video 2', 'https://youtube.test/watch?v=video2', 0, 1);
             INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at, removed_at)
             VALUES ('youtube', 'https://youtube.test/@gone', 'Gone', 0, 1, 2);",
            )
            .unwrap();
        crate::test_db::connection(conn)
            .execute(
                "INSERT INTO radio_stations (name, stream_url, added_at)
             VALUES ('Station', 'https://example.test/live', 1)",
                [],
            )
            .unwrap();
    }
    rebuild(&shared, None, "source data changed");
    let rows = shared.rows.borrow();
    let music = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Library))
        .unwrap();
    let podcasts = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Podcasts))
        .unwrap();
    let youtube = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Youtube))
        .unwrap();
    let radio = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Radio))
        .unwrap();
    let queue = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Queue))
        .unwrap();
    assert!(music < podcasts && podcasts < youtube && youtube < radio && radio < queue);
    assert_eq!(
        numeric_badge_text(rows[podcasts].0.upcast_ref()),
        Some("2".to_string())
    );
    assert_eq!(
        numeric_badge_text(rows[youtube].0.upcast_ref()),
        Some("1".to_string())
    );
    assert_eq!(
        numeric_badge_text(rows[radio].0.upcast_ref()),
        Some("1".to_string())
    );
}

/// Issue #96: Podcasts off + YouTube on must be a valid, independently
/// visible state — YouTube is a peer source, not a Podcasts sub-setting.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn issue_96_podcasts_off_youtube_on_shows_only_the_youtube_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();
    {
        let conn = &shared.conn;
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::YOUTUBE_MODULE, true)
            .unwrap();
        assert!(
            !reprise_core::modules::is_enabled(conn, &reprise_core::modules::PODCASTS_MODULE)
                .unwrap()
        );
    }

    rebuild(&shared, None, "youtube only");

    assert!(find_row(&shared, &ViewSource::Podcasts).is_none());
    assert!(find_row(&shared, &ViewSource::Youtube).is_some());
}

/// `NET-1a`: the global online-sources gate hides all three source rows,
/// even when their own modules are individually on — "off really means
/// off" for the sidebar, not only for the network calls behind it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_1a_global_gate_off_hides_podcasts_youtube_and_radio_rows() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();
    {
        let conn = &shared.conn;
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, true)
            .unwrap();
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::YOUTUBE_MODULE, true)
            .unwrap();
        reprise_core::online_sources::set_enabled(conn, false).unwrap();
    }

    rebuild(&shared, None, "global gate off");

    assert!(find_row(&shared, &ViewSource::Podcasts).is_none());
    assert!(find_row(&shared, &ViewSource::Youtube).is_none());
    assert!(find_row(&shared, &ViewSource::Radio).is_none());
}
