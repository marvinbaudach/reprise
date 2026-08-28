use std::cell::RefCell;
use std::sync::Arc;

use super::tests::{add_subscription, conn, feed_response, FakeFeed, FakeYoutube};
use super::*;

#[test]
fn scoped_sync_reports_the_feed_read_in_order_before_artwork_and_completion() {
    let db = conn();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 2, None))]),
        ..FakeFeed::default()
    };
    let abort = SyncAbort::new();
    let mut progress = Vec::new();

    let summary = sync_subscription(
        &db,
        &feed,
        &FakeYoutube,
        10,
        subscription_id,
        &abort,
        &mut |event| progress.push(event),
    )
    .unwrap();

    assert_eq!(summary.attempted, 1);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(summary.episodes_inserted, 2);
    assert_eq!(
        progress,
        vec![
            SyncProgress::Started,
            SyncProgress::FeedRead { episodes_found: 1 },
            SyncProgress::FeedRead { episodes_found: 2 },
            SyncProgress::FetchingArtwork,
            SyncProgress::Done(summary),
        ]
    );
}

#[test]
fn aborted_scoped_sync_commits_no_episodes_for_a_removed_subscription() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let db = crate::db::Db::open_migrated(Some(&database_path)).unwrap();
    crate::online_sources::set_enabled(&db, true).unwrap();
    crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let remover = crate::db::Db::open_migrated(Some(&database_path)).unwrap();
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 3, None))]),
        ..FakeFeed::default()
    };
    let abort = Arc::new(SyncAbort::new());
    let abort_from_callback = abort.clone();

    let result = sync_subscription(
        &db,
        &feed,
        &FakeYoutube,
        10,
        subscription_id,
        &abort,
        &mut |event| {
            if event == (SyncProgress::FeedRead { episodes_found: 1 }) {
                abort_from_callback.cancel();
                super::super::store::tombstone_subscription(&remover, subscription_id, 11).unwrap();
                super::super::store::commit_remove_subscription(&remover, subscription_id).unwrap();
            }
        },
    );

    assert!(matches!(result, Err(PipelineError::SyncAborted)));
    let stored: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM podcast_episodes WHERE subscription_id = ?1",
            [subscription_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        stored, 0,
        "an aborted sync must not publish staged episodes"
    );
    assert!(super::super::store::subscription(&db, subscription_id)
        .unwrap()
        .is_none());
}
