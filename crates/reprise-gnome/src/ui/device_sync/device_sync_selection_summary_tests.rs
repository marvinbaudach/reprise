//! `MTP-37`: the Content section's per-category selection summary must be a
//! live read of `POD-12`'s per-device subscription selection, not a static
//! string. This is exactly the shape of bug this branch has produced three
//! times already (a control that renders and persists but nothing reads):
//! here the risk is the mirror image — a *label* that looks live but is
//! wired to nothing, so it would stay wrong forever. These tests drive the
//! real selection state (`podcasts::phone_sync::set_device_enabled`)
//! through the real runtime and assert the summary the device page would
//! render actually changes.

use super::*;

#[test]
fn mtp_37_the_youtube_selection_summary_changes_when_a_channel_is_enabled_for_this_device() {
    run(async {
        let (_downloads, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'youtube', 'https://example.test/one', 'One', 0, 0, 1),
                        (11, 'youtube', 'https://example.test/two', 'Two', 0, 0, 1);",
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let before = device_view(&runtime, "a");
        assert_eq!(
            (
                before.youtube_selection.channels_selected,
                before.youtube_selection.channels_total
            ),
            (0, 2),
            "neither channel is selected for this device yet"
        );

        reprise_core::podcasts::phone_sync::set_device_enabled(&conn, 10, "a", true).unwrap();
        runtime.recompute_delta("a").unwrap();
        settle().await;

        let after = device_view(&runtime, "a");
        assert_eq!(
            (
                after.youtube_selection.channels_selected,
                after.youtube_selection.channels_total
            ),
            (1, 2),
            "enabling one channel for this device must change the live selection summary \
             the Content section renders — a summary that stays (0, 2) here would be exactly \
             the kind of dead control this branch has shipped before"
        );

        reprise_core::podcasts::phone_sync::set_device_enabled(&conn, 10, "a", false).unwrap();
        runtime.recompute_delta("a").unwrap();
        settle().await;

        let disabled_again = device_view(&runtime, "a");
        assert_eq!(
            (
                disabled_again.youtube_selection.channels_selected,
                disabled_again.youtube_selection.channels_total
            ),
            (0, 2),
            "disabling the channel again must move the summary back down"
        );
    });
}

#[test]
fn mtp_37_the_podcast_selection_summary_counts_selected_shows_independently_of_youtube() {
    run(async {
        let (_downloads, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (20, 'rss', 'https://example.test/show', 'Show', 0, 0, 1);",
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert_eq!(
            {
                let view = device_view(&runtime, "a");
                (
                    view.podcast_selection.shows_selected,
                    view.podcast_selection.shows_total,
                )
            },
            (0, 1)
        );

        reprise_core::podcasts::phone_sync::set_device_enabled(&conn, 20, "a", true).unwrap();
        runtime.recompute_delta("a").unwrap();
        settle().await;

        let view = device_view(&runtime, "a");
        assert_eq!(
            (
                view.podcast_selection.shows_selected,
                view.podcast_selection.shows_total
            ),
            (1, 1),
            "enabling the show for this device must change the podcast selection summary"
        );
        assert_eq!(
            (
                view.youtube_selection.channels_selected,
                view.youtube_selection.channels_total
            ),
            (0, 0),
            "an RSS-only fixture must not leak into the YouTube summary"
        );
    });
}

fn device_view(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> DeviceView {
    runtime
        .devices()
        .into_iter()
        .find(|device| device.id == device_id)
        .unwrap_or_else(|| panic!("device {device_id} not found"))
}
