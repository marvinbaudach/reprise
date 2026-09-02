use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::device_sync::settings::save_settings;
use reprise_core::device_sync::{
    DeviceSelection, DeviceSettings, DeviceStorageInspection, DeviceStorageSnapshot,
    SelectionSource, StorageId, SyncTarget, TransferProfile,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor};

use crate::ui::device_sync_runtime::{BackendFuture, DeviceBackend, DeviceSyncRuntime};

use super::*;

struct ConnectedDeviceBackend;

impl DeviceBackend for ConnectedDeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: "a".into(),
            persistent_id: Some("a".into()),
            name: "Phone a".into(),
            root_uri: "mtp://a".into(),
            icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
            reconnectable: true,
        }]
    }

    fn subscribe_devices(&self, _callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {}

    fn inspect(
        &self,
        _root_uri: String,
        _target: SyncTarget,
    ) -> BackendFuture<DeviceStorageInspection> {
        Box::pin(async {
            Ok(DeviceStorageInspection {
                snapshot: DeviceStorageSnapshot {
                    free_bytes: Some(1_000_000),
                    total_bytes: Some(2_000_000),
                    ..Default::default()
                },
                managed_files: Vec::new(),
                ..DeviceStorageInspection::default()
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        _device_id: String,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _source_path: PathBuf,
        _relative_target: String,
        _expected_size: u64,
        _cancellable: gio::Cancellable,
        _progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async { panic!("playlist notification test does not copy tracks") })
    }

    fn replace_playlist(
        &self,
        _device_id: String,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _name: String,
        _contents: Vec<u8>,
    ) -> BackendFuture<()> {
        Box::pin(async { panic!("playlist notification test does not write playlists") })
    }
}

fn fixture() -> (tempfile::TempDir, Rc<Db>) {
    let temp = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    for id in 1..=2 {
        let path = temp.path().join(format!("{id}.flac"));
        std::fs::write(&path, [id as u8; 100]).unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (id,path,title,artist,duration_ms,added_at) VALUES (?1,?2,?3,'Artist',1000,0)",
                rusqlite::params![id, path.to_string_lossy(), format!("Track {id}")],
            )
            .unwrap();
    }
    (temp, conn)
}

fn add_selected_playlist(conn: &Rc<Db>) {
    crate::test_db::connection(conn)
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 10)",
            [],
        )
        .unwrap();
    for (position, track_id) in [1_i64, 2].into_iter().enumerate() {
        crate::test_db::connection(conn)
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (10, ?1, ?2)",
                rusqlite::params![track_id, position as i64],
            )
            .unwrap();
    }
    save_settings(
        conn,
        &DeviceSettings {
            device_serial: "a".into(),
            device_name: "Phone a".into(),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
            profile: TransferProfile::default(),
            opus_bitrate: 0,
            remove_deleted: true,
            sync_automatically: false,
        },
    )
    .unwrap();
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
#[ignore = "requires a display; run via xvfb-run"]
fn bound_sidebar_playlist_deletion_refreshes_the_connected_device_projection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    libadwaita::init().unwrap();
    run(async {
        let (_temp, conn) = fixture();
        add_selected_playlist(&conn);
        let runtime = DeviceSyncRuntime::with_backend(&conn, Rc::new(ConnectedDeviceBackend));
        settle().await;

        let window = libadwaita::ApplicationWindow::builder().build();
        let sidebar = Sidebar::new(conn, &window, || 0);
        sidebar.bind_device_sync(&runtime, Rc::new(|_, _| {}));
        assert!(runtime.devices()[0]
            .page
            .playlists
            .iter()
            .any(|row| row.source == SelectionSource::Playlist(10)));

        super::super::sidebar_export::delete_playlist(&sidebar.shared, 10, "Road");

        assert!(!runtime.devices()[0]
            .page
            .playlists
            .iter()
            .any(|row| row.source == SelectionSource::Playlist(10)));
    });
}
