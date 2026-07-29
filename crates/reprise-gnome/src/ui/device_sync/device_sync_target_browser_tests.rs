use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use gtk4::gio;
use reprise_core::device_sync::browser::StorageKind;
use reprise_core::device_sync::DeviceStorageInspection;
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor};

use crate::ui::device_sync::device_sync_runtime::{BackendFuture, DeviceBackend};

use super::*;

#[test]
fn mtp_31_path_navigation_pushes_and_pops_components() {
    assert_eq!(push_path("/", "Music"), "/Music");
    assert_eq!(push_path("/Music", "Reprise"), "/Music/Reprise");
    assert_eq!(parent_path("/Music/Reprise"), Some("/Music".to_string()));
    assert_eq!(parent_path("/Music"), Some("/".to_string()));
    assert_eq!(parent_path("/"), None);
}

#[test]
fn mtp_31_preview_text_names_the_resolved_storage_and_path() {
    assert_eq!(
        preview_text(&TargetPreview::Resolved {
            storage_name: "Internal shared storage".to_string(),
            path: "/Music/Reprise-YouTube".to_string(),
        }),
        "Files will be stored at Internal shared storage → /Music/Reprise-YouTube"
    );
    assert!(preview_text(&TargetPreview::Unresolved {
        path: "/Music/Reprise-YouTube".to_string()
    })
    .contains("once a storage is chosen"));
    assert!(preview_text(&TargetPreview::StorageMissing {
        path: "/Music/Reprise-YouTube".to_string()
    })
    .contains("no longer available"));
}

#[test]
fn mtp_31_conflict_warning_only_fires_against_an_actual_playlist_target() {
    let playlists = SyncTarget {
        kind: SyncTargetKind::Playlists,
        storage_id: Some(StorageId(1)),
        path: "/Music/Reprise".to_string(),
        enabled: true,
        cap_bytes: None,
    };
    let state = BrowserState {
        original: SyncTarget {
            kind: SyncTargetKind::YoutubeAudio,
            storage_id: Some(StorageId(1)),
            path: "/Music/Reprise-YouTube".to_string(),
            enabled: true,
            cap_bytes: None,
        },
        playlist_target: Some(playlists.clone()),
        storages: vec![StorageOption {
            id: StorageId(1),
            name: "Internal".to_string(),
            kind: StorageKind::Internal,
        }],
        storage: Some(StorageId(1)),
        path: "/Music/Reprise/Nested".to_string(),
    };
    let conflicts = state.playlist_target.as_ref().is_some_and(|playlist| {
        folder_conflicts_with_playlist_target(state.storage, &state.path, playlist)
    });
    assert!(conflicts);
}

// `MTP-35`: the Save button's persistence-result handling, tested directly
// against `handle_save_result` — no display needed, since the function
// only ever touches its caller-supplied callbacks, never a concrete GTK
// widget.

#[test]
fn mtp_35_a_refused_save_reports_inline_and_leaves_the_dialog_open() {
    let closed = Rc::new(Cell::new(false));
    let shown_error = Rc::new(RefCell::new(None::<String>));
    let closed_for_call = closed.clone();
    let shown_for_call = shown_error.clone();

    handle_save_result(
        Err("device synchronization is active".to_string()),
        move |message| *shown_for_call.borrow_mut() = Some(message.to_string()),
        move || closed_for_call.set(true),
    );

    assert!(!closed.get(), "a refused save must not close the dialog");
    assert_eq!(
        shown_error.borrow().as_deref(),
        Some("Could not save: device synchronization is active"),
        "the refusal must be reported inline, not swallowed"
    );
}

#[test]
fn mtp_35_a_successful_save_closes_the_dialog_without_an_error() {
    let closed = Rc::new(Cell::new(false));
    let shown_error = Rc::new(RefCell::new(None::<String>));
    let closed_for_call = closed.clone();
    let shown_for_call = shown_error.clone();

    handle_save_result(
        Ok(()),
        move |message| *shown_for_call.borrow_mut() = Some(message.to_string()),
        move || closed_for_call.set(true),
    );

    assert!(
        closed.get(),
        "a successful save must still close the dialog"
    );
    assert!(shown_error.borrow().is_none());
}

// `MTP-34`: the folder-browser navigation race, exercised through
// `load_folders_if_current` directly with a minimal `DeviceBackend` double
// that can delay one path's listing — the same seam `present()`'s
// navigation closures call, so this proves the actual guard, not a
// stand-in predicate.

struct RaceBackend {
    descriptor: DeviceDescriptor,
    folders: RefCell<HashMap<(u32, String), Vec<String>>>,
    delays: RefCell<HashMap<String, u64>>,
}

impl RaceBackend {
    fn new(device_id: &str) -> Self {
        Self {
            descriptor: DeviceDescriptor {
                id: device_id.to_string(),
                name: "Race Phone".to_string(),
                root_uri: format!("mtp://{device_id}"),
                icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
                reconnectable: true,
            },
            folders: RefCell::new(HashMap::new()),
            delays: RefCell::new(HashMap::new()),
        }
    }

    fn set_listing(&self, storage: StorageId, path: &str, children: &[&str]) {
        self.folders.borrow_mut().insert(
            (storage.0, path.to_string()),
            children.iter().map(|name| (*name).to_string()).collect(),
        );
    }

    fn set_delay(&self, path: &str, milliseconds: u64) {
        self.delays
            .borrow_mut()
            .insert(path.to_string(), milliseconds);
    }
}

impl DeviceBackend for RaceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor.clone()]
    }

    fn subscribe_devices(&self, _callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {}

    fn inspect(
        &self,
        _root_uri: String,
        _targets: [SyncTarget; 3],
    ) -> BackendFuture<DeviceStorageInspection> {
        Box::pin(async { Ok(DeviceStorageInspection::default()) })
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        _device_id: String,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _source_path: std::path::PathBuf,
        _relative_target: String,
        _expected_size: u64,
        _cancellable: gio::Cancellable,
        _progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async { Err("not supported by this test double".into()) })
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
        Box::pin(async { Err("not supported by this test double".into()) })
    }

    fn list_folders(
        &self,
        _root_uri: String,
        storage: StorageId,
        path: String,
    ) -> BackendFuture<Vec<String>> {
        let folders = self
            .folders
            .borrow()
            .get(&(storage.0, path.clone()))
            .cloned()
            .unwrap_or_default();
        let delay_ms = self.delays.borrow().get(&path).copied();
        Box::pin(async move {
            if let Some(delay_ms) = delay_ms {
                gtk4::glib::timeout_future(Duration::from_millis(delay_ms)).await;
            }
            Ok(folders)
        })
    }
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let context = gtk4::glib::MainContext::new();
    context
        .with_thread_default(|| context.block_on(future))
        .unwrap()
}

#[test]
fn mtp_34_a_stale_folder_listing_is_dropped_not_appended_to_a_newer_navigation() {
    run(async {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let conn = Rc::new(RefCell::new(conn));

        let backend = Rc::new(RaceBackend::new("race-phone"));
        backend.set_listing(StorageId(1), "/Music", &["ShouldNotAppear"]);
        backend.set_listing(StorageId(1), "/Podcasts", &["Reprise"]);
        // Opening `/Music` is slow; the user has already moved on to
        // `/Podcasts` by the time it would resolve.
        backend.set_delay("/Music", 40);

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        for _ in 0..5 {
            gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        }

        let generation = Rc::new(Cell::new(0_u64));
        let music_result: Rc<RefCell<Option<Vec<String>>>> = Rc::new(RefCell::new(None));
        let podcasts_result: Rc<RefCell<Option<Vec<String>>>> = Rc::new(RefCell::new(None));

        // Navigation 1: `/Music` — slow, becomes stale before it resolves.
        generation.set(1);
        {
            let runtime = runtime.clone();
            let generation = generation.clone();
            let music_result = music_result.clone();
            gtk4::glib::MainContext::ref_thread_default().spawn_local(load_folders_if_current(
                runtime,
                "race-phone".to_string(),
                StorageId(1),
                "/Music".to_string(),
                generation,
                1,
                move |folders| *music_result.borrow_mut() = Some(folders),
                |_error| {},
            ));
        }

        // A few milliseconds later the user navigates to `/Podcasts` —
        // fast, no artificial delay.
        gtk4::glib::timeout_future(Duration::from_millis(5)).await;
        generation.set(2);
        {
            let runtime = runtime.clone();
            let generation = generation.clone();
            let podcasts_result = podcasts_result.clone();
            gtk4::glib::MainContext::ref_thread_default().spawn_local(load_folders_if_current(
                runtime,
                "race-phone".to_string(),
                StorageId(1),
                "/Podcasts".to_string(),
                generation,
                2,
                move |folders| *podcasts_result.borrow_mut() = Some(folders),
                |_error| {},
            ));
        }

        // Long enough for both the fast `/Podcasts` listing and the slow
        // `/Music` listing to have resolved.
        for _ in 0..30 {
            gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        }

        assert_eq!(
            podcasts_result.borrow().clone(),
            Some(vec!["Reprise".to_string()]),
            "the current navigation's own listing must still land"
        );
        assert_eq!(
            music_result.borrow().clone(),
            None,
            "the stale /Music listing must be dropped once /Podcasts became current, \
             not appended under it"
        );
    });
}
