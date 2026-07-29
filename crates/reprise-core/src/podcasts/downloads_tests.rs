//! Tests for `podcasts::downloads`, split out to keep the main module under the 800-line file-size gate.

use super::*;
use crate::podcasts::feed::ParsedEpisode;
use crate::podcasts::store::{self, NewSubscription};
use crate::podcasts::PodcastError;
use crate::podcasts::PodcastKind;

fn conn() -> Connection {
    crate::db::open_migrated(None).unwrap()
}

const DEFAULT_FEED_URL: &str = "https://example.test/feed";

fn add_show(conn: &Connection) -> i64 {
    add_show_with_feed(conn, DEFAULT_FEED_URL)
}

fn add_show_with_feed(conn: &Connection, feed_url: &str) -> i64 {
    store::add_or_restore(
        conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: feed_url.to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap()
}

fn add_download(
    conn: &Connection,
    root: &Path,
    subscription_id: i64,
    number: i64,
    played_at: Option<i64>,
) -> i64 {
    add_download_with_feed(
        conn,
        root,
        DEFAULT_FEED_URL,
        subscription_id,
        number,
        played_at,
    )
}

fn add_download_with_feed(
    conn: &Connection,
    root: &Path,
    feed_url: &str,
    subscription_id: i64,
    number: i64,
    played_at: Option<i64>,
) -> i64 {
    let guid = format!("episode-{number}");
    let result = store::upsert_episode(
        conn,
        subscription_id,
        &ParsedEpisode {
            guid: guid.clone(),
            title: guid.clone(),
            audio_url: format!("https://example.test/{guid}.mp3"),
            page_url: None,
            published_at: Some(number),
            duration_secs: None,
        },
        number,
    )
    .unwrap()
    .expect("episode should be imported");
    let path = download_path(root, feed_url, &guid, "mp3");
    prepare_destination(&path).unwrap();
    std::fs::write(&path, [0_u8; 4]).unwrap();
    store::set_downloaded_path(conn, result.episode_id, path.to_str()).unwrap();
    if let Some(played_at) = played_at {
        store::mark_played(conn, result.episode_id, played_at).unwrap();
    }
    result.episode_id
}

#[test]
fn pod_5_paths_are_guid_keyed_and_reclaimable() {
    let directory = tempfile::tempdir().unwrap();
    let first = download_path(
        directory.path(),
        "https://example.test/feed",
        "stable-guid",
        ".mp3",
    );
    let second = download_path(
        directory.path(),
        "https://example.test/feed",
        "stable-guid",
        "mp3",
    );
    assert_eq!(first, second);
    prepare_destination(&first).unwrap();
    std::fs::write(&first, b"audio").unwrap();
    assert_eq!(
        reclaim_existing(directory.path(), "https://example.test/feed", "stable-guid").unwrap(),
        Some(first)
    );
}

#[test]
fn pod_7_downloads_publish_only_complete_files_and_clean_failed_partials() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("episode.mp3");
    let part = partial_path(&destination);

    let bytes = download_atomically(&destination, |temporary| {
        assert_eq!(temporary, part);
        std::fs::write(temporary, b"complete")
            .map_err(|error| PodcastError::Body(error.to_string()))
    })
    .unwrap();
    assert_eq!(bytes, 8);
    assert_eq!(std::fs::read(&destination).unwrap(), b"complete");
    assert!(!part.exists());

    std::fs::remove_file(&destination).unwrap();
    let result = download_atomically(&destination, |temporary| {
        std::fs::write(temporary, b"partial").unwrap();
        Err(PodcastError::Transport("offline".to_owned()))
    });
    assert!(matches!(result, Err(PodcastError::Transport(_))));
    assert!(!destination.exists());
    assert!(!part.exists());
}

#[test]
fn pod_10_youtube_downloads_have_one_shared_opus_extension_policy() {
    assert_eq!(
        super::extension_for(
            super::super::PodcastKind::Youtube,
            "https://www.youtube.com/watch?v=video"
        ),
        "opus"
    );
    assert_eq!(
        super::extension_for(
            super::super::PodcastKind::Rss,
            "https://example.test/episode.mp3"
        ),
        "mp3"
    );
}

#[test]
fn pod_7_reclaim_ignores_partial_files() {
    let directory = tempfile::tempdir().unwrap();
    let destination = download_path(
        directory.path(),
        "https://example.test/feed",
        "stable-guid",
        "mp3",
    );
    prepare_destination(&destination).unwrap();
    std::fs::write(partial_path(&destination), b"partial").unwrap();

    assert_eq!(
        reclaim_existing(directory.path(), "https://example.test/feed", "stable-guid").unwrap(),
        None
    );
}

#[test]
fn pod_7_completed_file_is_not_persisted_after_episode_removal() {
    let conn = conn();
    let show = add_show(&conn);
    let result = store::upsert_episode(
        &conn,
        show,
        &ParsedEpisode {
            guid: "race".to_owned(),
            title: "Race".to_owned(),
            audio_url: "https://example.test/race.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap();
    store::tombstone_episode(&conn, result.episode_id, 2).unwrap();

    assert!(
        !persist_completed_if_active(&conn, result.episode_id, "/podcasts/race.mp3", 128).unwrap()
    );
}

#[test]
fn keep_all_never_deletes_downloads() {
    let conn = conn();
    let directory = tempfile::tempdir().unwrap();
    let show = add_show(&conn);
    let episode = add_download(&conn, directory.path(), show, 1, Some(1));

    assert_eq!(
        enforce_cleanup(
            &conn,
            directory.path(),
            CleanupPolicy::KeepAll,
            5,
            1_000_000,
        )
        .unwrap(),
        CleanupSummary::default()
    );
    assert!(store::episode(&conn, episode)
        .unwrap()
        .unwrap()
        .downloaded_path
        .is_some());
}

#[test]
fn played_age_policy_deletes_only_old_played_downloads() {
    let conn = conn();
    let directory = tempfile::tempdir().unwrap();
    let show = add_show(&conn);
    let now = 1_000_000;
    let old = add_download(
        &conn,
        directory.path(),
        show,
        1,
        Some(now - PLAYED_RETENTION_SECONDS),
    );
    let recent = add_download(&conn, directory.path(), show, 2, Some(now - 10));
    let unplayed = add_download(&conn, directory.path(), show, 3, None);

    let summary = enforce_cleanup(
        &conn,
        directory.path(),
        CleanupPolicy::DeletePlayedAfter7Days,
        5,
        now,
    )
    .unwrap();

    assert_eq!(summary.files_deleted, 1);
    assert!(store::episode(&conn, old)
        .unwrap()
        .unwrap()
        .downloaded_path
        .is_none());
    for id in [recent, unplayed] {
        assert!(store::episode(&conn, id)
            .unwrap()
            .unwrap()
            .downloaded_path
            .is_some());
    }
}

#[test]
fn cleanup_never_deletes_a_download_path_outside_its_root() {
    let conn = conn();
    let download_root = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let show = add_show(&conn);
    let now = 1_000_000;
    let episode = add_download(
        &conn,
        download_root.path(),
        show,
        1,
        Some(now - PLAYED_RETENTION_SECONDS),
    );
    let foreign_path = foreign_directory.path().join("library-track.flac");
    std::fs::write(&foreign_path, b"must survive").unwrap();
    store::set_downloaded_path(&conn, episode, foreign_path.to_str()).unwrap();

    let result = enforce_cleanup(
        &conn,
        download_root.path(),
        CleanupPolicy::DeletePlayedAfter7Days,
        5,
        now,
    );

    assert!(result.is_err());
    assert_eq!(std::fs::read(&foreign_path).unwrap(), b"must survive");
    assert_eq!(
        store::episode(&conn, episode)
            .unwrap()
            .unwrap()
            .downloaded_path
            .as_deref(),
        foreign_path.to_str()
    );
}

#[test]
fn keep_last_five_is_applied_per_show() {
    let conn = conn();
    let directory = tempfile::tempdir().unwrap();
    let show = add_show(&conn);
    for number in 1..=7 {
        add_download(&conn, directory.path(), show, number, None);
    }

    let summary = enforce_cleanup(&conn, directory.path(), CleanupPolicy::KeepLast5, 5, 0).unwrap();

    assert_eq!(summary.files_deleted, 2);
    let remaining = super::super::query::episodes_for_subscription(&conn, show)
        .unwrap()
        .into_iter()
        .filter(|episode| episode.downloaded_path.is_some())
        .count();
    assert_eq!(remaining, 5);
}

#[test]
fn pod_5_resolve_keep_downloaded_prefers_the_channel_override_over_the_global_default() {
    assert_eq!(resolve_keep_downloaded(5, None), 5);
    assert_eq!(resolve_keep_downloaded(5, Some(2)), 2);
    // An explicit 0 override always wins, even though it means something
    // different (unlimited) than "no override" (`E-9`).
    assert_eq!(resolve_keep_downloaded(5, Some(0)), 0);
}

/// `POD-5` / `O-5`: a channel's own "keep N downloaded" value must
/// actually change what cleanup deletes, not merely round-trip through
/// storage — a global default of 3 and a channel override of 1 must
/// leave that one channel at 1 downloaded episode while every other
/// channel stays at the global default of 3.
#[test]
fn pod_5_channel_keep_downloaded_override_changes_what_cleanup_deletes() {
    let conn = conn();
    let directory = tempfile::tempdir().unwrap();
    let overridden_feed = "https://example.test/overridden";
    let default_feed = "https://example.test/default";
    let overridden_show = add_show_with_feed(&conn, overridden_feed);
    let default_show = add_show_with_feed(&conn, default_feed);
    store::set_keep_downloaded(&conn, overridden_show, Some(1)).unwrap();
    for number in 1..=4 {
        add_download_with_feed(
            &conn,
            directory.path(),
            overridden_feed,
            overridden_show,
            number,
            None,
        );
        add_download_with_feed(
            &conn,
            directory.path(),
            default_feed,
            default_show,
            number,
            None,
        );
    }

    let summary = enforce_cleanup(&conn, directory.path(), CleanupPolicy::KeepLast5, 3, 0).unwrap();

    let remaining = |subscription_id| {
        super::super::query::episodes_for_subscription(&conn, subscription_id)
            .unwrap()
            .into_iter()
            .filter(|episode| episode.downloaded_path.is_some())
            .count()
    };
    assert_eq!(
        remaining(overridden_show),
        1,
        "the channel's own override (1) must win over the global default (3)"
    );
    assert_eq!(
        remaining(default_show),
        3,
        "a channel without an override must fall back to the global default (3)"
    );
    assert_eq!(summary.files_deleted, 3 + 1);
}

/// `POD-5` / `E-9`: a resolved "keep N downloaded" of 0 must mean
/// unlimited, never "delete everything" — this is the exact `take(0)`
/// class of bug `MTP-36` had to guard against for the sync-side "latest
/// N", and getting it wrong here deletes a user's downloads outright.
#[test]
fn pod_5_keep_downloaded_zero_means_unlimited_not_delete_everything() {
    let conn = conn();
    let directory = tempfile::tempdir().unwrap();
    let show = add_show(&conn);
    for number in 1..=7 {
        add_download(&conn, directory.path(), show, number, None);
    }

    let summary = enforce_cleanup(&conn, directory.path(), CleanupPolicy::KeepLast5, 0, 0).unwrap();

    assert_eq!(
        summary.files_deleted, 0,
        "a keep-N of 0 must mean unlimited, not delete-everything"
    );
    let remaining = super::super::query::episodes_for_subscription(&conn, show)
        .unwrap()
        .into_iter()
        .filter(|episode| episode.downloaded_path.is_some())
        .count();
    assert_eq!(remaining, 7);
}

/// `POD-5` / `E-9`: same zero-means-unlimited guarantee, but via a
/// channel's own override rather than the global default — the override
/// path must not silently fall through to a different behavior.
#[test]
fn pod_5_channel_override_of_zero_means_unlimited_for_that_channel_only() {
    let conn = conn();
    let directory = tempfile::tempdir().unwrap();
    let unlimited_feed = "https://example.test/unlimited";
    let default_feed = "https://example.test/default-two";
    let unlimited_show = add_show_with_feed(&conn, unlimited_feed);
    let default_show = add_show_with_feed(&conn, default_feed);
    store::set_keep_downloaded(&conn, unlimited_show, Some(0)).unwrap();
    for number in 1..=6 {
        add_download_with_feed(
            &conn,
            directory.path(),
            unlimited_feed,
            unlimited_show,
            number,
            None,
        );
        add_download_with_feed(
            &conn,
            directory.path(),
            default_feed,
            default_show,
            number,
            None,
        );
    }

    let summary = enforce_cleanup(&conn, directory.path(), CleanupPolicy::KeepLast5, 2, 0).unwrap();

    let remaining = |subscription_id| {
        super::super::query::episodes_for_subscription(&conn, subscription_id)
            .unwrap()
            .into_iter()
            .filter(|episode| episode.downloaded_path.is_some())
            .count()
    };
    assert_eq!(
        remaining(unlimited_show),
        6,
        "an explicit per-channel override of 0 must keep everything"
    );
    assert_eq!(remaining(default_show), 2);
    assert_eq!(summary.files_deleted, 4);
}
