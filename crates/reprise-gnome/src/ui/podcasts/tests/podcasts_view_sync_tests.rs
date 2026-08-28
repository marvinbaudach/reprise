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
