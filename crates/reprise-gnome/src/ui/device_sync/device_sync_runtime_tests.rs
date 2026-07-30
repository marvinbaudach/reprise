use std::cell::{Cell, RefCell};
use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
// Re-exported for the `#[path]` child test modules below, which reach it
// through `use super::*;` rather than importing it a second time.
#[allow(unused_imports)]
use gtk4::gio::prelude::*;
use reprise_core::db::Db;
use reprise_core::device_sync::browser::{StorageKind, StorageOption};
use reprise_core::device_sync::settings::save_settings;
use reprise_core::device_sync::{
    DeviceSelection, DeviceSettings, DeviceStorageAccess, DeviceStorageInspection,
    DeviceStorageSnapshot, ManagedDeviceFile, SelectionSource, StorageId, SyncTargetKind,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor};
#[allow(unused_imports)]
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};

use super::device_sync_runtime::*;

#[path = "device_sync_fake_backend.rs"]
mod fake_backend;
use fake_backend::*;

fn fixture() -> (tempfile::TempDir, Rc<Db>) {
    let temp = tempfile::tempdir().unwrap();
    let db = crate::test_db::open().unwrap();
    // Both source modules ship off (`NET-1a`) and `MTP-46` makes an off module
    // contribute nothing to a sync. These tests are about what a device
    // receives once the user uses the features, so having them switched on is
    // their precondition — `MTP-46`'s own tests are the ones that flip them.
    reprise_core::modules::set_enabled(&db, &reprise_core::modules::PODCASTS_MODULE, true).unwrap();
    reprise_core::modules::set_enabled(&db, &reprise_core::modules::YOUTUBE_MODULE, true).unwrap();
    for id in 1..=4 {
        let path = temp.path().join(format!("{id}.flac"));
        std::fs::write(&path, vec![id as u8; 100]).unwrap();
        crate::test_db::connection(&db).execute(
            "INSERT INTO tracks (id,path,title,artist,duration_ms,added_at) VALUES (?1,?2,?3,'Artist',1000,0)",
            rusqlite::params![id, path.to_string_lossy(), format!("Track {id}")],
        )
        .unwrap();
    }
    (temp, Rc::new(db))
}

fn write_silent_wav(path: &std::path::Path) {
    const SAMPLE_RATE: u32 = 8_000;
    const SAMPLES: u32 = SAMPLE_RATE / 10;
    const DATA_BYTES: u32 = SAMPLES * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + DATA_BYTES).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&DATA_BYTES.to_le_bytes());
    wav.resize(wav.len() + DATA_BYTES as usize, 0);
    std::fs::write(path, wav).unwrap();
}

fn run(future: impl Future<Output = ()>) {
    let context = gtk4::glib::MainContext::new();
    context
        .with_thread_default(|| context.block_on(future))
        .unwrap();
}

async fn settle() {
    for _ in 0..100 {
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
    }
}

#[test]
fn mtp_24_podcast_and_youtube_audio_are_always_copied_1_to_1_never_transcoded() {
    run(async {
        let (downloads, conn) = fixture();
        // Named with a `.flac` extension on purpose: if this ever went
        // through the music transfer-profile branch it would be flagged
        // as lossless and transcoded. Podcast/YouTube audio must never
        // take that branch (`MTP-24`) — it copies whatever bytes exist.
        let episode_path = downloads.path().join("episode.flac");
        std::fs::write(&episode_path, b"already-opus-bytes").unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'yt-100', 'Video', 'https://example.test/video.webm',
                         ?1, 18, 1)",
                [episode_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(
            backend.state.managed_copies.borrow().as_slice(),
            [(
                "/Music/Reprise-YouTube".to_string(),
                "Channel/100-Video.flac".to_string()
            )]
        );
        assert!(
            backend
                .state
                .planned_operations
                .borrow()
                .iter()
                .all(|(_, kind)| *kind != "transcode"),
            "podcast/YouTube audio must never be transcoded"
        );
    });
}

#[test]
fn pod_12_planned_sync_copies_selected_rss_and_youtube_each_to_its_own_target() {
    run(async {
        let (downloads, conn) = fixture();
        let rss_path = downloads.path().join("rss.mp3");
        let youtube_path = downloads.path().join("youtube.mp3");
        std::fs::write(&rss_path, b"rss audio").unwrap();
        std::fs::write(&youtube_path, b"youtube").unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES
                 (10, 'rss', 'https://example.test/rss', 'RSS Show', 0, 1, 1),
                 (11, 'youtube', 'https://example.test/youtube', 'Video', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a'), (11, 'a');",
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'Episode', 'https://example.test/rss.mp3',
                         ?1, 9, 1)",
                [rss_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (101, 11, 'yt-101', 'Video', 'https://example.test/youtube.webm',
                         ?1, 7, 1)",
                [youtube_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.podcast_files.replace(vec![ManagedDeviceFile {
            relative_path: "Old Show/99-Old.mp3".into(),
            size_bytes: 4,
        }]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let page = runtime.devices().remove(0).page;
        assert!(page.blockers.is_empty());
        // Both the RSS episode (9 bytes) and the YouTube episode (7 bytes)
        // are wanted — YouTube is no longer defensively cleared (POD-12).
        assert_eq!(page.changes.transfer_bytes, 16);
        assert!(page.controls.can_start);

        runtime.sync_now("a").unwrap();
        settle().await;

        let mut copies = backend.state.managed_copies.borrow().clone();
        copies.sort();
        assert_eq!(
            copies,
            [
                (
                    "/Music/Reprise-YouTube".to_string(),
                    "Video/101-Video.mp3".to_string()
                ),
                (
                    "/Podcasts/Reprise".to_string(),
                    "RSS Show/100-Episode.mp3".to_string()
                ),
            ]
        );
        assert_eq!(
            backend.state.managed_deleted.borrow().as_slice(),
            [(
                "/Podcasts/Reprise".to_string(),
                "Old Show/99-Old.mp3".to_string()
            )]
        );
    });
}

/// `MTP-36`: three settings have shipped on this branch that rendered and
/// persisted but were never read by any code path — the global default and
/// the per-channel override must not join that list. This drives the real
/// runtime (`recompute_delta` → `sync_now`) through the DB-backed config
/// default and `podcasts::store::set_latest_per_channel`, and asserts what
/// actually reaches the fake device, never a round-trip through storage
/// alone.
/// `MTP-46` mid-transfer, the case a review found and the gate on the plan
/// alone does not cover: `recompute_all_devices` deliberately skips a device
/// that is already syncing, so the running sync still holds the plan it was
/// given. Switching a source off while the mirror phase is copying must still
/// keep that source's work — copies *and removals* — out of the content phase
/// that follows.
#[test]
fn mtp_46_switching_a_source_off_mid_sync_keeps_it_out_of_the_running_transfer() {
    run(async {
        let (downloads, conn) = fixture();
        let path = downloads.path().join("video.webm");
        std::fs::write(&path, b"video-bytes").unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, published_at, first_seen_at)
                 VALUES (101, 10, 'yt-1', 'Video 1', 'https://example.test/v.webm', ?1, 11, 1, 1)",
                rusqlite::params![path.to_string_lossy().as_ref()],
            )
            .unwrap();
        select_road_playlist(&conn, &[1]);
        save_road_settings(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        // Fires while the mirror phase copies the playlist track, which is
        // strictly before the content phase reads its plan — the exact window
        // in which the user's switch would otherwise be ignored.
        {
            let conn = conn.clone();
            backend.observe_copies(Rc::new(move |relative_target: &str| {
                if !relative_target.contains("Channel") {
                    reprise_core::modules::set_enabled(
                        &conn,
                        &reprise_core::modules::YOUTUBE_MODULE,
                        false,
                    )
                    .unwrap();
                }
            }));
        }
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(
            backend
                .state
                .managed_copies
                .borrow()
                .iter()
                .all(|(root, _)| !root.contains("Reprise-YouTube")),
            "a source switched off mid-sync must not have its episodes copied by the \
             content phase of that same sync"
        );
        assert!(
            backend.state.managed_deleted.borrow().is_empty(),
            "and the stale plan's removals must not run either"
        );
    });
}

/// `MTP-46`'s live path: that `recompute_all_devices` — what the Preferences
/// switch triggers — actually re-reads the module state into the snapshot the
/// device page renders from. Both directions are asserted on purpose: the
/// snapshot's initial value is already `false`, so checking only the
/// switched-off state would pass with the refresh deleted.
///
/// It deliberately does **not** claim to prove the non-deletion property.
/// This fake device reports no inventory for what a previous sync copied, so
/// the second plan has nothing resident it could remove and the destructive
/// case cannot arise here at all — verified by mutation: removing either gate
/// leaves this test green. `mtp_46_switching_a_source_off_never_deletes_what_
/// is_already_on_the_phone` in `device_sync::podcasts` is what guards that,
/// against an inventory it constructs itself.
#[test]
fn mtp_46_a_recompute_reloads_the_module_state_into_the_device_snapshot() {
    run(async {
        let (downloads, conn) = fixture();
        let path = downloads.path().join("video.webm");
        std::fs::write(&path, b"video-bytes").unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, published_at, first_seen_at)
                 VALUES (101, 10, 'yt-1', 'Video 1', 'https://example.test/v.webm', ?1, 11, 1, 1)",
                rusqlite::params![path.to_string_lossy().as_ref()],
            )
            .unwrap();
        // A selected playlist keeps the sync itself runnable after YouTube is
        // switched off: without one the planner refuses with
        // `NoPlaylistsSelected` and the run would prove nothing about
        // removals, because no run would happen.
        select_road_playlist(&conn, &[1]);
        // `save_road_settings` also sets `remove_deleted: true`, which is
        // exactly the setting that makes the destructive reading possible.
        save_road_settings(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        // Counted by target root, not by total: the selected playlist copies
        // a track of its own, and this test is only about YouTube's share.
        let youtube_copies = |backend: &Rc<FakeBackend>| {
            backend
                .state
                .managed_copies
                .borrow()
                .iter()
                .filter(|(root, _)| root.contains("Reprise-YouTube"))
                .count()
        };

        runtime.sync_now("a").unwrap();
        settle().await;
        assert_eq!(
            youtube_copies(&backend),
            1,
            "with YouTube on the episode reaches the device"
        );
        assert!(backend.state.managed_deleted.borrow().is_empty());
        let youtube_enabled_in_snapshot = |runtime: &Rc<DeviceSyncRuntime>| {
            runtime
                .devices()
                .iter()
                .find(|device| device.id == "a")
                .expect("device")
                .enabled_sources
                .youtube
        };
        assert!(
            youtube_enabled_in_snapshot(&runtime),
            "with YouTube on, the snapshot the device page renders from must say so"
        );

        runtime.refresh_contents("a");
        settle().await;

        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::YOUTUBE_MODULE, false)
            .unwrap();
        backend.state.managed_copies.borrow_mut().clear();
        // What the Preferences switch triggers (`window.rs` wires this to the
        // "Online sources" master switches) — not `recompute_delta`, so the
        // test exercises the same entry point the GUI does.
        runtime.recompute_all_devices();
        settle().await;
        runtime.sync_now("a").unwrap();
        settle().await;

        // Deliberately *not* asserted here: this fake's inspection returns its
        // fixed `youtube_files` fixture rather than what a previous copy put
        // on the device, so nothing is ever resident for a plan to remove and
        // an emptiness check would hold no matter what the planner did.
        // `mtp_46_switching_a_source_off_never_deletes_what_is_already_on_the_
        // phone` in `device_sync::podcasts` builds that inventory itself and
        // is where the non-deletion property is actually guarded.
        assert_eq!(
            youtube_copies(&backend),
            0,
            "and must not copy anything new from the switched-off source"
        );
        assert!(
            !youtube_enabled_in_snapshot(&runtime),
            "and after the switch and a recompute it must say the opposite"
        );
    });
}

#[test]
fn mtp_36_the_persisted_latest_per_channel_actually_bounds_what_syncs() {
    run(async {
        let (downloads, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        // Eight episodes on one channel — the design's own "8 episodes,
        // default 5" example. `downloaded_bytes` must match the file's
        // actual size exactly, or `query_candidates_for_device` silently
        // drops the episode as a stale/mismatched download.
        for n in 1..=8i64 {
            let path = downloads.path().join(format!("video-{n}.mp3"));
            let content = format!("video-{n}");
            std::fs::write(&path, content.as_bytes()).unwrap();
            crate::test_db::connection(&conn)
                .execute(
                    "INSERT INTO podcast_episodes
                     (id, subscription_id, guid, title, audio_url, downloaded_path,
                      downloaded_bytes, published_at, first_seen_at)
                     VALUES (?1, 10, ?2, ?3, 'https://example.test/video.webm', ?4, ?5, ?1, 1)",
                    rusqlite::params![
                        100 + n,
                        format!("yt-{n}"),
                        format!("Video {n}"),
                        path.to_string_lossy().as_ref(),
                        content.len() as i64
                    ],
                )
                .unwrap();
        }
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;
        assert_eq!(
            backend.state.managed_copies.borrow().len(),
            5,
            "the global default of 5 must actually bound what gets copied to the device"
        );

        reprise_core::podcasts::store::set_latest_per_channel(&conn, 10, Some(2)).unwrap();
        backend.state.managed_copies.borrow_mut().clear();
        runtime.recompute_delta("a").unwrap();
        settle().await;
        runtime.sync_now("a").unwrap();
        settle().await;
        assert_eq!(
            backend.state.managed_copies.borrow().len(),
            2,
            "a channel override of 2 must change what actually syncs — it beats the global default"
        );

        reprise_core::podcasts::store::set_latest_per_channel(&conn, 10, Some(0)).unwrap();
        backend.state.managed_copies.borrow_mut().clear();
        runtime.recompute_delta("a").unwrap();
        settle().await;
        runtime.sync_now("a").unwrap();
        settle().await;
        assert_eq!(
            backend.state.managed_copies.borrow().len(),
            8,
            "an override of 0 must sync every episode — 0 means unlimited, not empty, \
             and getting this wrong would silently stop syncing the channel"
        );
    });
}

#[test]
fn mtp_31_folder_browser_lists_storages_browses_folders_and_creates_a_new_one() {
    run(async {
        let (_downloads, conn) = fixture();
        disable_auto_start(&conn, "a");
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1).with_storages(vec![
                StorageOption {
                    id: StorageId(1),
                    name: "Internal shared storage".to_string(),
                    kind: StorageKind::Internal,
                },
                StorageOption {
                    id: StorageId(2),
                    name: "SD card".to_string(),
                    kind: StorageKind::Removable,
                },
            ]),
        );
        backend.set_folder_listing(StorageId(1), "/Music", &["Reprise", "Podcasts"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let storages = runtime.browse_storages("a").await.unwrap();
        assert_eq!(
            storages.len(),
            2,
            "both storages the backend listed come through"
        );
        assert_eq!(storages[0].name, "Internal shared storage");

        let folders = runtime
            .browse_folders("a", StorageId(1), "/Music".to_string())
            .await
            .unwrap();
        assert_eq!(folders, vec!["Reprise".to_string(), "Podcasts".to_string()]);

        runtime
            .create_target_folder(
                "a",
                StorageId(1),
                "/Music".to_string(),
                "Reprise-YouTube".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            backend.created_folders(),
            vec![(1, "/Music".to_string(), "Reprise-YouTube".to_string())],
            "the browser's New folder action reaches the backend with the exact storage and path"
        );
    });
}

#[test]
fn mtp_31_folder_browser_surfaces_a_refused_folder_creation_instead_of_pretending_success() {
    run(async {
        let (_downloads, conn) = fixture();
        disable_auto_start(&conn, "a");
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1).with_folder_create_error(
                "this device does not allow creating folders directly in the storage root",
            ),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let result = runtime
            .create_target_folder("a", StorageId(1), "/".to_string(), "Music".to_string())
            .await;

        assert_eq!(
            result,
            Err(
                "this device does not allow creating folders directly in the storage root"
                    .to_string()
            )
        );
        assert!(
            backend.created_folders().is_empty(),
            "a refused creation must not be recorded as if it happened"
        );
    });
}

#[test]
fn mtp_32_changing_a_target_folder_on_the_same_storage_relocates_it_on_the_device() {
    run(async {
        let (_downloads, conn) = fixture();
        disable_auto_start(&conn, "a");
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        // First resolution: the target had no storage yet, so this is a
        // fresh assignment, not a move (`MTP-32`'s `Unchanged` branch).
        runtime
            .set_target_folder(
                "a",
                SyncTargetKind::YoutubeAudio,
                Some(StorageId(1)),
                "/Music/Reprise-YouTube".to_string(),
            )
            .unwrap();
        settle().await;
        assert!(backend.moved_folders().is_empty());

        // Same storage, new path: a genuine rename (`MoveFolder`) — the
        // already-synced files must be relocated, not re-copied.
        runtime
            .set_target_folder(
                "a",
                SyncTargetKind::YoutubeAudio,
                Some(StorageId(1)),
                "/Music/YT".to_string(),
            )
            .unwrap();
        settle().await;
        assert_eq!(
            backend.moved_folders(),
            vec![(
                1,
                "/Music/Reprise-YouTube".to_string(),
                "/Music/YT".to_string()
            )]
        );
        assert_eq!(
            runtime
                .current_target("a", SyncTargetKind::YoutubeAudio)
                .unwrap()
                .path,
            "/Music/YT",
            "the new folder is persisted immediately, like set_target_enabled"
        );
    });
}

#[test]
fn mtp_32_changing_a_target_folder_to_a_different_storage_does_not_relocate() {
    run(async {
        let (_downloads, conn) = fixture();
        disable_auto_start(&conn, "a");
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime
            .set_target_folder(
                "a",
                SyncTargetKind::PodcastEpisodes,
                Some(StorageId(1)),
                "/Podcasts/Reprise".to_string(),
            )
            .unwrap();
        settle().await;

        runtime
            .set_target_folder(
                "a",
                SyncTargetKind::PodcastEpisodes,
                Some(StorageId(2)),
                "/Podcasts/Reprise".to_string(),
            )
            .unwrap();
        settle().await;

        assert!(
            backend.moved_folders().is_empty(),
            "a storage change must go through the copy-and-orphan path (MTP-38), not a move"
        );
    });
}

fn signal_when(
    runtime: &Rc<DeviceSyncRuntime>,
    condition: impl Fn(&DeviceSyncState) -> bool + 'static,
) -> (Subscription, async_channel::Receiver<()>) {
    let (sender, receiver) = async_channel::bounded(1);
    let subscription = runtime.subscribe(Rc::new(move |state| {
        if condition(&state) {
            let _ = sender.try_send(());
        }
    }));
    (subscription, receiver)
}

fn select_road_playlist(conn: &Rc<Db>, ids: &[i64]) {
    crate::test_db::connection(conn.as_ref())
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 0)",
            [],
        )
        .unwrap();
    for (position, track_id) in ids.iter().enumerate() {
        crate::test_db::connection(conn.as_ref())
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (10, ?1, ?2)",
                rusqlite::params![track_id, position as i64],
            )
            .unwrap();
    }
    save_road_settings(conn, "a");
}

/// `MTP-30`: seeds a device-settings row with the switch off and no
/// playlist selection, for tests that set up their own podcast/YouTube work
/// directly via SQL and then drive `sync_now` manually — without this, the
/// default-on switch (`DEFAULT 1`, schema v44) would start a sync on
/// connect before the test's own `sync_now` call runs, doubling every copy.
fn disable_auto_start(conn: &Rc<Db>, device_id: &str) {
    save_settings(
        conn,
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(Vec::new()),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            sync_automatically: false,
            prepare_before_sync: true,
        },
    )
    .unwrap();
}

fn save_road_settings(conn: &Rc<Db>, device_id: &str) {
    save_settings(
        conn,
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            // `MTP-30`: most tests using this fixture orchestrate `sync_now`
            // manually (gates, cancellation races, progress observation) and
            // must not race an automatic start on connect. `MTP-30`'s own
            // tests (`device_sync_auto_start_tests.rs`) set this explicitly
            // instead of relying on this shared fixture.
            sync_automatically: false,
            prepare_before_sync: true,
        },
    )
    .unwrap();
}

#[path = "device_sync_auto_start_tests.rs"]
mod auto_start_tests;
#[path = "device_sync_cap_tests.rs"]
mod cap_tests;
#[path = "device_sync_compact_tests.rs"]
mod compact_tests;
#[path = "device_sync_content_transfer_tests.rs"]
mod content_transfer_tests;
#[path = "device_sync_inflight_tests.rs"]
mod inflight_tests;
#[path = "device_sync_planned_tests.rs"]
mod planned_tests;
#[path = "device_sync_podcast_removal_tests.rs"]
mod podcast_removal_tests;
#[path = "device_sync_preparation_tests.rs"]
mod preparation_tests;
#[path = "device_sync_readback_tests.rs"]
mod readback_tests;
#[path = "device_sync_safety_tests.rs"]
mod safety_tests;
#[path = "device_sync_selection_summary_tests.rs"]
mod selection_summary_tests;
#[path = "device_sync_selection_tests.rs"]
mod selection_tests;
#[path = "device_sync_transfer_profile_tests.rs"]
mod transfer_profile_tests;
