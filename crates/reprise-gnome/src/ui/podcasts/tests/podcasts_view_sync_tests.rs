//! Initial-sync action tests split from the main Podcasts view inventory.

use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_26_the_rows_cancel_action_trips_the_core_abort_handle() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::YOUTUBE_MODULE, true)
        .unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/channel".into(),
            title: "Channel".into(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let view = view(conn, PodcastKind::Youtube);
    let abort = podcasts::pipeline::SyncAbort::new();
    view.syncing
        .borrow_mut()
        .insert(subscription_id, SyncRowState::new(abort.clone()));
    view.render();
    let _window = present(&view);
    let cancel = descendant_buttons(&view.sync_widgets.borrow()[&subscription_id].root)
        .into_iter()
        .find(|button| button.action_name().as_deref() == Some("podcasts.cancel-sync"))
        .expect("loading row Cancel button");

    cancel.emit_clicked();

    assert!(abort.is_cancelled());
    assert!(!view.syncing.borrow().contains_key(&subscription_id));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_successful_refresh_clears_a_failed_initial_sync_that_now_has_episodes() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::YOUTUBE_MODULE, true)
        .unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/channel".into(),
            title: "Channel".into(),
            author: None,
            image_url: Some("https://youtube.test/avatar.jpg".into()),
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        &conn,
        subscription_id,
        &reprise_core::podcasts::feed::ParsedEpisode {
            guid: "episode-1".into(),
            title: "Episode".into(),
            image_url: None,
            audio_url: "https://youtube.test/watch/episode-1".into(),
            page_url: None,
            published_at: Some(2),
            duration_secs: None,
        },
        2,
    )
    .unwrap();
    let view = view(conn, PodcastKind::Youtube);
    let mut failed = SyncRowState::new(podcasts::pipeline::SyncAbort::new());
    failed.apply(&podcasts::pipeline::SyncProgress::Failed(
        podcasts::pipeline::SyncError::Database,
    ));
    view.syncing.borrow_mut().insert(subscription_id, failed);

    view.refresh();

    assert!(!view.syncing.borrow().contains_key(&subscription_id));
    assert!(view.sync_widgets.borrow().get(&subscription_id).is_none());
    assert!(!view.artwork_rebinds.borrow().is_empty());
}

#[test]
fn a_stale_terminal_owner_cannot_remove_a_newer_subscription_sync() {
    let subscription_id = 7;
    let first = podcasts::pipeline::SyncAbort::new();
    let second = podcasts::pipeline::SyncAbort::new();
    let mut syncing =
        std::collections::HashMap::from([(subscription_id, SyncRowState::new(second.clone()))]);

    assert!(!remove_subscription_sync_if_owned(
        &mut syncing,
        subscription_id,
        &first,
    ));
    assert!(syncing[&subscription_id].abort.is_same_request(&second));
}
