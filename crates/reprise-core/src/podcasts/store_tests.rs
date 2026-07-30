//! Tests for `podcasts::store`, split out to keep the main module under the 800-line file-size gate.

use super::*;

fn conn() -> Db {
    Db::open_in_memory().unwrap()
}

fn subscription_draft() -> NewSubscription {
    NewSubscription {
        kind: PodcastKind::Rss,
        feed_url: "https://example.test/feed.xml".to_owned(),
        title: "Original Show".to_owned(),
        author: Some("Ada".to_owned()),
        image_url: None,
        auto_download: false,
    }
}

fn parsed_episode(title: &str) -> ParsedEpisode {
    ParsedEpisode {
        guid: "stable-guid".to_owned(),
        title: title.to_owned(),
        image_url: None,
        audio_url: "https://example.test/episode.mp3".to_owned(),
        page_url: None,
        published_at: Some(100),
        duration_secs: None,
    }
}

#[test]
fn pod_2_episode_upsert_changes_metadata_but_preserves_listening_state() {
    let conn = conn();
    let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    let first = upsert_episode(&conn, subscription_id, &parsed_episode("Old"), 20)
        .unwrap()
        .expect("episode should be imported");
    save_position(&conn, first.episode_id, 8_000).unwrap();
    conn.conn()
        .execute(
            "UPDATE podcast_episodes SET played_at = 30 WHERE id = ?1",
            [first.episode_id],
        )
        .unwrap();

    let second = upsert_episode(&conn, subscription_id, &parsed_episode("Renamed"), 99)
        .unwrap()
        .expect("episode should be updated");
    let row = episode(&conn, first.episode_id).unwrap().unwrap();

    assert!(first.inserted);
    assert!(!second.inserted);
    assert_eq!(second.episode_id, first.episode_id);
    assert_eq!(row.title, "Renamed");
    assert_eq!(row.first_seen_at, 20);
    assert_eq!(row.position_ms, 8_000);
    assert_eq!(row.played_at, Some(30));
}

#[test]
fn episode_upsert_backfills_publication_date_and_artwork() {
    let conn = conn();
    let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    let mut initial = parsed_episode("Episode");
    initial.published_at = None;
    let result = upsert_episode(&conn, subscription_id, &initial, 20)
        .unwrap()
        .expect("episode should be imported");

    let mut enriched = initial;
    enriched.published_at = Some(1_785_369_600);
    enriched.image_url = Some("https://img.test/episode.jpg".to_owned());
    let updated = upsert_episode(&conn, subscription_id, &enriched, 30)
        .unwrap()
        .expect("episode should be updated");
    let row = episode(&conn, result.episode_id).unwrap().unwrap();

    assert!(!updated.inserted);
    assert_eq!(row.published_at, Some(1_785_369_600));
    assert_eq!(
        row.image_url.as_deref(),
        Some("https://img.test/episode.jpg")
    );
    assert_eq!(row.first_seen_at, 20);
}

#[test]
fn future_only_baseline_replaces_and_clears_atomically() {
    let conn = conn();
    let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();

    replace_future_only_baseline(
        &conn,
        subscription_id,
        &["old-a".to_owned(), "old-b".to_owned()],
    )
    .unwrap();
    assert_eq!(
        future_only_baseline(&conn, subscription_id).unwrap(),
        ["old-a".to_owned(), "old-b".to_owned()]
    );

    replace_future_only_baseline(&conn, subscription_id, &["new".to_owned()]).unwrap();
    assert_eq!(
        future_only_baseline(&conn, subscription_id).unwrap(),
        ["new".to_owned()]
    );

    clear_future_only_baseline(&conn, subscription_id).unwrap();
    assert!(future_only_baseline(&conn, subscription_id)
        .unwrap()
        .is_empty());
}

#[test]
fn subscription_tombstone_cycle_updates_counts_and_can_commit() {
    let conn = conn();
    let id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    upsert_episode(&conn, id, &parsed_episode("Episode"), 20).unwrap();
    assert_eq!(count_subscriptions(&conn).unwrap(), 1);

    tombstone_subscription(&conn, id, 30).unwrap();
    assert_eq!(count_subscriptions(&conn).unwrap(), 0);
    assert!(active_subscriptions(&conn).unwrap().is_empty());

    undo_remove_subscription(&conn, id).unwrap();
    assert_eq!(count_subscriptions(&conn).unwrap(), 1);

    tombstone_subscription(&conn, id, 40).unwrap();
    commit_remove_subscription(&conn, id).unwrap();
    assert!(subscription(&conn, id).unwrap().is_none());
    let count: i64 = conn
        .conn()
        .query_row("SELECT COUNT(*) FROM podcast_episodes", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn resubscribe_revives_existing_identity_and_history() {
    let conn = conn();
    let id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    let episode = upsert_episode(&conn, id, &parsed_episode("Episode"), 20)
        .unwrap()
        .expect("episode should be imported");
    save_position(&conn, episode.episode_id, 12_000).unwrap();
    tombstone_subscription(&conn, id, 30).unwrap();

    let revived = add_or_restore(
        &conn,
        &NewSubscription {
            title: "Renamed Show".to_owned(),
            ..subscription_draft()
        },
        40,
    )
    .unwrap();

    assert_eq!(revived, id);
    assert_eq!(subscription(&conn, id).unwrap().unwrap().added_at, 10);
    assert_eq!(
        super::episode(&conn, episode.episode_id)
            .unwrap()
            .unwrap()
            .position_ms,
        12_000
    );
}

#[test]
fn pod_12_restoring_a_source_at_the_same_kind_preserves_phone_sync() {
    let conn = conn();
    let id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    super::super::phone_sync::set_enabled(&conn, id, true).unwrap();
    super::super::phone_sync::set_device_enabled(&conn, id, "mtp:pixel", true).unwrap();

    let restored = add_or_restore(
        &conn,
        &NewSubscription {
            title: "Renamed Show".to_owned(),
            ..subscription_draft()
        },
        20,
    )
    .unwrap();

    assert_eq!(restored, id);
    assert!(subscription(&conn, id).unwrap().unwrap().sync_to_phone);
    assert_eq!(
        super::super::phone_sync::selected_device_ids(&conn, id).unwrap(),
        ["mtp:pixel".to_owned()]
    );
}

#[test]
fn episode_finish_marks_played_and_clears_resume_position() {
    let conn = conn();
    let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    let result = upsert_episode(&conn, subscription_id, &parsed_episode("Episode"), 20)
        .unwrap()
        .expect("episode should be imported");
    save_position(&conn, result.episode_id, 9_000).unwrap();

    mark_played(&conn, result.episode_id, 30).unwrap();

    let row = episode(&conn, result.episode_id).unwrap().unwrap();
    assert_eq!(row.played_at, Some(30));
    assert_eq!(row.position_ms, 0);
}

#[test]
fn pod_7_download_metadata_persists_and_clears_path_with_size() {
    let conn = conn();
    let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
    let episode = upsert_episode(&conn, subscription_id, &parsed_episode("Episode"), 20)
        .unwrap()
        .expect("episode should be imported");

    set_downloaded_file(
        &conn,
        episode.episode_id,
        Some("/downloads/episode.mp3"),
        Some(41_943_040),
    )
    .unwrap();
    let downloaded = super::episode(&conn, episode.episode_id).unwrap().unwrap();
    assert_eq!(
        downloaded.downloaded_path.as_deref(),
        Some("/downloads/episode.mp3")
    );
    assert_eq!(downloaded.downloaded_bytes, Some(41_943_040));

    set_downloaded_file(&conn, episode.episode_id, None, None).unwrap();
    let cleared = super::episode(&conn, episode.episode_id).unwrap().unwrap();
    assert_eq!(cleared.downloaded_path, None);
    assert_eq!(cleared.downloaded_bytes, None);
}

#[test]
fn pod_6_episode_removal_undo_and_commit_block_rss_and_youtube_reimport() {
    for kind in [PodcastKind::Rss, PodcastKind::Youtube] {
        let conn = conn();
        let subscription_id = add_or_restore(
            &conn,
            &NewSubscription {
                kind,
                ..subscription_draft()
            },
            10,
        )
        .unwrap();
        let episode = upsert_episode(&conn, subscription_id, &parsed_episode("Episode"), 20)
            .unwrap()
            .expect("episode should be imported");
        set_downloaded_path(&conn, episode.episode_id, Some("/kept/download.mp3")).unwrap();

        assert!(tombstone_episode(&conn, episode.episode_id, 30).unwrap());
        assert!(super::episode(&conn, episode.episode_id).unwrap().is_none());
        assert!(super::super::query::list_episodes(&conn)
            .unwrap()
            .is_empty());

        assert!(undo_remove_episode(&conn, episode.episode_id).unwrap());
        assert!(super::episode(&conn, episode.episode_id).unwrap().is_some());

        assert!(tombstone_episode(&conn, episode.episode_id, 40).unwrap());
        let retained_download = commit_remove_episode(&conn, episode.episode_id).unwrap();
        assert_eq!(retained_download.as_deref(), Some("/kept/download.mp3"));
        assert!(super::episode(&conn, episode.episode_id).unwrap().is_none());
        assert_eq!(
            conn.conn()
                .query_row("SELECT COUNT(*) FROM podcast_episodes", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );

        let reimport =
            upsert_episode(&conn, subscription_id, &parsed_episode("Reimported"), 50).unwrap();
        assert!(reimport.is_none());
        assert!(super::super::query::list_episodes(&conn)
            .unwrap()
            .is_empty());
        assert_eq!(
            conn.conn()
                .query_row(
                    "SELECT COUNT(*) FROM podcast_episode_dismissals",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
}

#[test]
fn mtp_36_a_fresh_channel_has_no_override_and_setting_persists_it_including_zero() {
    let conn = conn();
    let subscription_id = add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            ..subscription_draft()
        },
        10,
    )
    .unwrap();

    assert_eq!(
        subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .latest_per_channel,
        None,
        "a channel nobody has touched has no override"
    );
    assert!(latest_per_channel_overrides(&conn, &[subscription_id])
        .unwrap()
        .is_empty());

    assert!(set_latest_per_channel(&conn, subscription_id, Some(2)).unwrap());
    assert_eq!(
        subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .latest_per_channel,
        Some(2)
    );
    assert_eq!(
        latest_per_channel_overrides(&conn, &[subscription_id]).unwrap(),
        std::collections::HashMap::from([(subscription_id, 2)])
    );

    // 0 must persist as 0 (unlimited), never fall back to "no override".
    assert!(set_latest_per_channel(&conn, subscription_id, Some(0)).unwrap());
    assert_eq!(
        latest_per_channel_overrides(&conn, &[subscription_id]).unwrap(),
        std::collections::HashMap::from([(subscription_id, 0)]),
        "an explicit 0 override must still be reported, not treated as absent"
    );

    assert!(set_latest_per_channel(&conn, subscription_id, None).unwrap());
    assert!(
        latest_per_channel_overrides(&conn, &[subscription_id])
            .unwrap()
            .is_empty(),
        "clearing the override removes it from the map again"
    );
}
