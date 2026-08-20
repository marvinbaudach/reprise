use std::path::Path;

use super::super::download_state::DownloadState;
use super::super::fill_downloads::{fill_downloads, missing_episode_ids_in, FillSummary};
use super::tests::{add_subscription, conn, feed_response, FakeFeed, FakeYoutube};
use super::*;

/// One show with `count` episodes, newest first. Returns their ids in that
/// order.
fn show_with_episodes(db: &Db, root: &Path, count: usize) -> (i64, Vec<i64>) {
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
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
    let (_, ids) = show_with_episodes(&db, root.path(), 20);
    let missing = missing_episode_ids_in(db.conn(), 10).unwrap();
    assert_eq!(missing.len(), 10, "exactly the newest ten are missing");
    assert!(missing.contains(&ids[0]), "the newest is among them");
    assert!(!missing.contains(&ids[10]), "the eleventh is not");
}

#[test]
fn the_fill_up_ignores_episodes_that_are_already_downloaded() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let (_, ids) = show_with_episodes(&db, root.path(), 20);
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
    let (_, ids) = show_with_episodes(&db, root.path(), 20);
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
    show_with_episodes(&db, root.path(), 20);
    let missing = missing_episode_ids_in(db.conn(), 0).unwrap();
    assert_eq!(missing.len(), 20);
}

#[test]
fn the_fill_up_downloads_every_missing_episode_and_reports_each() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    show_with_episodes(&db, root.path(), 12);
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
    show_with_episodes(&db, root.path(), 12);
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
    let (subscription_id, ids) = show_with_episodes(&db, root.path(), 12);
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
