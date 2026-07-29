//! `MTP-43`: proves the preparation surface's actual *behavior*, not just
//! that its settings round-trip or its widgets render.
//!
//! The one test that matters most here
//! (`mtp_43_the_switch_actually_gates_whether_a_preparation_download_starts`)
//! drives `DeviceSyncRuntime::sync_now` end to end through a real device
//! with a `wanted_on_device` episode that has no local file, and observes
//! whether [`PreparationDownloader::download`] was actually invoked — with
//! the switch off it must not be, with it on it must. A test that only
//! checked `DeviceSettings::prepare_before_sync` round-trips (already
//! covered in `reprise-core`) would stay green even if this whole feature
//! were deleted; this one would not.
//!
//! `FakeDownloader` resolves every request immediately (no real network, no
//! real worker thread), so nothing here can hang.

use super::*;
use std::time::Duration;

/// Resolves every request after one real main-loop suspension (`MTP-44`'s
/// actual worker also answers asynchronously) so a test can observe "this
/// download was requested" and "the loop moved on to the next one" as two
/// distinct, orderable moments — never so long that a bounded
/// [`wait_until`] could fail to catch up.
const FAKE_DOWNLOAD_DELAY: Duration = Duration::from_millis(5);

#[derive(Default)]
struct FakeDownloader {
    calls: Rc<RefCell<Vec<i64>>>,
}

impl PreparationDownloader for FakeDownloader {
    fn download(
        &self,
        episode_id: i64,
        _on_progress: Rc<dyn Fn(u64, Option<u64>)>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.borrow_mut().push(episode_id);
            gtk4::glib::timeout_future(FAKE_DOWNLOAD_DELAY).await;
            Ok(())
        })
    }
}

/// Polls `condition` on the glib main loop up to `max_iters` times, 2ms
/// apart, and returns whether it became true — a bounded wait so a
/// regression that never satisfies it fails the test instead of hanging it.
async fn wait_until(mut condition: impl FnMut() -> bool, max_iters: usize) -> bool {
    for _ in 0..max_iters {
        if condition() {
            return true;
        }
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
    }
    condition()
}

/// Seeds one RSS show enabled for `device_id`, with the given episode ids
/// `wanted_on_device` and with no `downloaded_path` — exactly `MTP-40`'s
/// "wanted but missing" shape, scoped the same way
/// `query_selection_candidates_for_device`'s own tests seed it.
fn seed_missing_wanted_episodes(
    conn: &Rc<RefCell<Connection>>,
    device_id: &str,
    episode_ids: &[i64],
) {
    let conn = conn.borrow();
    conn.execute(
        "INSERT INTO podcast_subscriptions (id, kind, feed_url, title, auto_download, added_at)
         VALUES (1, 'rss', 'https://example.test/feed', 'Show', 0, 1)",
        [],
    )
    .unwrap();
    reprise_core::podcasts::phone_sync::set_device_enabled(&conn, 1, device_id, true).unwrap();
    for episode_id in episode_ids {
        conn.execute(
            "INSERT INTO podcast_episodes (id, subscription_id, guid, title, audio_url, first_seen_at)
             VALUES (?1, 1, ?2, ?3, 'https://example.test/e.mp3', 1)",
            rusqlite::params![episode_id, format!("guid-{episode_id}"), format!("Episode {episode_id}")],
        )
        .unwrap();
        reprise_core::podcasts::wanted_on_device::set_wanted_on_device(&conn, *episode_id, true)
            .unwrap();
    }
}

fn settings_with_prepare_switch(device_id: &str, prepare_before_sync: bool) -> DeviceSettings {
    DeviceSettings {
        device_serial: device_id.into(),
        device_name: format!("Phone {device_id}"),
        selection: DeviceSelection::Sources(Vec::new()),
        profile: reprise_core::device_sync::TransferProfile::default(),
        opus_bitrate: 0,
        ratings_back: false,
        remove_deleted: true,
        // These tests drive `sync_now` manually and must not race an
        // automatic start on connect (`MTP-30`).
        sync_automatically: false,
        prepare_before_sync,
    }
}

/// The behavioral proof the task exists for: the switch's two positions
/// produce two different, observable outcomes — not a rendered value, an
/// actual download dispatch (or its absence).
#[test]
fn mtp_43_the_switch_actually_gates_whether_a_preparation_download_starts() {
    run(async {
        let (_temp, conn) = fixture();
        seed_missing_wanted_episodes(&conn, "a", &[101]);
        save_settings(&conn.borrow(), &settings_with_prepare_switch("a", false)).unwrap();

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        let off_calls: Rc<RefCell<Vec<i64>>> = Rc::default();
        runtime.bind_test_preparation_downloader(Rc::new(FakeDownloader {
            calls: off_calls.clone(),
        }));
        settle().await;

        // Switch OFF: `sync_now` must never dispatch a preparation download,
        // whatever the connection state turns out to be — `MTP-42`'s
        // `Offered`/`SkippedOffline` phases both leave the primary button
        // (and this call) a plain sync.
        let _ = runtime.sync_now("a");
        settle().await;
        assert!(
            off_calls.borrow().is_empty(),
            "the switch was off — no preparation download may start"
        );

        // Switch ON: the same missing episode must now actually be
        // requested through the download seam before the transfer runs.
        // Goes through the runtime's own setter (not a direct `save_settings`
        // write) so the in-memory `device.settings` the next `sync_now` reads
        // actually reflects it — `save_settings` alone would only change the
        // database, not the cached copy this runtime already loaded.
        runtime.set_prepare_before_sync("a", true).unwrap();
        let on_calls: Rc<RefCell<Vec<i64>>> = Rc::default();
        runtime.bind_test_preparation_downloader(Rc::new(FakeDownloader {
            calls: on_calls.clone(),
        }));

        let phase = runtime.devices().remove(0).preparation;
        assert_eq!(
            reprise_core::device_sync::primary_action(&phase),
            reprise_core::device_sync::PrimaryAction::DownloadAndSync,
            "sanity check: the seeded fixture must actually reach MTP-42's Planned phase \
             (network available, unmetered, switch on) for this half of the test to mean anything"
        );

        let _ = runtime.sync_now("a");
        settle().await;
        assert_eq!(
            on_calls.borrow().as_slice(),
            [101],
            "the switch was on and the connection allows it — the missing episode must be \
             requested through the priority download lane before the transfer starts"
        );
    });
}

/// Cancelling mid-run must stop the *next* download from ever being
/// requested, while leaving the one already in flight completely alone —
/// two missing episodes, cancel fires between them, and the assertion
/// checks both halves of that claim in one shot.
#[test]
fn mtp_43_cancelling_a_preparation_run_stops_further_downloads_but_keeps_the_one_in_flight() {
    run(async {
        let (_temp, conn) = fixture();
        seed_missing_wanted_episodes(&conn, "a", &[101, 102]);
        save_settings(&conn.borrow(), &settings_with_prepare_switch("a", true)).unwrap();

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        let calls: Rc<RefCell<Vec<i64>>> = Rc::default();
        runtime.bind_test_preparation_downloader(Rc::new(FakeDownloader {
            calls: calls.clone(),
        }));
        settle().await;

        let _ = runtime.sync_now("a");
        // Wait only until the first file has actually been requested (never
        // unbounded — `wait_until` gives up after 100 x 2ms = 200ms), then
        // cancel while it is still "in flight" inside `FAKE_DOWNLOAD_DELAY`.
        let requested = wait_until(|| !calls.borrow().is_empty(), 100).await;
        assert!(requested, "the first missing episode was never requested");
        runtime.cancel_current("a");
        // Give the loop time to notice the cancel flag and stop — bounded
        // the same way.
        wait_until(
            || {
                runtime
                    .devices()
                    .into_iter()
                    .find(|device| device.id == "a")
                    .is_none_or(|device| device.preparation_run == PreparationRunState::Idle)
            },
            100,
        )
        .await;

        assert_eq!(
            calls.borrow().as_slice(),
            [101],
            "the first episode was already in flight when cancel fired and must stay requested; \
             the second must never be requested at all once cancel wins the race"
        );
    });
}
