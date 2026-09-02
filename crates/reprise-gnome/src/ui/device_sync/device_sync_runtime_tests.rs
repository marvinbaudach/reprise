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
    DeviceStorageSnapshot, ManagedDeviceFile, SelectionSource, StorageId,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor};
#[allow(unused_imports)]
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};

use super::device_sync_runtime::*;

const SETTLE_UNTIL_TIMEOUT: Duration = Duration::from_secs(5);

#[path = "device_sync_fake_backend.rs"]
mod fake_backend;
use fake_backend::*;

#[path = "device_sync_agent_log_tests.rs"]
mod agent_log_tests;
#[path = "device_sync_memory_tests.rs"]
mod memory_tests;
#[path = "device_sync_presence_tests.rs"]
mod presence_tests;
#[path = "device_sync_remembered_tests.rs"]
mod remembered_tests;
#[path = "device_sync_target_fixture.rs"]
mod target_fixture;
use target_fixture::disable_auto_start;

fn fixture() -> (tempfile::TempDir, Rc<Db>) {
    let temp = tempfile::tempdir().unwrap();
    let db = crate::test_db::open().unwrap();
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

async fn settle_until(label: &str, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + SETTLE_UNTIL_TIMEOUT;
    while !condition() {
        assert!(std::time::Instant::now() < deadline, "timed out: {label}");
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
    }
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
        backend.set_folder_listing(StorageId(1), "/Music", &["Reprise", "Audiobooks"]);
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
        assert_eq!(
            folders,
            vec!["Reprise".to_string(), "Audiobooks".to_string()]
        );

        runtime
            .create_target_folder(
                "a",
                StorageId(1),
                "/Music".to_string(),
                "Selected".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            backend.created_folders(),
            vec![(1, "/Music".to_string(), "Selected".to_string())],
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
            .set_target_folder("a", Some(StorageId(1)), "/Music/Reprise".to_string())
            .unwrap();
        settle().await;
        assert!(backend.moved_folders().is_empty());

        // Same storage, new path: a genuine rename (`MoveFolder`) — the
        // already-synced files must be relocated, not re-copied.
        runtime
            .set_target_folder(
                "a",
                Some(StorageId(1)),
                "/Music/Reprise-Renamed".to_string(),
            )
            .unwrap();
        settle().await;
        assert_eq!(
            backend.moved_folders(),
            vec![(
                1,
                "/Music/Reprise".to_string(),
                "/Music/Reprise-Renamed".to_string()
            )]
        );
        assert_eq!(
            runtime.current_target("a").unwrap().path,
            "/Music/Reprise-Renamed",
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
            .set_target_folder("a", Some(StorageId(1)), "/Music/Reprise".to_string())
            .unwrap();
        settle().await;

        runtime
            .set_target_folder("a", Some(StorageId(2)), "/Music/Reprise".to_string())
            .unwrap();
        settle().await;

        assert!(
            backend.moved_folders().is_empty(),
            "a storage change must go through the copy-and-orphan path, not a move"
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

fn save_road_settings(conn: &Rc<Db>, device_id: &str) {
    save_settings(
        conn,
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            remove_deleted: true,
            // `MTP-30`: most tests using this fixture orchestrate `sync_now`
            // manually (gates, cancellation races, progress observation) and
            // must not race an automatic start on connect. `MTP-30`'s own
            // tests (`device_sync_auto_start_tests.rs`) set this explicitly
            // instead of relying on this shared fixture.
            sync_automatically: false,
        },
    )
    .unwrap();
}

#[path = "device_sync_analysis_metadata_tests.rs"]
mod analysis_metadata_tests;
#[path = "device_sync_analysis_planning_warning_tests.rs"]
mod analysis_planning_warning_tests;
#[path = "device_sync_auto_start_tests.rs"]
mod auto_start_tests;
#[path = "device_sync_compact_tests.rs"]
mod compact_tests;
#[path = "device_sync_inflight_tests.rs"]
mod inflight_tests;
#[path = "device_sync_listen_report_tests.rs"]
mod listen_report_tests;
#[path = "device_sync_lyrics_sidecar_tests.rs"]
mod lyrics_sidecar_tests;
#[path = "device_sync_picker_tests.rs"]
mod picker_tests;
#[path = "device_sync_planned_tests.rs"]
mod planned_tests;
#[path = "device_sync_readback_tests.rs"]
mod readback_tests;
#[path = "device_sync_run_log_tests.rs"]
mod run_log_tests;
#[path = "device_sync_safety_tests.rs"]
mod safety_tests;
#[path = "device_sync_transfer_profile_tests.rs"]
mod transfer_profile_tests;
