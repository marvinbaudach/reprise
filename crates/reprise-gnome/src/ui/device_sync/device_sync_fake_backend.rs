//! The recording `DeviceBackend` double every device-sync test drives
//! instead of a real or simulated phone (see the trait's own doc comment
//! on why: `MTP-23`). Extracted out of `device_sync_runtime_tests.rs` to
//! keep that file under the 800-line architecture gate — every test module
//! under `device_sync_runtime_tests` reaches this through its own
//! `use super::*;`, unchanged.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::browser::StorageOption;
use reprise_core::device_sync::{
    DeviceStorageAccess, DeviceStorageInspection, DeviceStorageSnapshot, ManagedDeviceFile,
    StorageId, SyncTarget,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor};
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};

use crate::ui::device_sync::device_sync_runtime::*;

pub(super) type TestFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
type DeviceSubscriber = Rc<dyn Fn(Vec<DeviceDescriptor>)>;
type DeleteObserver = Rc<dyn Fn(&str)>;
/// `MTP-46`: lets a test act *between* the phases of one sync — the mirror
/// copies first, the content phase runs after, and only an observer fired in
/// between can simulate the user flipping a switch mid-transfer.
type CopyObserver = Rc<dyn Fn(&str)>;

#[derive(Clone)]
struct CopyGate {
    started: async_channel::Sender<String>,
    releases: HashMap<String, async_channel::Receiver<()>>,
}

#[derive(Clone)]
struct PlaylistGate {
    started: async_channel::Sender<()>,
    release: async_channel::Receiver<()>,
}

#[derive(Clone)]
struct InspectionGate {
    started: async_channel::Sender<()>,
    release: async_channel::Receiver<()>,
}

#[derive(Default)]
pub(super) struct FakeState {
    pub(super) devices: RefCell<Vec<DeviceDescriptor>>,
    subscribers: RefCell<Vec<DeviceSubscriber>>,
    pub(super) copy_order: RefCell<Vec<(String, String)>>,
    pub(super) copy_attempts: Cell<usize>,
    pub(super) active_by_device: RefCell<HashMap<String, usize>>,
    pub(super) max_by_device: RefCell<HashMap<String, usize>>,
    pub(super) active_total: Cell<usize>,
    pub(super) max_total: Cell<usize>,
    pub(super) playlists: RefCell<Vec<(String, String, Vec<u8>)>>,
    pub(super) deleted: RefCell<Vec<String>>,
    /// Every `replace_track`/`delete_track` call that reached this double,
    /// recorded as `(target_path, relative_path)` — the seam's proof that
    /// the right named target (`MTP-38`) was used, without touching a real
    /// or simulated filesystem.
    pub(super) managed_copies: RefCell<Vec<(String, String)>>,
    pub(super) managed_deleted: RefCell<Vec<(String, String)>>,
    /// `MTP-38`/finding-1 proof: the `storage_id` each `replace_track`/
    /// `delete_track`/`replace_playlist` call actually reached this double
    /// with, keyed by `target_path` — the seam a test uses to prove a
    /// device's persisted per-target storage choice is what the transfer
    /// layer uses, not just what `device.targets` records in memory.
    pub(super) transfer_storage_ids: RefCell<Vec<(String, Option<StorageId>)>>,
    pub(super) last_inspected_targets: RefCell<Option<[SyncTarget; 3]>>,
    pub(super) podcast_files: RefCell<Vec<ManagedDeviceFile>>,
    pub(super) youtube_files: RefCell<Vec<ManagedDeviceFile>>,
    pub(super) ejected: RefCell<Vec<String>>,
    pub(super) planned_operations: RefCell<Vec<(String, &'static str)>>,
    available_bytes: Cell<Option<u64>>,
    total_bytes: Cell<Option<u64>>,
    storage_access: Cell<DeviceStorageAccess>,
    transcode_probe_error: RefCell<Option<String>>,
    cleanup_error: RefCell<Option<String>>,
    /// Forces every `replace_track` call to fail, whatever target it is
    /// aimed at — the content-phase counterpart of `playlist_error` /
    /// `cleanup_error` below. Used to prove a failed podcast/YouTube copy
    /// must stop the run before later removals (`MTP-23`).
    replace_track_error: RefCell<Option<String>>,
    copy_gate: RefCell<Option<CopyGate>>,
    playlist_error: RefCell<Option<String>>,
    playlist_gate: RefCell<Option<PlaylistGate>>,
    inspection_gate: RefCell<Option<InspectionGate>>,
    inspection_error: RefCell<Option<String>>,
    delete_observer: RefCell<Option<DeleteObserver>>,
    copy_observer: RefCell<Option<CopyObserver>>,
    /// Design 7d's folder browser (`MTP-31`/`MTP-32`) double: storages this
    /// backend claims to have, and a `(storage, path) -> children` map the
    /// tests populate directly instead of touching a real filesystem.
    storages: RefCell<Vec<StorageOption>>,
    folders: RefCell<HashMap<(u32, String), Vec<String>>>,
    folder_create_error: RefCell<Option<String>>,
    created_folders: RefCell<Vec<(u32, String, String)>>,
    moved_folders: RefCell<Vec<(u32, String, String)>>,
}

#[derive(Clone)]
pub(super) struct FakeBackend {
    pub(super) state: Rc<FakeState>,
    delay_ms: u64,
}

impl FakeBackend {
    pub(super) fn new(devices: Vec<DeviceDescriptor>, delay_ms: u64) -> Self {
        let state = Rc::new(FakeState::default());
        state.devices.replace(devices);
        state.available_bytes.set(Some(1_000_000));
        state.total_bytes.set(Some(2_000_000));
        Self { state, delay_ms }
    }

    pub(super) fn with_available_bytes(self, available_bytes: Option<u64>) -> Self {
        self.state.available_bytes.set(available_bytes);
        self
    }

    pub(super) fn with_storage_access(self, access: DeviceStorageAccess) -> Self {
        self.state.storage_access.set(access);
        self
    }

    pub(super) fn with_transcode_probe_error(self, error: &str) -> Self {
        self.state.transcode_probe_error.replace(Some(error.into()));
        self
    }

    /// Came in with the dev merge: `MTP-21`'s proven-transfer work publishes
    /// through a `.part` file, which makes a failed cleanup a case a test has
    /// to be able to force.
    pub(super) fn with_cleanup_error(self, error: &str) -> Self {
        self.state.cleanup_error.replace(Some(error.into()));
        self
    }

    pub(super) fn with_playlist_error(self, error: &str) -> Self {
        self.state.playlist_error.replace(Some(error.into()));
        self
    }

    /// Makes every `replace_track` call fail on demand — the podcast/YouTube
    /// content-phase counterpart of `with_playlist_error`. A test that wants
    /// only the content copy to fail, not any music-mirror copy in the same
    /// run, must keep the device's library selection empty so the mirror
    /// never calls `replace_track` itself (see the content-phase tests).
    pub(super) fn with_replace_track_error(self, error: &str) -> Self {
        self.state.replace_track_error.replace(Some(error.into()));
        self
    }

    pub(super) fn set_available_bytes(&self, available_bytes: Option<u64>) {
        self.state.available_bytes.set(available_bytes);
    }

    pub(super) fn set_devices(&self, devices: &[DeviceDescriptor]) {
        self.state.devices.replace(devices.to_owned());
        let subscribers = self.state.subscribers.borrow().clone();
        for subscriber in subscribers {
            subscriber(devices.to_owned());
        }
    }

    pub(super) fn gate_copies(
        &self,
        device_ids: &[&str],
    ) -> (
        async_channel::Receiver<String>,
        HashMap<String, async_channel::Sender<()>>,
    ) {
        let (started, started_rx) = async_channel::unbounded();
        let mut releases = HashMap::new();
        let mut release_senders = HashMap::new();
        for device_id in device_ids {
            let (release, release_rx) = async_channel::unbounded();
            releases.insert((*device_id).to_string(), release_rx);
            release_senders.insert((*device_id).to_string(), release);
        }
        self.state
            .copy_gate
            .replace(Some(CopyGate { started, releases }));
        (started_rx, release_senders)
    }

    pub(super) fn gate_playlist(&self) -> (async_channel::Receiver<()>, async_channel::Sender<()>) {
        let (started, started_rx) = async_channel::bounded(1);
        let (release, release_rx) = async_channel::bounded(1);
        self.state.playlist_gate.replace(Some(PlaylistGate {
            started,
            release: release_rx,
        }));
        (started_rx, release)
    }

    pub(super) fn gate_next_inspection(
        &self,
    ) -> (async_channel::Receiver<()>, async_channel::Sender<()>) {
        let (started, started_rx) = async_channel::bounded(1);
        let (release, release_rx) = async_channel::bounded(1);
        self.state.inspection_gate.replace(Some(InspectionGate {
            started,
            release: release_rx,
        }));
        (started_rx, release)
    }

    pub(super) fn fail_next_inspection(&self, error: &str) {
        self.state.inspection_error.replace(Some(error.into()));
    }

    pub(super) fn observe_copies(&self, observer: CopyObserver) {
        self.state.copy_observer.replace(Some(observer));
    }

    pub(super) fn observe_deletes(&self, observer: DeleteObserver) {
        self.state.delete_observer.replace(Some(observer));
    }

    pub(super) fn with_storages(self, storages: Vec<StorageOption>) -> Self {
        self.state.storages.replace(storages);
        self
    }

    pub(super) fn set_folder_listing(&self, storage: StorageId, path: &str, children: &[&str]) {
        self.state.folders.borrow_mut().insert(
            (storage.0, path.to_string()),
            children.iter().map(|name| (*name).to_string()).collect(),
        );
    }

    pub(super) fn with_folder_create_error(self, error: &str) -> Self {
        self.state.folder_create_error.replace(Some(error.into()));
        self
    }

    pub(super) fn created_folders(&self) -> Vec<(u32, String, String)> {
        self.state.created_folders.borrow().clone()
    }

    pub(super) fn moved_folders(&self) -> Vec<(u32, String, String)> {
        self.state.moved_folders.borrow().clone()
    }
}

impl DeviceBackend for FakeBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        self.state.devices.borrow().clone()
    }

    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {
        self.state.subscribers.borrow_mut().push(callback);
    }

    fn inspect(
        &self,
        _root_uri: String,
        targets: [SyncTarget; 3],
    ) -> TestFuture<DeviceStorageInspection> {
        let available_bytes = self.state.available_bytes.get();
        let total_bytes = self.state.total_bytes.get();
        let storage_access = self.state.storage_access.get();
        let gate = self.state.inspection_gate.borrow_mut().take();
        let inspection_error = self.state.inspection_error.borrow_mut().take();
        let podcast_files = self.state.podcast_files.borrow().clone();
        let youtube_files = self.state.youtube_files.borrow().clone();
        self.state.last_inspected_targets.replace(Some(targets));
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.started
                    .send(())
                    .await
                    .map_err(|_| "inspection-start observer was dropped".to_string())?;
                gate.release
                    .recv()
                    .await
                    .map_err(|_| "inspection gate was dropped".to_string())?;
            }
            if let Some(error) = inspection_error {
                return Err(error);
            }
            Ok(DeviceStorageInspection {
                snapshot: DeviceStorageSnapshot {
                    target_name: Some("Internal shared storage".into()),
                    access: storage_access,
                    free_bytes: available_bytes,
                    total_bytes,
                    ..DeviceStorageSnapshot::default()
                },
                managed_files: Vec::new(),
                podcast_files,
                youtube_files,
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        device_id: String,
        _root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        _source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> TestFuture<CopyOutcome> {
        let state = self.state.clone();
        let delay_ms = self.delay_ms;
        state
            .transfer_storage_ids
            .borrow_mut()
            .push((target_path.clone(), storage_id));
        Box::pin(async move {
            if let Some(error) = state.replace_track_error.borrow().clone() {
                return Err(error);
            }
            state
                .planned_operations
                .borrow_mut()
                .push((device_id.clone(), "copy"));
            state.copy_attempts.set(state.copy_attempts.get() + 1);
            {
                let mut active = state.active_by_device.borrow_mut();
                let count = active.entry(device_id.clone()).or_default();
                *count += 1;
                let current = *count;
                let mut maxima = state.max_by_device.borrow_mut();
                let maximum = maxima.entry(device_id.clone()).or_default();
                *maximum = (*maximum).max(current);
            }
            let active_total = state.active_total.get() + 1;
            state.active_total.set(active_total);
            state.max_total.set(state.max_total.get().max(active_total));
            progress(expected_size / 2, expected_size);
            let gate = state.copy_gate.borrow().clone();
            if let Some(gate) = gate {
                gate.started
                    .send(device_id.clone())
                    .await
                    .map_err(|_| "copy-start observer was dropped".to_string())?;
                gate.releases
                    .get(&device_id)
                    .ok_or_else(|| format!("missing copy gate for {device_id}"))?
                    .recv()
                    .await
                    .map_err(|_| format!("copy gate for {device_id} was dropped"))?;
            } else {
                gtk4::glib::timeout_future(Duration::from_millis(delay_ms)).await;
            }
            let current = state.active_total.get();
            state.active_total.set(current.saturating_sub(1));
            if let Some(active) = state.active_by_device.borrow_mut().get_mut(&device_id) {
                *active = active.saturating_sub(1);
            }
            if cancellable.is_cancelled() {
                let stale_progress = progress.clone();
                gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
                    gtk4::glib::timeout_future(Duration::from_millis(5)).await;
                    stale_progress(expected_size.saturating_mul(10), expected_size);
                });
                return Err("cancelled".into());
            }
            let observer = state.copy_observer.borrow().clone();
            if let Some(observer) = observer {
                observer(&relative_target);
            }
            state
                .copy_order
                .borrow_mut()
                .push((device_id, relative_target.clone()));
            state
                .managed_copies
                .borrow_mut()
                .push((target_path, relative_target));
            Ok(CopyOutcome::Copied)
        })
    }

    fn cleanup_partials(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
    ) -> BackendFuture<u32> {
        let error = self.state.cleanup_error.borrow().clone();
        Box::pin(async move { error.map_or(Ok(0), Err) })
    }

    fn probe_transcode(&self, _profile: TranscodeProfile) -> Result<(), String> {
        self.state
            .transcode_probe_error
            .borrow()
            .clone()
            .map_or(Ok(()), Err)
    }

    fn transcode_track(
        &self,
        request: TranscodeRequest,
        _cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> TestFuture<TranscodedFile> {
        let state = self.state.clone();
        Box::pin(async move {
            state
                .planned_operations
                .borrow_mut()
                .push(("fake".into(), "transcode"));
            Ok(TranscodedFile {
                path: request.output,
                size_bytes: 100,
            })
        })
    }

    fn delete_track(
        &self,
        _root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        relative_target: String,
    ) -> TestFuture<bool> {
        let state = self.state.clone();
        state
            .transfer_storage_ids
            .borrow_mut()
            .push((target_path.clone(), storage_id));
        Box::pin(async move {
            let observer = state.delete_observer.borrow().clone();
            if let Some(observer) = observer {
                observer(&relative_target);
            }
            state.deleted.borrow_mut().push(relative_target.clone());
            state
                .managed_deleted
                .borrow_mut()
                .push((target_path, relative_target));
            Ok(true)
        })
    }

    fn replace_playlist(
        &self,
        device_id: String,
        _root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        name: String,
        contents: Vec<u8>,
    ) -> TestFuture<()> {
        let state = self.state.clone();
        state
            .transfer_storage_ids
            .borrow_mut()
            .push((target_path, storage_id));
        Box::pin(async move {
            if let Some(error) = state.playlist_error.borrow().clone() {
                return Err(error);
            }
            let gate = state.playlist_gate.borrow().clone();
            if let Some(gate) = gate {
                gate.started
                    .send(())
                    .await
                    .map_err(|_| "playlist-start observer was dropped".to_string())?;
                gate.release
                    .recv()
                    .await
                    .map_err(|_| "playlist gate was dropped".to_string())?;
            }
            state
                .playlists
                .borrow_mut()
                .push((device_id, name, contents));
            Ok(())
        })
    }

    fn eject(&self, device_id: String) -> TestFuture<bool> {
        let state = self.state.clone();
        Box::pin(async move {
            state.ejected.borrow_mut().push(device_id);
            Ok(true)
        })
    }

    fn list_storages(&self, _root_uri: String) -> TestFuture<Vec<StorageOption>> {
        let storages = self.state.storages.borrow().clone();
        Box::pin(async move { Ok(storages) })
    }

    fn list_folders(
        &self,
        _root_uri: String,
        storage: StorageId,
        path: String,
    ) -> TestFuture<Vec<String>> {
        let folders = self
            .state
            .folders
            .borrow()
            .get(&(storage.0, path))
            .cloned()
            .unwrap_or_default();
        Box::pin(async move { Ok(folders) })
    }

    fn create_folder(
        &self,
        _root_uri: String,
        storage: StorageId,
        path: String,
        name: String,
    ) -> TestFuture<()> {
        let state = self.state.clone();
        Box::pin(async move {
            if let Some(error) = state.folder_create_error.borrow_mut().take() {
                return Err(error);
            }
            state
                .created_folders
                .borrow_mut()
                .push((storage.0, path.clone(), name.clone()));
            state
                .folders
                .borrow_mut()
                .entry((storage.0, path))
                .or_default()
                .push(name);
            Ok(())
        })
    }

    fn move_folder(
        &self,
        _root_uri: String,
        storage: StorageId,
        from_path: String,
        to_path: String,
    ) -> TestFuture<()> {
        let state = self.state.clone();
        Box::pin(async move {
            state
                .moved_folders
                .borrow_mut()
                .push((storage.0, from_path, to_path));
            Ok(())
        })
    }
}

pub(super) fn descriptor(id: &str, reconnectable: bool) -> DeviceDescriptor {
    DeviceDescriptor {
        id: id.into(),
        name: format!("Phone {id}"),
        root_uri: format!("mtp://{id}"),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        reconnectable,
    }
}
