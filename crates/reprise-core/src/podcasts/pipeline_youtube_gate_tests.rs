//! NET-1a: every YouTube network entry point respects the module gate.

use super::youtube_test_support::*;
use super::{RefreshRequest as R, *};
use crate::podcasts::store::{self, NewSubscription};

/// `NET-1a`: a YouTube subscription is skipped, not fetched, when the
/// YouTube module is off — this is issue #96's other half: Podcasts on
/// must not implicitly allow YouTube (see the RSS half in
/// `pipeline_refresh_tests.rs`).
#[test]
fn net_1a_disabled_youtube_module_skips_refresh_without_fetching() {
    let conn = conn();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, false).unwrap();
    let id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCabc123".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(
        &conn,
        &FakeFeedNeverCalled,
        &NeverYoutube,
        10,
        R::force(),
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.failures.len(), 1);
    assert_eq!(
        store::subscription(&conn, id)
            .unwrap()
            .unwrap()
            .last_outcome
            .as_deref(),
        Some("failed")
    );
}

/// `NET-1a`: the explicit "Load more" action also respects the gate —
/// every YouTube network entry point, not just the periodic refresh.
#[test]
fn net_1a_load_more_is_blocked_when_youtube_module_is_off() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCmore".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, false).unwrap();

    let result = load_more_youtube(&conn, &NeverYoutube, subscription_id, 40, 20);

    assert!(matches!(
        result,
        Err(PipelineError::YoutubeSourceUnavailable)
    ));
}
