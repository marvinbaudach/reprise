use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::db::Db;
use reprise_core::device_sync::browser::StorageOption;
use reprise_core::device_sync::device_view::{
    project_category_content_row, project_contents_state, project_device_music_reading,
};
use reprise_core::device_sync::settings::{forget_device, save_settings};
use reprise_core::device_sync::sync_log;
use reprise_core::device_sync::{
    DeviceSelection, DeviceSessionState, DeviceSettings, DeviceStorageInspection,
    DeviceStorageSnapshot, ManagedDeviceFile, MirrorPlan, MusicDiff, MusicReading, SelectionSource,
    StorageId, SyncPageState, SyncTarget,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor, DeviceMonitor};
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};

use crate::ui::device_sync_strings;

use super::device_sync_remembered;

#[path = "device_sync_rate.rs"]
pub(super) mod rate;
#[path = "device_sync_runtime_refresh.rs"]
mod refresh;
#[path = "device_sync_types.rs"]
mod types;

use rate::MtpRateMeter;
pub use types::*;

struct DeviceState {
    descriptor: DeviceDescriptor,
    connected: bool,
    session_state: DeviceSessionState,
    /// The run that currently owns this device, if any. Identity of this
    /// handle is what tells a superseded run to stop writing here.
    machine: Option<Rc<RefCell<reprise_core::device_sync::DeviceSyncMachine>>>,
    cancellable: Option<gio::Cancellable>,
    storage: DeviceStorageSnapshot,
    managed_files: Vec<ManagedDeviceFile>,
    partial_paths: Vec<String>,
    lyrics_files: Vec<ManagedDeviceFile>,
    managed_track_count: usize,
    scanning: bool,
    scan_generation: u64,
    scan_error: Option<String>,
    /// `MTP-26`: whether `backend.inspect` has ever completed successfully
    /// for this device this session. Session-only by design — the
    /// inventory itself is rebuilt live from MTP on every connect, so
    /// "verified" cannot outlive the connection it was verified on.
    ever_inspected: bool,
    target: SyncTarget,
    settings: DeviceSettings,
    sync_phase: PlannedSyncPhase,
    sync_error: Option<SyncFailure>,
    planned_cancel: Option<Arc<AtomicBool>>,
    active_initiator: Option<planned::SyncInitiator>,
    resume_initiator: Option<planned::SyncInitiator>,
    last_sync: Option<chrono::DateTime<chrono::Utc>>,
    verified_managed_track_count: Option<usize>,
    last_verified_size_bytes: Option<u64>,
    size_on_device_bytes: Option<u64>,
    mtp_rate: MtpRateMeter,
    mirror_plan: MirrorPlan,
    keep_smart_playlists_updated: bool,
    library_dirty: bool,
    page: SyncPageState,
}

impl DeviceState {
    fn new(
        descriptor: DeviceDescriptor,
        settings: DeviceSettings,
        target: SyncTarget,
        session_state: DeviceSessionState,
    ) -> Self {
        let sync_phase = if session_state.opens_session() {
            PlannedSyncPhase::ComputingDelta
        } else {
            PlannedSyncPhase::Idle
        };
        Self {
            descriptor,
            connected: true,
            session_state,
            machine: None,
            cancellable: None,
            storage: DeviceStorageSnapshot::default(),
            managed_files: Vec::new(),
            partial_paths: Vec::new(),
            lyrics_files: Vec::new(),
            managed_track_count: 0,
            scanning: false,
            scan_generation: 0,
            scan_error: None,
            ever_inspected: false,
            target,
            settings,
            sync_phase,
            sync_error: None,
            planned_cancel: None,
            active_initiator: None,
            resume_initiator: None,
            last_sync: None,
            verified_managed_track_count: None,
            last_verified_size_bytes: None,
            size_on_device_bytes: None,
            mtp_rate: MtpRateMeter::default(),
            mirror_plan: MirrorPlan::default(),
            keep_smart_playlists_updated: true,
            library_dirty: false,
            page: SyncPageState::default(),
        }
    }

    fn remembered(memory: memory::RememberedDeviceMemory) -> Self {
        let mut state = Self::new(
            memory.descriptor,
            memory.settings,
            memory.target,
            DeviceSessionState::Remembered,
        );
        state.connected = false;
        state.sync_phase = PlannedSyncPhase::Idle;
        state.last_sync = memory
            .last_verified_at
            .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0));
        state.last_verified_size_bytes = memory.size_on_device_bytes;
        state
    }

    fn view(&self) -> DeviceView {
        let (units_done, units_total) = match self.sync_phase {
            PlannedSyncPhase::Syncing { done, total, .. } => (done, total),
            PlannedSyncPhase::Finishing => self
                .machine
                .as_ref()
                .map(|machine| {
                    let machine = machine.borrow();
                    (machine.units_done(), machine.units_total())
                })
                .unwrap_or_default(),
            _ => (0, 0),
        };
        let (bytes_done, bytes_total) = self
            .machine
            .as_ref()
            .map(|machine| {
                let machine = machine.borrow();
                (machine.bytes_done(), machine.bytes_total())
            })
            .unwrap_or_default();
        let mut page = self.page.clone();
        page.update_controls(
            self.connected && self.session_state.opens_session(),
            !self.scanning
                && self.scan_error.is_none()
                && self.sync_phase != PlannedSyncPhase::ComputingDelta,
            self.is_active(),
        );
        if self.sync_phase == PlannedSyncPhase::Finishing {
            page.controls = reprise_core::device_sync::SyncPageControls::default();
        }
        let target_reading = if self.session_state.shows_diff() {
            self.target_reading()
        } else {
            project_device_music_reading(MusicDiff::default())
        };
        let content_row = if self.session_state.shows_diff() {
            project_category_content_row(
                &self.target,
                self.managed_files.len(),
                compact::verified_track_bytes(&self.managed_files),
            )
        } else {
            project_category_content_row(
                &self.target,
                self.managed_track_count,
                self.size_on_device_bytes.unwrap_or(0),
            )
        };
        DeviceView {
            id: self.descriptor.id.clone(),
            name: self.settings.device_name.clone(),
            icon: self.descriptor.icon.clone(),
            connected: self.connected,
            rememberable: self.descriptor.persistent_id.is_some(),
            memory_status: (self.connected && self.descriptor.persistent_id.is_none())
                .then(device_sync_strings::unrememberable_device_status),
            session_state: self.session_state.clone(),
            storage: self.storage.clone(),
            storage_measured: self.ever_inspected,
            scan_error: self.scan_error.clone(),
            settings: self.settings.clone(),
            sync_phase: self.sync_phase.clone(),
            sync_error: self.sync_error.clone(),
            last_sync: self.last_sync,
            verified_managed_track_count: self.verified_managed_track_count,
            size_on_device_bytes: self.size_on_device_bytes,
            managed_track_count: self.managed_track_count,
            bytes_done,
            bytes_total,
            bytes_per_second: self.mtp_rate.bytes_per_second(),
            units_done,
            units_total,
            estimated_remaining: self.mtp_rate.remaining(units_done, units_total),
            page,
            contents_state: project_contents_state(
                self.scanning,
                self.scan_error.as_deref(),
                self.ever_inspected,
                self.last_sync,
            ),
            content_row,
            target_reading,
            keep_smart_playlists_updated: self.keep_smart_playlists_updated,
        }
    }

    fn is_active(&self) -> bool {
        self.machine.is_some()
    }

    fn is_busy(&self) -> bool {
        self.is_active() || self.sync_phase == PlannedSyncPhase::Finishing
    }

    fn target_reading(&self) -> MusicReading {
        project_device_music_reading(MusicDiff::from_mirror_plan(&self.mirror_plan))
    }
}

pub struct DeviceSyncRuntime {
    conn: Rc<Db>,
    backend: Rc<dyn DeviceBackend>,
    device_states: RefCell<Vec<DeviceState>>,
    subscribers: RefCell<HashMap<u64, StateCallback>>,
    next_subscription_id: Cell<u64>,
    weak_self: RefCell<Weak<Self>>,
    agent_subscription: RefCell<Option<Subscription>>,
}

impl DeviceSyncRuntime {
    pub fn new(conn: &Rc<Db>, monitor: DeviceMonitor) -> Rc<Self> {
        Self::with_backend(
            conn,
            Rc::new(super::device_sync_backend::GioDeviceBackend::new(monitor)),
        )
    }

    /// Reprojects library playlists for every idle device.
    ///
    /// Playlist CRUD and membership changes are local database work, so this
    /// deliberately uses the non-toasting projection path and not a device
    /// contents scan. An active sync owns its plan until completion and must
    /// not be disturbed by a concurrent library edit.
    pub(in crate::ui) fn library_playlists_changed(self: &Rc<Self>) {
        let available_sources = match reprise_core::device_sync::load_mirror_playlist_snapshots(
            &self.conn,
        ) {
            Ok(playlists) => playlists
                .into_iter()
                .map(|playlist| playlist.source)
                .collect::<HashSet<_>>(),
            Err(error) => {
                tracing::warn!(%error, "could not load playlists after a library playlist change");
                return;
            }
        };
        let devices = self
            .device_states
            .borrow()
            .iter()
            .filter(|device| !device.is_busy())
            .map(|device| {
                (
                    device.descriptor.id.clone(),
                    device.descriptor.persistent_id.is_some(),
                    device.settings.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut recomputed = false;
        for (device_id, rememberable, mut settings) in devices {
            let selection_changed = match &mut settings.selection {
                DeviceSelection::Sources(sources) => {
                    let before = sources.len();
                    sources.retain(|source| available_sources.contains(source));
                    sources.len() != before
                }
                DeviceSelection::EntireLibrary => false,
            };
            if selection_changed && rememberable {
                if let Err(error) = save_settings(&self.conn, &settings) {
                    tracing::warn!(
                        %error,
                        device_id,
                        "could not persist device selection after a playlist deletion"
                    );
                    continue;
                }
            }
            if selection_changed {
                let mut device_states = self.device_states.borrow_mut();
                let Some(device) = device_states
                    .iter_mut()
                    .find(|device| device.descriptor.id == device_id && !device.is_busy())
                else {
                    continue;
                };
                device.settings = settings;
            }
            match self.recompute_delta_silent(&device_id) {
                Ok(()) => recomputed = true,
                Err(error) => tracing::warn!(
                    %error,
                    device_id,
                    "could not refresh device playlists after a library playlist change"
                ),
            }
        }
        if recomputed {
            self.notify();
        }
    }

    pub fn with_backend(conn: &Rc<Db>, backend: Rc<dyn DeviceBackend>) -> Rc<Self> {
        match sync_log::close_orphaned_runs(conn) {
            Ok(count) => tracing::debug!(count, "closed orphaned device sync runs"),
            Err(error) => tracing::warn!(%error, "could not close orphaned device sync runs"),
        }
        let remembered = memory::load_remembered_device_memories(conn)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not load remembered device history");
                Vec::new()
            })
            .into_iter()
            .map(DeviceState::remembered)
            .collect();
        let runtime = Rc::new(Self {
            conn: conn.clone(),
            backend,
            device_states: RefCell::new(remembered),
            subscribers: RefCell::new(HashMap::new()),
            next_subscription_id: Cell::new(1),
            weak_self: RefCell::new(Weak::new()),
            agent_subscription: RefCell::new(None),
        });
        runtime.weak_self.replace(Rc::downgrade(&runtime));
        runtime.apply_devices(runtime.backend.devices());
        let weak = Rc::downgrade(&runtime);
        runtime.backend.subscribe_devices(Rc::new(move |devices| {
            if let Some(runtime) = weak.upgrade() {
                runtime.apply_devices(devices);
            }
        }));
        runtime
    }

    pub fn devices(&self) -> Vec<DeviceView> {
        let devices = self
            .device_states
            .borrow()
            .iter()
            .filter(|state| state.connected || state.descriptor.persistent_id.is_some())
            .map(DeviceState::view)
            .collect::<Vec<_>>();
        devices
    }

    pub(in crate::ui) fn mark_all_devices_stale(&self) {
        for device in self.device_states.borrow_mut().iter_mut() {
            device.library_dirty = true;
        }
    }

    pub(in crate::ui) fn recompute_if_stale(
        self: &Rc<Self>,
        device_id: &str,
    ) -> Result<(), String> {
        {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return Ok(());
            };
            if !device.library_dirty {
                return Ok(());
            }
            if !device_sync_remembered::page_is_readable(device.connected, &device.session_state) {
                let device_name = device.settings.device_name.clone();
                let session_state = device.session_state.clone();
                drop(devices);
                tracing::debug!(
                    %device_id,
                    %device_name,
                    ?session_state,
                    "skipping stale device refresh because its page is neither connected nor remembered"
                );
                return Ok(());
            }
            device.library_dirty = false;
        }
        let result = self.recompute_delta(device_id);
        if result.is_err() {
            if let Some(device) = self
                .device_states
                .borrow_mut()
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            {
                device.library_dirty = true;
            }
        }
        result
    }

    pub fn cancel_current(self: &Rc<Self>, device_id: &str) {
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            cancel_device_run(device);
        }
        self.notify();
    }

    pub fn forget_remembered_device(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let can_forget = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .is_some_and(|device| {
                !device.connected && device.session_state == DeviceSessionState::Remembered
            });
        if !can_forget {
            return Err("disconnect the device before forgetting it".into());
        }
        forget_device(&self.conn, device_id).map_err(|error| error.to_string())?;
        self.device_states
            .borrow_mut()
            .retain(|device| device.descriptor.id != device_id);
        self.notify();
        Ok(())
    }

    pub fn refresh_contents(self: &Rc<Self>, device_id: &str) {
        self.refresh_contents_with_delta(device_id, true, RefreshPurpose::Normal, false);
    }

    pub fn dismiss_legacy_media_notice(&self, device_id: &str) {
        let rememberable = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .is_some_and(|device| device.descriptor.persistent_id.is_some());
        if let Err(error) = reprise_core::device_sync::settings::dismiss_legacy_media_notice(
            &self.conn,
            device_id,
            rememberable,
        ) {
            tracing::warn!(%error, "could not dismiss the retired media-sync notice");
            return;
        }
        self.notify();
    }

    pub fn legacy_media_notice_pending(&self, device_id: &str) -> bool {
        match reprise_core::device_sync::settings::legacy_media_notice_pending(
            &self.conn, device_id,
        ) {
            Ok(pending) => pending,
            Err(error) => {
                tracing::warn!(%error, "could not read the retired media-sync notice");
                false
            }
        }
    }

    pub fn subscribe(self: &Rc<Self>, callback: StateCallback) -> Subscription {
        callback(DeviceSyncState {
            devices: self.devices(),
        });
        let id = self.next_subscription_id.get();
        self.next_subscription_id.set(id.saturating_add(1));
        self.subscribers.borrow_mut().insert(id, callback);
        let weak = Rc::downgrade(self);
        Subscription {
            cancel: RefCell::new(Some(Box::new(move || {
                if let Some(runtime) = weak.upgrade() {
                    runtime.subscribers.borrow_mut().remove(&id);
                }
            }))),
        }
    }

    fn notify(&self) {
        let state = DeviceSyncState {
            devices: self.devices(),
        };
        let callbacks = self
            .subscribers
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for callback in callbacks {
            callback(state.clone());
        }
    }
}

/// Stops a device's run in all three places it can be interrupted: the
/// reducer, the GIO copy in flight, and the transcoder thread.
fn cancel_device_run(device: &mut DeviceState) {
    if let Some(machine) = &device.machine {
        machine
            .borrow_mut()
            .dispatch(reprise_core::device_sync::Event::Cancel);
    }
    if let Some(cancelled) = &device.planned_cancel {
        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(cancellable) = &device.cancellable {
        cancellable.cancel();
    }
}

#[path = "device_sync_agent.rs"]
mod agent;
#[path = "device_sync_compact.rs"]
mod compact;
#[path = "device_sync_device_list.rs"]
mod device_list;
#[path = "device_sync_memory.rs"]
mod memory;
#[path = "device_sync_naming.rs"]
mod naming;
#[path = "device_sync_picker_runtime.rs"]
mod picker;
#[path = "device_sync_planned.rs"]
mod planned;
#[path = "device_sync_target_actions.rs"]
mod target_actions;

pub(super) use picker::*;
#[cfg(test)]
pub(super) use planned::{record_rejected_start, RunLog, SyncInitiator, SyncStartError};
