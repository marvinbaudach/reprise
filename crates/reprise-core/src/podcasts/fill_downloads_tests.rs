use std::path::Path;

use super::super::download_state::DownloadState;
use super::super::fill_downloads::{
    fill_downloads, missing_episode_ids_in, record_fill_outcome, FillSummary,
};
use super::tests::{add_subscription, conn, feed_response, FakeFeed, FakeYoutube};
use super::*;

/// One show with `count` episodes, newest first. Returns their ids in that
/// order.
fn show_with_episodes(db: &Db, root: &Path, count: usize, auto_download: bool) -> (i64, Vec<i64>) {
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", auto_download);
    let feed = FakeFeed {
        responses: std::cell::RefCell::new(vec![Ok(feed_response("Show", count, None))]),
        ..FakeFeed::default()
    };
    refresh_to_root(db, &feed, &FakeYoutube, 10, RefreshRequest::force(), root).unwrap();
    let ids = super::super::query::episodes_for_subscription(db, subscription_id)
        .unwrap()
        .into_iter()
        .map(|episode| episode.id)
        .collect();
    (subscription_id, ids)
}

fn mark_played(connection: &rusqlite::Connection, episode_id: i64) {
    connection
        .execute(
            "UPDATE podcast_episodes SET played_at = 1 WHERE id = ?1",
            [episode_id],
        )
        .unwrap();
}

#[test]
fn the_fill_up_takes_the_newest_missing_episodes() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (_, ids) = show_with_episodes(&db, root.path(), 20, true);
    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 10, "exactly the newest ten are missing");
    assert!(missing.contains(&ids[0]), "the newest is among them");
    assert!(!missing.contains(&ids[10]), "the eleventh is not");
}

#[test]
fn the_fill_up_ignores_episodes_that_are_already_downloaded() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (_, ids) = show_with_episodes(&db, root.path(), 20, true);
    download_episode(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        ids[0],
        &mut |_| {},
    )
    .unwrap();

    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 9);
    assert!(!missing.contains(&ids[0]));
}

#[test]
fn the_fill_up_skips_played_episodes_instead_of_sliding_past_them() {
    // Sliding would pull the eleventh episode into the download set, which the
    // cleanup ranks outside the newest ten — the two would then fight forever.
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (_, ids) = show_with_episodes(&db, root.path(), 20, true);
    mark_played(db.conn(), ids[0]);
    mark_played(db.conn(), ids[1]);

    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 8);
    assert!(!missing.contains(&ids[10]), "the window does not slide");
}

#[test]
fn a_keep_of_zero_means_unlimited() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(&db, root.path(), 20, true);
    let missing = missing_episode_ids_in(db.conn(), 0).unwrap();
    assert_eq!(missing.len(), 20);
}

#[test]
fn the_fill_up_downloads_every_missing_episode_and_reports_each() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(&db, root.path(), 12, true);
    let mut seen: Vec<(i64, DownloadState)> = Vec::new();

    let summary = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut |episode_id, state| seen.push((episode_id, state)),
    )
    .unwrap();

    assert_eq!(summary.downloaded, 10);
    assert_eq!(summary.failed, 0);
    assert!(seen
        .iter()
        .any(|(_, state)| matches!(state, DownloadState::Downloaded { .. })));
}

#[test]
fn a_second_fill_up_run_downloads_nothing() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(&db, root.path(), 12, true);
    let mut ignore = |_: i64, _: DownloadState| {};

    let first = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut ignore,
    )
    .unwrap();
    let second = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut ignore,
    )
    .unwrap();

    assert_eq!(first.downloaded, 10);
    assert_eq!(second, FillSummary::default());
}

#[test]
fn the_fill_up_and_the_cleanup_agree_on_the_newest_ten() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (subscription_id, ids) = show_with_episodes(&db, root.path(), 12, true);
    let mut ignore = |_: i64, _: DownloadState| {};

    fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut ignore,
    )
    .unwrap();
    let downloaded = super::super::query::episodes_for_subscription(&db, subscription_id)
        .unwrap()
        .into_iter()
        .filter(|episode| episode.downloaded_path.is_some())
        .map(|episode| episode.id)
        .collect::<Vec<_>>();
    assert_eq!(downloaded, ids[..10]);

    let first_cleanup = super::super::downloads::enforce_cleanup(
        &db,
        root.path(),
        super::super::config::CleanupPolicy::KeepLast5,
        10,
        0,
    )
    .unwrap();
    assert_eq!(
        first_cleanup,
        super::super::downloads::CleanupSummary::default()
    );

    let second_fill = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut ignore,
    )
    .unwrap();
    let second_cleanup = super::super::downloads::enforce_cleanup(
        &db,
        root.path(),
        super::super::config::CleanupPolicy::KeepLast5,
        10,
        0,
    )
    .unwrap();
    assert_eq!(second_fill, FillSummary::default());
    assert_eq!(
        second_cleanup,
        super::super::downloads::CleanupSummary::default()
    );
}

#[test]
fn a_tombstoned_download_does_not_displace_a_live_episode_during_cleanup() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (_, ids) = show_with_episodes(&db, root.path(), 11, true);

    download_episode(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        ids[0],
        &mut |_| {},
    )
    .unwrap();
    assert!(super::super::store::tombstone_episode(&db, ids[0], 2).unwrap());

    let mut ignore = |_: i64, _: DownloadState| {};
    let first_fill = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut ignore,
    )
    .unwrap();
    assert_eq!(first_fill.downloaded, 10);

    let cleanup = super::super::downloads::enforce_cleanup(
        &db,
        root.path(),
        super::super::config::CleanupPolicy::KeepLast5,
        10,
        0,
    )
    .unwrap();
    assert_eq!(
        cleanup,
        super::super::downloads::CleanupSummary::default(),
        "a tombstoned download must not consume a live episode's rank"
    );

    let second_fill = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut ignore,
    )
    .unwrap();
    assert_eq!(second_fill, FillSummary::default());
}

#[test]
fn a_mid_batch_error_is_counted_and_later_episodes_are_still_downloaded() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (_, ids) = show_with_episodes(&db, root.path(), 3, true);
    let removed = std::cell::Cell::new(false);

    let summary = fill_downloads(
        &db,
        &FakeFeed::default(),
        &FakeYoutube,
        root.path(),
        &mut |episode_id, state| {
            if episode_id == ids[0]
                && matches!(state, DownloadState::Downloaded { .. })
                && !removed.replace(true)
            {
                assert!(super::super::store::tombstone_episode(&db, ids[1], 2).unwrap());
            }
        },
    )
    .expect("one vanished episode must not abort the fill");

    assert_eq!(summary.downloaded, 2);
    assert_eq!(summary.failed, 1);
    assert!(
        super::super::store::episode(&db, ids[2])
            .unwrap()
            .unwrap()
            .downloaded_path
            .is_some(),
        "the episode after the failed one must still be downloaded"
    );
}

#[test]
fn a_non_terminal_download_result_is_warned_instead_of_silently_dropped() {
    let logs = crate::log_capture::CapturedLogs::default();
    let mut summary = FillSummary::default();

    logs.capture(|| {
        record_fill_outcome(&mut summary, 42, Ok(DownloadState::Queued));
    });

    assert_eq!(summary, FillSummary::default());
    let logged = logs.joined();
    assert!(
        logged.contains("podcast fill received a non-terminal download state"),
        "missing non-terminal warning: {logged}"
    );
    assert!(
        logged.contains("42"),
        "warning dropped episode id: {logged}"
    );
}

#[test]
fn the_fill_up_respects_each_subscriptions_auto_download_switch() {
    let disabled_db = conn();
    let disabled_root = tempfile::tempdir().unwrap();
    show_with_episodes(&disabled_db, disabled_root.path(), 3, false);
    let mut ignore = |_: i64, _: DownloadState| {};

    let disabled = fill_downloads(
        &disabled_db,
        &FakeFeed::default(),
        &FakeYoutube,
        disabled_root.path(),
        &mut ignore,
    )
    .unwrap();

    assert_eq!(disabled, FillSummary::default());

    let enabled_db = conn();
    let enabled_root = tempfile::tempdir().unwrap();
    show_with_episodes(&enabled_db, enabled_root.path(), 3, true);
    let enabled = fill_downloads(
        &enabled_db,
        &FakeFeed::default(),
        &FakeYoutube,
        enabled_root.path(),
        &mut ignore,
    )
    .unwrap();

    assert_eq!(enabled.downloaded, 3);
    assert_eq!(enabled.failed, 0);
}
