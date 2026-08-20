use std::cell::RefCell;

use super::tests::{add_subscription, conn, feed_response, FakeFeed, FakeYoutube};
use super::*;

#[test]
fn a_concurrent_download_of_the_same_episode_is_refused() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    refresh_to_root(
        &db,
        &feed,
        &FakeYoutube,
        10,
        RefreshRequest::force(),
        root.path(),
    )
    .unwrap();
    let episode_id =
        super::super::query::episodes_for_subscription(&db, subscription_id).unwrap()[0].id;
    let held = super::super::download_claims::claim(episode_id).expect("claim");

    let error = download_episode(
        &db,
        &feed,
        &FakeYoutube,
        root.path(),
        episode_id,
        &mut |_| {},
    )
    .expect_err("a claimed episode must not download twice");

    assert!(matches!(error, PipelineError::DownloadAlreadyRunning));
    drop(held);
}
