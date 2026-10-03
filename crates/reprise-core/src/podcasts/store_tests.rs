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
fn duration_gaps_ignore_channel_tab_guids() {
    let db = conn();
    let subscription_id = add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            ..subscription_draft()
        },
        10,
    )
    .unwrap();
    let mut video = parsed_episode("Video");
    video.guid = "abcdefghijk".to_owned();
    upsert_episode(&db, subscription_id, &video, 20).unwrap();
    let mut channel_tab = parsed_episode("Channel - Shorts");
    channel_tab.guid = "UClDzr-KM5H2-bsO3xIC32mg".to_owned();
    upsert_episode(&db, subscription_id, &channel_tab, 20).unwrap();

    assert_eq!(
        episodes_missing_duration_in(db.conn(), subscription_id).unwrap(),
        1
    );
}

#[test]
fn duration_fill_ignores_unknown_guids() {
    let db = conn();
    let subscription_id = add_or_restore(&db, &subscription_draft(), 10).unwrap();
    upsert_episode(&db, subscription_id, &parsed_episode("Episode"), 20).unwrap();

    let changed = fill_missing_durations_in(
        db.conn(),
        subscription_id,
        &[("unknown-guid".to_owned(), 225)],
    )
    .unwrap();

    assert_eq!(changed, 0);
}

#[test]
fn duration_fill_ignores_non_video_guids() {
    const CHANNEL_TAB_ID: &str = "UClDzr-KM5H2-bsO3xIC32mg";
    let db = conn();
    let subscription_id = add_or_restore(&db, &subscription_draft(), 10).unwrap();
    let mut channel_tab = parsed_episode("Channel - Shorts");
    channel_tab.guid = CHANNEL_TAB_ID.to_owned();
    let episode_id = upsert_episode(&db, subscription_id, &channel_tab, 20)
        .unwrap()
        .unwrap()
        .episode_id;

    let changed = fill_missing_durations_in(
        db.conn(),
        subscription_id,
        &[(CHANNEL_TAB_ID.to_owned(), 225)],
    )
    .unwrap();

    assert_eq!(changed, 0);
    assert_eq!(
        episode(&db, episode_id).unwrap().unwrap().duration_secs,
        None
    );
}

#[test]
fn duration_fill_ignores_zero_durations() {
    let db = conn();
    let subscription_id = add_or_restore(&db, &subscription_draft(), 10).unwrap();
    let episode_id = upsert_episode(&db, subscription_id, &parsed_episode("Episode"), 20)
        .unwrap()
        .unwrap()
        .episode_id;

    let changed =
        fill_missing_durations_in(db.conn(), subscription_id, &[("stable-guid".to_owned(), 0)])
            .unwrap();

    assert_eq!(changed, 0);
    assert_eq!(
        episode(&db, episode_id).unwrap().unwrap().duration_secs,
        None
    );
}

#[test]
fn duration_fill_is_scoped_to_one_subscription() {
    let db = conn();
    let first_subscription = add_or_restore(&db, &subscription_draft(), 10).unwrap();
    let mut second_draft = subscription_draft();
    second_draft.feed_url = "https://example.test/second.xml".to_owned();
    let second_subscription = add_or_restore(&db, &second_draft, 10).unwrap();
    let episode_id = upsert_episode(
        &db,
        second_subscription,
        &parsed_episode("Second show episode"),
        20,
    )
    .unwrap()
    .unwrap()
    .episode_id;

    let changed = fill_missing_durations_in(
        db.conn(),
        first_subscription,
        &[("stable-guid".to_owned(), 225)],
    )
    .unwrap();

    assert_eq!(changed, 0);
    assert_eq!(
        episode(&db, episode_id).unwrap().unwrap().duration_secs,
        None
    );
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
fn youtube_resolution_persists_duration_and_category_atomically() {
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
    let episode_id = upsert_episode(&conn, subscription_id, &parsed_episode("Video"), 20)
        .unwrap()
        .unwrap()
        .episode_id;

    save_youtube_resolution(&conn, episode_id, Some(93), Some("Music")).unwrap();

    let stored = episode(&conn, episode_id).unwrap().unwrap();
    assert_eq!(stored.duration_secs, Some(93));
    assert_eq!(stored.media_category.as_deref(), Some("Music"));

    save_youtube_resolution(&conn, episode_id, None, None).unwrap();
    let stored = episode(&conn, episode_id).unwrap().unwrap();
    assert_eq!(stored.duration_secs, Some(93));
    assert_eq!(stored.media_category.as_deref(), Some("Music"));

    conn.conn()
        .execute(
            "UPDATE podcast_episodes
             SET duration_secs = NULL, media_category = NULL
             WHERE id = ?1",
            [episode_id],
        )
        .unwrap();
    conn.conn()
        .execute_batch(
            "CREATE TRIGGER reject_media_category
             BEFORE UPDATE OF media_category ON podcast_episodes
             WHEN NEW.media_category = 'Music'
             BEGIN
               SELECT RAISE(ABORT, 'reject category');
             END;",
        )
        .unwrap();
    assert!(save_youtube_resolution(&conn, episode_id, Some(93), Some("Music")).is_err());
    let stored = episode(&conn, episode_id).unwrap().unwrap();
    assert_eq!(stored.duration_secs, None);
    assert_eq!(stored.media_category, None);
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
fn pod_6_removing_an_episode_waits_for_a_concurrent_writer() {
    use std::time::{Duration, Instant};

    const WRITER_HOLD: Duration = Duration::from_secs(1);

    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("library.db");
    let db = Db::open_migrated(Some(&database_path)).unwrap();
    assert_eq!(
        db.conn()
            .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        crate::db::DEFAULT_BUSY_TIMEOUT_MS
    );
    let subscription_id = add_or_restore(&db, &subscription_draft(), 10).unwrap();
    let episode_id = upsert_episode(&db, subscription_id, &parsed_episode("Episode"), 20)
        .unwrap()
        .expect("episode should be imported")
        .episode_id;
    assert!(tombstone_episode(&db, episode_id, 30).unwrap());

    let (locked, lock_observed) = std::sync::mpsc::sync_channel::<Result<Instant, String>>(1);
    let writer = std::thread::spawn(move || -> Result<(), String> {
        let writer_db = match Db::open_ready(&database_path) {
            Ok(db) => db,
            Err(error) => {
                let message = format!("could not open concurrent writer: {error}");
                let _ = locked.send(Err(message.clone()));
                return Err(message);
            }
        };
        let transaction = match rusqlite::Transaction::new_unchecked(
            writer_db.conn(),
            rusqlite::TransactionBehavior::Immediate,
        ) {
            Ok(transaction) => transaction,
            Err(error) => {
                let message = format!("could not begin concurrent write: {error}");
                let _ = locked.send(Err(message.clone()));
                return Err(message);
            }
        };
        // SQLite's writer lock is database-wide, so the table written here is irrelevant.
        if let Err(error) = transaction.execute(
            "UPDATE podcast_subscriptions SET title = ?2 WHERE id = ?1",
            params![subscription_id, "Writer held the lock"],
        ) {
            let message = format!("could not establish concurrent write: {error}");
            let _ = locked.send(Err(message.clone()));
            return Err(message);
        }
        let release_at = Instant::now() + WRITER_HOLD;
        locked
            .send(Ok(release_at))
            .map_err(|error| format!("could not report concurrent writer lock: {error}"))?;
        std::thread::sleep(WRITER_HOLD);
        transaction
            .commit()
            .map_err(|error| format!("could not commit concurrent write: {error}"))
    });
    let release_at = match lock_observed.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(release_at)) => release_at,
        Ok(Err(error)) => {
            let writer_result = writer
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            panic!("concurrent writer setup failed: {error}; writer result: {writer_result:?}");
        }
        Err(error) => {
            let writer_result = writer
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            panic!(
                "concurrent writer did not report setup before the deadline: {error}; writer result: {writer_result:?}"
            );
        }
    };

    let removal_started = Instant::now();
    assert!(
        removal_started < release_at,
        "episode removal did not start before the concurrent writer's planned release"
    );
    let removed = commit_remove_episode(&db, episode_id);
    let removal_finished = Instant::now();
    writer
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        .expect("concurrent writer should finish successfully");

    assert!(
        removal_finished >= release_at,
        "episode removal returned before the concurrent writer released: removal_finished={removal_finished:?}, release_at={release_at:?}"
    );
    assert!(
        removed.is_ok(),
        "episode removal should wait for the concurrent writer: {removed:?}"
    );
    assert!(episode(&db, episode_id).unwrap().is_none());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM podcast_episode_dismissals WHERE guid = ?1",
                ["stable-guid"],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}
