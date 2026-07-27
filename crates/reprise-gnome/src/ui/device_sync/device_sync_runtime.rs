use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::settings::{load_or_create_settings, mark_device_playlists_synced};
use reprise_core::device_sync::{
    DeviceSelection, DeviceSettings, DeviceStorageInspection, DeviceStorageSnapshot,
    ManagedDeviceFile, MirrorPlan, SelectionSource, SyncPageState,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor, DeviceMonitor};
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};
use rusqlite::Connection;

#[path = "device_sync_rate.rs"]
pub(super) mod rate;
#[path = "device_sync_types.rs"]
mod types;

use rate::MtpRateMeter;
use types::StateCallback;
pub use types::*;

struct DeviceState {
    descriptor: DeviceDescriptor,
    connected: bool,
    generation: u64,
    cancellable: Option<gio::Cancellable>,
    storage: DeviceStorageSnapshot,
    managed_files: Vec<ManagedDeviceFile>,
    managed_track_count: usize,
    scanning: bool,
    scan_generation: u64,
    scan_error: Option<String>,
    settings: DeviceSettings,
    sync_phase: PlannedSyncPhase,
    sync_error: Option<SyncFailure>,
    planned_cancel: Option<Arc<AtomicBool>>,
    resume_planned: bool,
    last_sync: Option<chrono::DateTime<chrono::Utc>>,
    verified_managed_track_count: Option<usize>,
    mtp_rate: MtpRateMeter,
    mirror_plan: MirrorPlan,
    page: SyncPageState,
}

impl DeviceState {
    fn new(descriptor: DeviceDescriptor, settings: DeviceSettings) -> Self {
        Self {
            descriptor,
            connected: true,
            generation: 0,
            cancellable: None,
            storage: DeviceStorageSnapshot::default(),
            managed_files: Vec::new(),
            managed_track_count: 0,
            scanning: false,
            scan_generation: 0,
            scan_error: None,
            settings,
            sync_phase: PlannedSyncPhase::ComputingDelta,
            sync_error: None,
            planned_cancel: None,
            resume_planned: false,
            last_sync: None,
            verified_managed_track_count: None,
            mtp_rate: MtpRateMeter::default(),
            mirror_plan: MirrorPlan::default(),
            page: SyncPageState::default(),
        }
    }

    fn view(&self) -> DeviceView {
        let mut page = self.page.clone();
        page.update_controls(
            self.connected,
            !self.scanning
                && self.scan_error.is_none()
                && self.sync_phase != PlannedSyncPhase::ComputingDelta,
            self.is_active(),
        );
        if self.sync_phase == PlannedSyncPhase::Finishing {
            page.controls = reprise_core::device_sync::SyncPageControls::default();
        }
        DeviceView {
            id: self.descriptor.id.clone(),
            name: self.descriptor.name.clone(),
            icon: self.descriptor.icon.clone(),
            connected: self.connected,
            storage: self.storage.clone(),
            scan_error: self.scan_error.clone(),
            settings: self.settings.clone(),
            sync_phase: self.sync_phase.clone(),
            sync_error: self.sync_error.clone(),
            last_sync: self.last_sync,
            verified_managed_track_count: self.verified_managed_track_count,
            managed_track_count: self.managed_track_count,
            bytes_per_second: self.mtp_rate.bytes_per_second(),
            page,
        }
    }

    fn is_active(&self) -> bool {
        self.planned_cancel.is_some()
    }

    fn is_busy(&self) -> bool {
        self.is_active() || self.sync_phase == PlannedSyncPhase::Finishing
    }
}

pub struct DeviceSyncRuntime {
    conn: Rc<RefCell<Connection>>,
    backend: Rc<dyn DeviceBackend>,
    device_states: RefCell<Vec<DeviceState>>,
    subscribers: RefCell<HashMap<u64, StateCallback>>,
    next_subscription_id: Cell<u64>,
    weak_self: RefCell<Weak<Self>>,
    agent_subscription: RefCell<Option<Subscription>>,
}

impl DeviceSyncRuntime {
    pub fn new(conn: &Rc<RefCell<Connection>>, monitor: DeviceMonitor) -> Rc<Self> {
        Self::with_backend(
            conn,
            Rc::new(super::device_sync_backend::GioDeviceBackend::new(monitor)),
        )
    }

    pub fn with_backend(
        conn: &Rc<RefCell<Connection>>,
        backend: Rc<dyn DeviceBackend>,
    ) -> Rc<Self> {
        let runtime = Rc::new(Self {
            conn: conn.clone(),
            backend,
            device_states: RefCell::new(Vec::new()),
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
        let mut devices = self
            .device_states
            .borrow()
            .iter()
            .map(DeviceState::view)
            .collect::<Vec<_>>();
        devices.sort_by(|left, right| left.name.cmp(&right.name));
        devices
    }

    pub fn cancel_current(self: &Rc<Self>, device_id: &str) {
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            if let Some(cancelled) = &device.planned_cancel {
                cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            if let Some(cancellable) = &device.cancellable {
                cancellable.cancel();
            }
        }
        self.notify();
    }

    pub fn refresh_contents(self: &Rc<Self>, device_id: &str) {
        self.refresh_contents_with_delta(device_id, true, RefreshPurpose::Normal);
    }

    fn refresh_contents_after_sync(
        self: &Rc<Self>,
        device_id: &str,
        sources: Vec<SelectionSource>,
    ) {
        self.refresh_contents_with_delta(device_id, true, RefreshPurpose::VerifySync(sources));
    }

    fn refresh_contents_with_delta(
        self: &Rc<Self>,
        device_id: &str,
        recompute_delta: bool,
        purpose: RefreshPurpose,
    ) {
        let request = {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return;
            };
            if !device.connected {
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
            let result = backend.inspect(root_uri).await;
            let Some(runtime) = weak.upgrade() else {
                return;
            };
            let verified_track_count = result
                .as_ref()
                .ok()
                .map(|inspection| inspection.managed_files.len());
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
                        if let Err(error) = mark_device_playlists_synced(
                            &runtime.conn.borrow(),
                            &id,
                            sources,
                            verified_at.timestamp(),
                        ) {
                            playlist_timestamp_error = Some(format!(
                                "could not record verified playlist synchronization: {error}"
                            ));
                            None
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
                let should_resume = {
                    let mut devices = runtime.device_states.borrow_mut();
                    devices
                        .iter_mut()
                        .find(|device| device.descriptor.id == id)
                        .is_some_and(|device| {
                            let resume = device.resume_planned
                                && device.connected
                                && !device.is_active();
                            if resume {
                                device.resume_planned = false;
                            }
                            resume
                        })
                };
                if should_resume {
                    if let Err(error) = runtime.sync_now(&id) {
                        tracing::warn!(device_id = id, %error, "could not resume device synchronization");
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

    fn apply_devices(self: &Rc<Self>, descriptors: Vec<DeviceDescriptor>) {
        let incoming = descriptors
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect::<HashMap<_, _>>();
        let mut refresh = Vec::new();
        {
            let mut states = self.device_states.borrow_mut();
            for state in states.iter_mut() {
                if incoming.contains_key(&state.descriptor.id) || !state.connected {
                    continue;
                }
                state.connected = false;
                state.scanning = false;
                state.scan_generation = state.scan_generation.saturating_add(1);
                if let Some(cancelled) = &state.planned_cancel {
                    cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    state.resume_planned = state.descriptor.reconnectable;
                    if let Some(cancellable) = &state.cancellable {
                        cancellable.cancel();
                    }
                }
            }
            states.retain(|state| {
                incoming.contains_key(&state.descriptor.id)
                    || state.planned_cancel.is_some()
                    || state.resume_planned
            });
            for (id, descriptor) in incoming {
                if let Some(state) = states.iter_mut().find(|state| state.descriptor.id == id) {
                    let was_connected = state.connected;
                    state.descriptor = descriptor;
                    state.connected = true;
                    if !was_connected {
                        refresh.push(id.clone());
                    }
                } else {
                    let settings = load_or_create_settings(
                        &self.conn.borrow(),
                        &descriptor.id,
                        &descriptor.name,
                    )
                    .unwrap_or_else(|error| {
                        tracing::warn!(device_id = descriptor.id, %error, "could not load device settings");
                        DeviceSettings {
                            device_serial: descriptor.id.clone(),
                            device_name: descriptor.name.clone(),
                            selection: DeviceSelection::default(),
                            profile: reprise_core::device_sync::TransferProfile::default(),
                            opus_bitrate: 0,
                            ratings_back: false,
                            remove_deleted: true,
                        }
                    });
                    states.push(DeviceState::new(descriptor, settings));
                    refresh.push(id);
                }
            }
        }
        self.notify();
        for id in refresh {
            self.refresh_contents(&id);
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum RefreshPurpose {
    Normal,
    VerifySync(Vec<SelectionSource>),
}

#[path = "device_sync_agent.rs"]
mod agent;
#[path = "device_sync_compact.rs"]
mod compact;
#[path = "device_sync_planned.rs"]
mod planned;

#[cfg(test)]
pub(super) use planned::SyncStartError;
