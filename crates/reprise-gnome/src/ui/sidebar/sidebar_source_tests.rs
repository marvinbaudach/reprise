use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_1_podcast_and_radio_rows_are_gated_ordered_and_live_counted() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();

    rebuild(&shared, None, "source defaults");
    assert!(find_row(&shared, &ViewSource::Podcasts).is_none());
    assert!(find_row(&shared, &ViewSource::Youtube).is_none());
    assert!(find_row(&shared, &ViewSource::Radio).is_some());
    {
        let conn = shared.conn.borrow();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
            .unwrap();
        conn.execute(
            "INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at)
             VALUES ('rss', 'https://example.test/feed', 'Show', 0, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO podcast_episodes
               (subscription_id, guid, title, audio_url, position_ms, first_seen_at)
             VALUES (1, 'episode', 'Episode', 'https://example.test/episode.mp3', 0, 1)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at)
             VALUES ('youtube', 'https://youtube.test/@channel', 'Channel', 0, 1);
             INSERT INTO podcast_episodes
               (subscription_id, guid, title, audio_url, position_ms, first_seen_at)
             VALUES (2, 'video', 'Video', 'https://youtube.test/watch?v=video', 0, 1);",
        )
        .unwrap();
        conn.execute(
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
        Some("1".to_string())
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
