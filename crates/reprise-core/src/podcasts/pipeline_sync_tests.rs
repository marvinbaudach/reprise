use std::cell::RefCell;
use std::sync::Arc;

use super::tests::{add_subscription, conn, feed_response, FakeFeed, FakeYoutube};
use super::*;

#[test]
fn scoped_sync_coalesces_a_large_feed_to_its_final_count_before_completion() {
    let db = conn();
    super::super::config::set_import_count(&db, 0).unwrap();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 250, None))]),
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
    assert_eq!(summary.episodes_inserted, 250);
    assert_eq!(
        progress,
        vec![
            SyncProgress::Started,
            SyncProgress::FeedRead {
                episodes_found: 250,
            },
            SyncProgress::FetchingArtwork,
            SyncProgress::Done(summary),
        ]
    );
}

#[test]
fn external_removal_before_the_sync_transaction_emits_a_terminal_failure() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let db = crate::db::Db::open_migrated(Some(&database_path)).unwrap();
    crate::online_sources::set_enabled(&db, true).unwrap();
    crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let remover = crate::db::Db::open_migrated(Some(&database_path)).unwrap();
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    let mut progress = Vec::new();

    let result = sync_subscription(
        &db,
        &feed,
        &FakeYoutube,
        10,
        subscription_id,
        &SyncAbort::new(),
        &mut |event| {
            if event == (SyncProgress::FeedRead { episodes_found: 1 }) {
                super::super::store::tombstone_subscription(&remover, subscription_id, 11).unwrap();
                super::super::store::commit_remove_subscription(&remover, subscription_id).unwrap();
            }
            progress.push(event);
        },
    );

    assert!(matches!(result, Err(PipelineError::SubscriptionNotFound)));
    assert_eq!(
        progress.last(),
        Some(&SyncProgress::Failed(SyncError::SubscriptionUnavailable))
    );
}

#[test]
fn concurrent_removal_cannot_commit_between_the_active_check_and_sync_commit() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let db = crate::db::Db::open_migrated(Some(&database_path)).unwrap();
    crate::online_sources::set_enabled(&db, true).unwrap();
    crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let remover = crate::db::Db::open_migrated(Some(&database_path)).unwrap();
    remover
        .conn()
        .busy_timeout(std::time::Duration::ZERO)
        .unwrap();
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    let mut removal_was_blocked = false;

    let summary = sync_subscription(
        &db,
        &feed,
        &FakeYoutube,
        10,
        subscription_id,
        &SyncAbort::new(),
        &mut |event| {
            if event == SyncProgress::FetchingArtwork {
                let removal =
                    super::super::store::tombstone_subscription(&remover, subscription_id, 11);
                removal_was_blocked = matches!(
                    removal,
                    Err(rusqlite::Error::SqliteFailure(error, _))
                        if matches!(
                            error.code,
                            rusqlite::ErrorCode::DatabaseBusy
                                | rusqlite::ErrorCode::DatabaseLocked
                        )
                );
            }
        },
    )
    .unwrap();

    assert!(removal_was_blocked);
    assert_eq!(summary.episodes_inserted, 1);
    assert!(super::super::store::subscription(&db, subscription_id)
        .unwrap()
        .is_some());
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
            if event == (SyncProgress::FeedRead { episodes_found: 3 }) {
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
