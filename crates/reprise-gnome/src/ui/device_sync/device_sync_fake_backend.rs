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
use std::sync::atomic::Ordering;
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
type TranscodeObserver = Rc<dyn Fn(&std::path::Path)>;

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
    /// the selected target was used, without touching a real or simulated
    /// filesystem.
    pub(super) managed_copies: RefCell<Vec<(String, String)>>,
    /// Bytes handed to the backend for generated attachments and fake
    /// transcoded audio.
    pub(super) managed_copy_contents: RefCell<Vec<(String, String, Vec<u8>)>>,
    pub(super) managed_reads: RefCell<Vec<(String, String)>>,
    pub(super) managed_deleted: RefCell<Vec<(String, String)>>,
    /// The `storage_id` each `replace_track`/
    /// `delete_track`/`replace_playlist` call actually reached this double
    /// with, keyed by `target_path` — the seam a test uses to prove a
    /// device's persisted per-target storage choice is what the transfer
    /// layer uses, not just what `device.targets` records in memory.
    pub(super) transfer_storage_ids: RefCell<Vec<(String, Option<StorageId>)>>,
    pub(super) inspection_roots: RefCell<Vec<String>>,
    pub(super) managed_root_enumerations: Cell<u32>,
    pub(super) last_inspected_target: RefCell<Option<SyncTarget>>,
    pub(super) managed_files: RefCell<Vec<ManagedDeviceFile>>,
    pub(super) partial_paths: RefCell<Vec<String>>,
    pub(super) lyrics_files: RefCell<Vec<ManagedDeviceFile>>,
    pub(super) cleaned_partials: RefCell<Vec<String>>,
    pub(super) ejected: RefCell<Vec<String>>,
    pub(super) planned_operations: RefCell<Vec<(String, &'static str)>>,
    pub(super) transcode_starts: RefCell<Vec<(PathBuf, PathBuf)>>,
    pub(super) copy_sources: RefCell<Vec<(PathBuf, String)>>,
    available_bytes: Cell<Option<u64>>,
    total_bytes: Cell<Option<u64>>,
    storage_access: Cell<DeviceStorageAccess>,
    transcode_probe_error: RefCell<Option<String>>,
    transcode_delay_ms: Cell<u64>,
    transcode_failures: RefCell<HashMap<PathBuf, String>>,
    transcode_observer: RefCell<Option<TranscodeObserver>>,
    transcode_output_override: RefCell<Option<PathBuf>>,
    cleanup_error: RefCell<Option<String>>,
    sidecar_replace_error: RefCell<Option<String>>,
    analysis_sidecar_replace_error: RefCell<Option<String>>,
    track_metadata_replace_error: RefCell<Option<String>>,
    copy_gate: RefCell<Option<CopyGate>>,
    playlist_error: RefCell<Option<String>>,
    playlist_gate: RefCell<Option<PlaylistGate>>,
    inspection_gate: RefCell<Option<InspectionGate>>,
    inspection_error: RefCell<Option<String>>,
    listen_report: RefCell<Option<Vec<u8>>>,
    listen_report_read_error: RefCell<Option<String>>,
    delete_observer: RefCell<Option<DeleteObserver>>,
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

    pub(super) fn with_transcode_delay(self, delay_ms: u64) -> Self {
        self.state.transcode_delay_ms.set(delay_ms);
        self
    }

    pub(super) fn fail_transcode_for(&self, source: PathBuf, error: &str) {
        self.state
            .transcode_failures
            .borrow_mut()
            .insert(source, error.into());
    }

    pub(super) fn observe_transcode_completion(&self, observer: TranscodeObserver) {
        self.state.transcode_observer.replace(Some(observer));
    }

    pub(super) fn return_transcode_from(&self, path: PathBuf) {
        self.state.transcode_output_override.replace(Some(path));
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

    pub(super) fn with_sidecar_replace_error(self, error: &str) -> Self {
        self.state.sidecar_replace_error.replace(Some(error.into()));
        self
    }

    pub(super) fn with_analysis_sidecar_replace_error(self, error: &str) -> Self {
        self.state
            .analysis_sidecar_replace_error
            .replace(Some(error.into()));
        self
    }

    pub(super) fn with_track_metadata_replace_error(self, error: &str) -> Self {
        self.state
            .track_metadata_replace_error
            .replace(Some(error.into()));
        self
    }

    pub(super) fn with_listen_report(self, bytes: Vec<u8>) -> Self {
        self.set_listen_report(bytes);
        self
    }

    pub(super) fn set_listen_report(&self, bytes: Vec<u8>) {
        self.state.listen_report.replace(Some(bytes));
        self.state.listen_report_read_error.replace(None);
    }

    pub(super) fn set_listen_report_read_error(&self, error: &str) {
        self.state
            .listen_report_read_error
            .replace(Some(error.into()));
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

    fn inspect(&self, root_uri: String, target: SyncTarget) -> TestFuture<DeviceStorageInspection> {
        let available_bytes = self.state.available_bytes.get();
        let total_bytes = self.state.total_bytes.get();
        let storage_access = self.state.storage_access.get();
        let gate = self.state.inspection_gate.borrow_mut().take();
        let inspection_error = self.state.inspection_error.borrow_mut().take();
        let managed_files = self.state.managed_files.borrow().clone();
        let partial_paths = self.state.partial_paths.borrow().clone();
        let lyrics_files = self.state.lyrics_files.borrow().clone();
        self.state
            .managed_root_enumerations
            .set(self.state.managed_root_enumerations.get().saturating_add(1));
        self.state.inspection_roots.borrow_mut().push(root_uri);
        self.state.last_inspected_target.replace(Some(target));
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
                managed_files,
                partial_paths,
                lyrics_files,
            })
        })
    }

    fn read_managed_file(
        &self,
        _root_uri: String,
        target_path: String,
        _storage_id: Option<StorageId>,
        relative_path: String,
    ) -> TestFuture<Option<Vec<u8>>> {
        self.state
            .managed_reads
            .borrow_mut()
            .push((target_path, relative_path));
        let error = self.state.listen_report_read_error.borrow().clone();
        let report = self.state.listen_report.borrow().clone();
        Box::pin(async move {
            match error {
                Some(error) => Err(error),
                None => Ok(report),
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        device_id: String,
        _root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> TestFuture<CopyOutcome> {
        let state = self.state.clone();
        let delay_ms = self.delay_ms;
        state
            .copy_sources
            .borrow_mut()
            .push((source_path.clone(), relative_target.clone()));
        let source_contents = std::fs::read(source_path).ok();
        let is_generated_metadata = matches!(
            relative_target.as_str(),
            reprise_core::device_sync::track_metadata_list::FILE_NAME
                | reprise_core::device_sync::listen_report::ACKNOWLEDGEMENT_FILE_NAME
        );
        let is_lyrics = reprise_core::device_sync::lyrics_sidecar::is_sidecar_path(
            std::path::Path::new(&relative_target),
        );
        let is_playlists_target = state
            .last_inspected_target
            .borrow()
            .as_ref()
            .is_some_and(|target| target.path == target_path);
        if !is_generated_metadata {
            state
                .transfer_storage_ids
                .borrow_mut()
                .push((target_path.clone(), storage_id));
        }
        Box::pin(async move {
            if relative_target == reprise_core::device_sync::track_metadata_list::FILE_NAME {
                if let Some(error) = state.track_metadata_replace_error.borrow().clone() {
                    return Err(error);
                }
            }
            if reprise_core::device_sync::lyrics_sidecar::is_sidecar_path(std::path::Path::new(
                &relative_target,
            )) {
                if let Some(error) = state.sidecar_replace_error.borrow().clone() {
                    return Err(error);
                }
            }
            if reprise_core::device_sync::analysis_sidecar::is_sidecar_path(std::path::Path::new(
                &relative_target,
            )) {
                if let Some(error) = state.analysis_sidecar_replace_error.borrow().clone() {
                    return Err(error);
                }
            }
            if is_generated_metadata {
                if let Some(contents) = source_contents {
                    state.managed_copy_contents.borrow_mut().push((
                        target_path,
                        relative_target.clone(),
                        contents,
                    ));
                }
                return Ok(CopyOutcome::Copied {
                    relative_path: relative_target,
                });
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
            if is_playlists_target {
                let files = if is_lyrics {
                    &state.lyrics_files
                } else {
                    &state.managed_files
                };
                let mut files = files.borrow_mut();
                if let Some(file) = files
                    .iter_mut()
                    .find(|file| file.relative_path == relative_target)
                {
                    file.size_bytes = expected_size;
                } else {
                    files.push(ManagedDeviceFile {
                        relative_path: relative_target.clone(),
                        size_bytes: expected_size,
                    });
                }
            }
            state
                .copy_order
                .borrow_mut()
                .push((device_id, relative_target.clone()));
            state
                .managed_copies
                .borrow_mut()
                .push((target_path.clone(), relative_target.clone()));
            if let Some(contents) = source_contents {
                state.managed_copy_contents.borrow_mut().push((
                    target_path,
                    relative_target.clone(),
                    contents,
                ));
            }
            Ok(CopyOutcome::Copied {
                relative_path: relative_target,
            })
        })
    }

    fn cleanup_partials(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        partial_paths: Vec<String>,
    ) -> BackendFuture<u32> {
        let error = self.state.cleanup_error.borrow().clone();
        let state = self.state.clone();
        Box::pin(async move {
            if let Some(error) = error {
                return Err(error);
            }
            let partial_paths = partial_paths
                .into_iter()
                .filter(|path| path.ends_with(".part"))
                .collect::<Vec<_>>();
            let removed = u32::try_from(partial_paths.len()).unwrap_or(u32::MAX);
            state
                .partial_paths
                .borrow_mut()
                .retain(|candidate| !partial_paths.iter().any(|removed| removed == candidate));
            state.cleaned_partials.borrow_mut().extend(partial_paths);
            Ok(removed)
        })
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
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> TestFuture<TranscodedFile> {
        let state = self.state.clone();
        Box::pin(async move {
            state
                .planned_operations
                .borrow_mut()
                .push(("fake".into(), "transcode"));
            state
                .transcode_starts
                .borrow_mut()
                .push((request.source.clone(), request.output.clone()));
            transcode_prefetch_tests::write_fake_output(&request.output)?;
            for _ in 0..state.transcode_delay_ms.get() {
                if cancelled.load(Ordering::SeqCst) {
                    return Err("cancelled".into());
                }
                gtk4::glib::timeout_future(Duration::from_millis(1)).await;
            }
            if let Some(error) = state
                .transcode_failures
                .borrow()
                .get(&request.source)
                .cloned()
            {
                return Err(error);
            }
            let output = state
                .transcode_output_override
                .borrow()
                .clone()
                .unwrap_or_else(|| request.output.clone());
            if output != request.output {
                transcode_prefetch_tests::write_fake_output(&output)?;
            }
            if let Some(observer) = state.transcode_observer.borrow().clone() {
                observer(&output);
            }
            Ok(TranscodedFile {
                path: output,
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

#[path = "device_sync_lyrics_gate_tests.rs"]
mod lyrics_gate_tests;
#[path = "device_sync_transcode_prefetch_tests.rs"]
mod transcode_prefetch_tests;

pub(super) fn descriptor(id: &str, reconnectable: bool) -> DeviceDescriptor {
    DeviceDescriptor {
        id: id.into(),
        persistent_id: reconnectable.then(|| id.to_string()),
        name: format!("Phone {id}"),
        root_uri: format!("mtp://{id}"),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        reconnectable,
    }
}
