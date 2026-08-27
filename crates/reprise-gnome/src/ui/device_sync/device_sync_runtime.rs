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
use reprise_core::device_sync::settings::{
    forget_device, mark_device_playlists_synced, record_device_verification, save_settings,
};
use reprise_core::device_sync::sync_log;
use reprise_core::device_sync::{
    aggregate_balance, should_auto_start, AutoStartFacts, DeviceSelection, DeviceSessionState,
    DeviceSettings, DeviceStorageInspection, DeviceStorageSnapshot, ManagedDeviceFile, MirrorPlan,
    MusicDiff, MusicReading, SelectionSource, StorageId, SyncPageState, SyncTarget,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor, DeviceMonitor};
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};

use crate::ui::device_sync_strings;

#[path = "device_sync_rate.rs"]
pub(super) mod rate;
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
            scan_error: self.scan_error.clone(),
            settings: self.settings.clone(),
            sync_phase: self.sync_phase.clone(),
            sync_error: self.sync_error.clone(),
            last_sync: self.last_sync,
            verified_managed_track_count: self.verified_managed_track_count,
            size_on_device_bytes: self.size_on_device_bytes,
            managed_track_count: self.managed_track_count,
            bytes_per_second: self.mtp_rate.bytes_per_second(),
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

    /// Same as [`Self::refresh_contents`], except this refresh is the first
    /// one after the device connected (`apply_devices`, both a brand-new
    /// device and a reconnect) — the only refresh `MTP-30`'s auto-start
    /// decision is allowed to fire from. A manual "Refresh" click or the
    /// post-sync verify refresh must never re-trigger it.
    fn refresh_contents_on_connect(self: &Rc<Self>, device_id: &str) {
        self.refresh_contents_with_delta(device_id, true, RefreshPurpose::Normal, true);
    }

    fn refresh_contents_after_sync(
        self: &Rc<Self>,
        device_id: &str,
        sources: Vec<SelectionSource>,
    ) {
        self.refresh_contents_with_delta(
            device_id,
            true,
            RefreshPurpose::VerifySync(sources),
            false,
        );
    }

    fn refresh_contents_with_delta(
        self: &Rc<Self>,
        device_id: &str,
        recompute_delta: bool,
        purpose: RefreshPurpose,
        just_connected: bool,
    ) {
        let target = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.target.clone());
        let Some(target) = target else {
            return;
        };
        let request = {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return;
            };
            if !device.connected || !device.session_state.opens_session() {
                return;
            }
            device.scan_generation = device.scan_generation.saturating_add(1);
            device.scanning = true;
            device.scan_error = None;
            Some((device.descriptor.root_uri.clone(), device.scan_generation))
        };
        self.notify();
        let Some((root_uri, generation)) = request else {
            return;
        };
        let backend = self.backend.clone();
        let weak = self.weak_self.borrow().clone();
        let id = device_id.to_string();
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            let result = backend.inspect(root_uri, target).await;
            let Some(runtime) = weak.upgrade() else {
                return;
            };
            let verified_track_count = result
                .as_ref()
                .ok()
                .map(|inspection| {
                    inspection
                        .managed_files
                        .iter()
                        .filter(|file| compact::is_verified_track_file(file))
                        .count()
                });
            let inspection_error = result.as_ref().err().cloned();
            {
                let mut devices = runtime.device_states.borrow_mut();
                if let Some(device) = devices.iter_mut().find(|device| device.descriptor.id == id) {
                    if device.scan_generation != generation {
                        return;
                    }
                    device.scanning = false;
                    match result {
                        Ok(DeviceStorageInspection {
                            snapshot,
                            managed_files,
                        }) => {
                            device.storage = snapshot;
                            device.managed_files = managed_files;
                            device.scan_error = None;
                            device.ever_inspected = true;
                        }
                        Err(error) => device.scan_error = Some(error),
                    }
                }
            }
            if !recompute_delta {
                runtime.notify();
            } else {
                let planning_error = runtime.recompute_delta_silent(&id).err();
                let mut playlist_timestamp_error = None;
                let verified_at = match &purpose {
                    RefreshPurpose::VerifySync(sources)
                        if inspection_error.is_none() && planning_error.is_none() =>
                    {
                        let verified_at = chrono::Utc::now();
                        let rememberable = runtime
                            .device_states
                            .borrow()
                            .iter()
                            .find(|device| device.descriptor.id == id)
                            .is_some_and(|device| device.descriptor.persistent_id.is_some());
                        if rememberable {
                            let size_on_device = runtime
                                .device_states
                                .borrow()
                                .iter()
                                .find(|device| device.descriptor.id == id)
                                .map_or(0, |device| {
                                    compact::verified_track_bytes(&device.managed_files)
                                });
                            if let Err(error) = mark_device_playlists_synced(
                                &runtime.conn,
                                &id,
                                sources,
                                verified_at.timestamp(),
                            ) {
                                playlist_timestamp_error = Some(format!(
                                    "could not record verified playlist synchronization: {error}"
                                ));
                                None
                            } else if let Err(error) = record_device_verification(
                                &runtime.conn,
                                &id,
                                verified_at.timestamp(),
                                size_on_device,
                            ) {
                                playlist_timestamp_error = Some(format!(
                                    "could not remember verified device state: {error}"
                                ));
                                None
                            } else {
                                Some(verified_at)
                            }
                        } else {
                            Some(verified_at)
                        }
                    }
                    _ => None,
                };
                if let Some(device) = runtime
                    .device_states
                    .borrow_mut()
                    .iter_mut()
                    .find(|device| device.descriptor.id == id)
                {
                    match &purpose {
                        RefreshPurpose::VerifySync(sources)
                            if inspection_error.is_none()
                                && planning_error.is_none()
                                && playlist_timestamp_error.is_none() =>
                        {
                            if let Some(verified_at) = verified_at {
                                for row in &mut device.page.playlists {
                                    if sources.contains(&row.source) {
                                        row.last_synced_at = Some(verified_at.timestamp());
                                    }
                                }
                                device.last_sync = Some(verified_at);
                                device.verified_managed_track_count = verified_track_count;
                                let verified_size =
                                    compact::verified_track_bytes(&device.managed_files);
                                device.last_verified_size_bytes = Some(verified_size);
                                device.size_on_device_bytes = Some(verified_size);
                                device.sync_error = None;
                            }
                        }
                        RefreshPurpose::VerifySync(_) => {
                            device.sync_phase = PlannedSyncPhase::Idle;
                            device.sync_error = Some(SyncFailure {
                                message: inspection_error.clone().map_or_else(
                                    || {
                                        planning_error.clone().unwrap_or_else(|| {
                                            playlist_timestamp_error.clone().unwrap_or_else(|| {
                                                "device content verification failed".into()
                                            })
                                        })
                                    },
                                    |error| {
                                        format!(
                                            "could not verify device contents after synchronization: {error}"
                                        )
                                    },
                                ),
                                failed_tracks: Vec::new(),
                            });
                        }
                        RefreshPurpose::Normal => {
                            if let Some(error) = planning_error.clone() {
                                device.sync_phase = PlannedSyncPhase::Idle;
                                device.sync_error = Some(SyncFailure {
                                    message: error,
                                    failed_tracks: Vec::new(),
                                });
                            }
                        }
                    }
                }
                runtime.notify();
                let resume_initiator = {
                    let mut devices = runtime.device_states.borrow_mut();
                    devices
                        .iter_mut()
                        .find(|device| device.descriptor.id == id)
                        .and_then(|device| {
                            let resume = device.resume_initiator.is_some()
                                && device.connected
                                && !device.is_active();
                            if resume {
                                device.resume_initiator.take()
                            } else {
                                None
                            }
                        })
                };
                if let Some(initiator) = resume_initiator {
                    if let Err(error) = runtime.start_sync(&id, initiator) {
                        tracing::warn!(device_id = id, %error, "could not resume device synchronization");
                    }
                } else if just_connected {
                    // `MTP-30`: gather every fact from one short borrow, drop
                    // it, and only then decide — same discipline as
                    // `should_resume` above. A refused or failed automatic
                    // start is silent apart from this log: the user did not
                    // press anything, so it must never raise a modal or an
                    // error banner.
                    let facts = {
                        let devices = runtime.device_states.borrow();
                        devices
                            .iter()
                            .find(|device| device.descriptor.id == id)
                            .map(|device| AutoStartFacts {
                                just_connected,
                                sync_automatically: device.settings.sync_automatically,
                                scan_ok: device.scan_error.is_none(),
                                planning_ok: planning_error.is_none(),
                                device_connected: device.connected,
                                device_busy: device.is_busy(),
                                balance: aggregate_balance(&[device.target_reading()]),
                            })
                    };
                    if facts.is_some_and(should_auto_start) {
                        if let Err(error) = runtime.sync_automatically(&id) {
                            tracing::warn!(device_id = id, %error, "could not start automatic device synchronization");
                        }
                    }
                }
            }
        });
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
