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
use reprise_core::device_sync::settings::{
    load_device_files, load_or_create_settings, resolve_selection_track_ids, save_settings,
    set_file_pinned,
};
use reprise_core::device_sync::transfer::{build_transfer_plan, TransferMode, TransferPlanEntry};
use reprise_core::device_sync::{
    compute_delta, merge_playlist_entries, track_relative_path, DeviceQueue, DeviceSelection,
    DeviceSettings, SyncCandidate, SyncDelta, SyncJob, SyncPhase, SyncSnapshot, SyncTrack,
    TrackOutcome,
};
use reprise_core::library::m3u::{M3uEntry, M3uExportEntry};
use reprise_platform_linux::device_sync::{
    CopyOutcome, DeviceContents, DeviceDescriptor, DeviceMonitor,
};
use reprise_platform_linux::device_transfer::{EncodeOutcome, EncodeRequest, ReadyFile};
use rusqlite::Connection;

#[path = "device_sync_types.rs"]
mod types;

use types::StateCallback;
pub use types::*;
#[derive(Clone)]
struct Work {
    job: SyncJob,
    next_track: usize,
    appended: Vec<M3uExportEntry>,
}

struct DeviceState {
    descriptor: DeviceDescriptor,
    connected: bool,
    queue: DeviceQueue,
    running: bool,
    generation: u64,
    cancellable: Option<gio::Cancellable>,
    interrupted_disconnect: bool,
    paused_work: Option<Work>,
    contents: DeviceContents,
    available_bytes: Option<u64>,
    scanning: bool,
    scan_generation: u64,
    scan_error: Option<String>,
    draft_playlists: Vec<String>,
    last_enqueue: Option<EnqueueReceipt>,
    reserved_bytes: u64,
    settings: DeviceSettings,
    delta: Option<SyncDelta>,
    transfer_plan: Vec<TransferPlanEntry>,
    sync_phase: PlannedSyncPhase,
    sync_error: Option<SyncFailure>,
    planned_cancel: Option<Arc<AtomicBool>>,
    resume_planned: bool,
    last_sync: Option<chrono::DateTime<chrono::Utc>>,
    tracks: Vec<DeviceTrackView>,
    selected_track_count: usize,
}

impl DeviceState {
    fn new(descriptor: DeviceDescriptor, settings: DeviceSettings) -> Self {
        Self {
            descriptor,
            connected: true,
            queue: DeviceQueue::new(),
            running: false,
            generation: 0,
            cancellable: None,
            interrupted_disconnect: false,
            paused_work: None,
            contents: DeviceContents::default(),
            available_bytes: None,
            scanning: false,
            scan_generation: 0,
            scan_error: None,
            draft_playlists: Vec::new(),
            last_enqueue: None,
            reserved_bytes: 0,
            settings,
            delta: None,
            transfer_plan: Vec::new(),
            sync_phase: PlannedSyncPhase::ComputingDelta,
            sync_error: None,
            planned_cancel: None,
            resume_planned: false,
            last_sync: None,
            tracks: Vec::new(),
            selected_track_count: 0,
        }
    }

    fn view(&self) -> DeviceView {
        DeviceView {
            id: self.descriptor.id.clone(),
            name: self.descriptor.name.clone(),
            icon: self.descriptor.icon.clone(),
            connected: self.connected,
            available_bytes: self.available_bytes,
            contents: self.contents.clone(),
            scanning: self.scanning,
            scan_error: self.scan_error.clone(),
            draft_playlists: self.draft_playlists.clone(),
            last_enqueue: self.last_enqueue.clone(),
            snapshot: self.queue.snapshot(),
            settings: self.settings.clone(),
            delta: self.delta.clone(),
            sync_phase: self.sync_phase.clone(),
            sync_error: self.sync_error.clone(),
            last_sync: self.last_sync,
            tracks: self.tracks.clone(),
            selected_track_count: self.selected_track_count,
        }
    }
}

pub struct DeviceSyncRuntime {
    conn: Rc<RefCell<Connection>>,
    backend: Rc<dyn DeviceBackend>,
    device_states: RefCell<Vec<DeviceState>>,
    subscribers: RefCell<HashMap<u64, StateCallback>>,
    next_subscription_id: Cell<u64>,
    next_job_id: Cell<u64>,
    active_device: RefCell<Option<String>>,
    weak_self: RefCell<Weak<Self>>,
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
            next_job_id: Cell::new(1),
            active_device: RefCell::new(None),
            weak_self: RefCell::new(Weak::new()),
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

    pub fn enqueue(
        self: &Rc<Self>,
        device_id: &str,
        playlist: &str,
        ids: &[i64],
    ) -> Result<usize, EnqueueError> {
        let status = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| (device.connected, device.planned_cancel.is_some()));
        match status {
            Some((true, false)) => {}
            Some((true, true)) => return Err(EnqueueError::Busy),
            Some((false, _)) | None => return Err(EnqueueError::UnknownDevice),
        }
        let tracks = reprise_core::queries::query_sync_tracks(&self.conn.borrow(), ids)
            .map_err(|error| EnqueueError::Database(error.to_string()))?;
        if tracks.is_empty() {
            return Err(EnqueueError::NoUsableTracks);
        }
        let count = tracks.len();
        let required_bytes = tracks
            .iter()
            .fold(0_u64, |total, track| total.saturating_add(track.size_bytes));
        let job_id = self.next_job_id.get();
        let mut devices = self.device_states.borrow_mut();
        let device = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
            .ok_or(EnqueueError::UnknownDevice)?;
        if let Some(available_bytes) = device.available_bytes {
            let unreserved_bytes = available_bytes.saturating_sub(device.reserved_bytes);
            if required_bytes > unreserved_bytes {
                return Err(EnqueueError::InsufficientSpace {
                    required_bytes,
                    available_bytes: unreserved_bytes,
                });
            }
        }
        self.next_job_id.set(job_id.saturating_add(1));
        let playlist = reprise_core::device_sync::safe_component(playlist, "Playlist");
        device.reserved_bytes = device.reserved_bytes.saturating_add(required_bytes);
        device.queue.enqueue(SyncJob {
            id: job_id,
            playlist: playlist.clone(),
            tracks,
        });
        let snapshot = device.queue.snapshot();
        device.last_enqueue = Some(EnqueueReceipt {
            playlist,
            track_count: count,
            queue_position: snapshot.queued_jobs + usize::from(device.running),
        });
        drop(devices);
        self.notify();
        self.start_or_resume(device_id);
        Ok(count)
    }

    pub fn cancel_current(self: &Rc<Self>, device_id: &str) {
        let mut start_next = false;
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.interrupted_disconnect = false;
            if let Some(cancelled) = &device.planned_cancel {
                cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            device.queue.request_cancel();
            if let Some(cancellable) = device.cancellable.take() {
                cancellable.cancel();
            } else if let Some(work) = device.paused_work.take() {
                device.reserved_bytes = device
                    .reserved_bytes
                    .saturating_sub(remaining_work_bytes(&work));
                device.queue.resume();
                device.queue.finish_job();
                start_next = device.connected;
            }
        }
        self.notify();
        if start_next {
            self.start_or_resume(device_id);
        }
    }

    pub fn create_playlist_draft(self: &Rc<Self>, device_id: &str, name: &str) -> Option<String> {
        let name = reprise_core::device_sync::safe_component(name, "Playlist");
        let created = {
            let mut devices = self.device_states.borrow_mut();
            let device = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)?;
            let exists_on_device = device
                .contents
                .playlists
                .iter()
                .any(|playlist| playlist.name == name);
            if !exists_on_device && !device.draft_playlists.contains(&name) {
                device.draft_playlists.push(name.clone());
                device.draft_playlists.sort();
            }
            name
        };
        self.notify();
        Some(created)
    }

    pub fn update_settings(self: &Rc<Self>, settings: DeviceSettings) -> Result<(), String> {
        save_settings(&self.conn.borrow(), &settings).map_err(|error| error.to_string())?;
        let device_id = settings.device_serial.clone();
        {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return Err("device is not connected".into());
            };
            device.settings = settings;
            device.sync_phase = PlannedSyncPhase::ComputingDelta;
            device.sync_error = None;
        }
        self.recompute_delta(&device_id)
    }

    pub fn set_pinned(
        self: &Rc<Self>,
        device_id: &str,
        track_id: i64,
        pinned: bool,
    ) -> Result<bool, String> {
        let changed = set_file_pinned(&self.conn.borrow(), device_id, track_id, pinned)
            .map_err(|error| error.to_string())?;
        if changed {
            self.recompute_delta(device_id)?;
        }
        Ok(changed)
    }

    pub fn selection_options(&self) -> Result<Vec<DeviceSelectionOption>, String> {
        let conn = self.conn.borrow();
        let mut options = reprise_core::library::playlists::list(&conn)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|playlist| DeviceSelectionOption {
                source: reprise_core::device_sync::SelectionSource::Playlist(playlist.id),
                name: playlist.name,
                track_count: usize::try_from(playlist.track_count.max(0)).unwrap_or(usize::MAX),
                smart: false,
            })
            .collect::<Vec<_>>();
        for playlist in reprise_core::library::playlists::list_smart(&conn)
            .map_err(|error| error.to_string())?
        {
            let source = reprise_core::device_sync::SelectionSource::Smart(playlist.id);
            let count =
                resolve_selection_track_ids(&conn, &DeviceSelection::Sources(vec![source.clone()]))
                    .map_err(|error| error.to_string())?
                    .len();
            options.push(DeviceSelectionOption {
                source,
                name: playlist.name,
                track_count: count,
                smart: true,
            });
        }
        Ok(options)
    }

    pub fn recompute_delta(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let settings = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.settings.clone())
            .ok_or_else(|| "device is not connected".to_string())?;
        let (delta, transfer_plan, tracks) = {
            let conn = self.conn.borrow();
            let ids = resolve_selection_track_ids(&conn, &settings.selection)
                .map_err(|error| error.to_string())?;
            let tracks = reprise_core::queries::query_sync_tracks(&conn, &ids)
                .map_err(|error| error.to_string())?;
            let transfer_plan = build_transfer_plan(tracks, settings.opus_bitrate);
            let candidates = transfer_plan
                .iter()
                .map(|entry| SyncCandidate {
                    track_id: entry.track.id,
                    device_path: entry.device_path.clone(),
                    transfer_bytes: entry.expected_bytes,
                    source_mtime: entry.track.source_mtime,
                })
                .collect::<Vec<_>>();
            let files = load_device_files(&conn, device_id).map_err(|error| error.to_string())?;
            let delta = compute_delta(&candidates, &files, settings.remove_deleted);
            let tracks = build_device_tracks(&conn, &transfer_plan, &files, &delta);
            (delta, transfer_plan, tracks)
        };
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.delta = Some(delta);
            device.transfer_plan = transfer_plan;
            device.tracks = tracks;
            device.selected_track_count = device.transfer_plan.len();
            device.sync_phase = PlannedSyncPhase::Idle;
            device.sync_error = None;
        }
        self.notify();
        Ok(())
    }

    pub fn refresh_contents(self: &Rc<Self>, device_id: &str) {
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
            {
                let mut devices = runtime.device_states.borrow_mut();
                if let Some(device) = devices.iter_mut().find(|device| device.descriptor.id == id) {
                    if device.scan_generation != generation {
                        return;
                    }
                    device.scanning = false;
                    match result {
                        Ok((contents, available)) => {
                            device.contents = contents;
                            device.available_bytes = available;
                            device.scan_error = None;
                        }
                        Err(error) => device.scan_error = Some(error),
                    }
                }
            }
            if let Err(error) = runtime.recompute_delta(&id) {
                if let Some(device) = runtime
                    .device_states
                    .borrow_mut()
                    .iter_mut()
                    .find(|device| device.descriptor.id == id)
                {
                    device.sync_phase = PlannedSyncPhase::Idle;
                    device.sync_error = Some(SyncFailure {
                        message: error,
                        failed_tracks: Vec::new(),
                    });
                }
                runtime.notify();
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
        let mut resume = Vec::new();
        let mut resume_planned = Vec::new();
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
                if state.running {
                    state.interrupted_disconnect = true;
                    if state.descriptor.reconnectable {
                        state.queue.pause_disconnected();
                    } else {
                        state
                            .queue
                            .fail_job("Device disconnected; reconnect and enqueue again");
                    }
                    if let Some(cancellable) = &state.cancellable {
                        cancellable.cancel();
                    }
                }
            }
            states.retain(|state| {
                incoming.contains_key(&state.descriptor.id)
                    || state.running
                    || state.paused_work.is_some()
                    || state.planned_cancel.is_some()
                    || state.resume_planned
                    || state.queue.snapshot().queued_jobs > 0
                    || matches!(
                        state.queue.snapshot().phase,
                        SyncPhase::PausedDisconnected | SyncPhase::Failed
                    )
            });
            for (id, descriptor) in incoming {
                if let Some(state) = states.iter_mut().find(|state| state.descriptor.id == id) {
                    let was_connected = state.connected;
                    state.descriptor = descriptor;
                    state.connected = true;
                    if !was_connected && state.paused_work.is_some() && !state.running {
                        resume.push(id.clone());
                    }
                    if !was_connected && state.resume_planned && state.planned_cancel.is_none() {
                        state.resume_planned = false;
                        resume_planned.push(id.clone());
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
        for id in resume {
            self.start_or_resume(&id);
        }
        for id in resume_planned {
            if let Err(error) = self.sync_now(&id) {
                tracing::warn!(device_id = id, %error, "could not resume device synchronization");
            }
        }
    }

    fn start_or_resume(self: &Rc<Self>, device_id: &str) {
        if self
            .active_device
            .borrow()
            .as_deref()
            .is_some_and(|active| active != device_id)
        {
            return;
        }
        let start = {
            let mut states = self.device_states.borrow_mut();
            let Some(device) = states
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return;
            };
            if device.running || device.planned_cancel.is_some() || !device.connected {
                return;
            }
            let work = if let Some(work) = device.paused_work.take() {
                device.queue.resume();
                work
            } else {
                let Some(job) = device.queue.start_next() else {
                    return;
                };
                Work {
                    job,
                    next_track: 0,
                    appended: Vec::new(),
                }
            };
            device.running = true;
            device.interrupted_disconnect = false;
            device.generation = device.generation.saturating_add(1);
            let cancellable = gio::Cancellable::new();
            device.cancellable = Some(cancellable.clone());
            Some((device.generation, work, cancellable))
        };
        if start.is_some() {
            self.active_device.replace(Some(device_id.to_string()));
        }
        self.notify();
        let Some((generation, work, cancellable)) = start else {
            return;
        };
        let weak = Rc::downgrade(self);
        let id = device_id.to_string();
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            run_work(weak, id, generation, work, cancellable).await;
        });
    }

    fn release_and_start_next(self: &Rc<Self>, device_id: &str) {
        if self.active_device.borrow().as_deref() == Some(device_id) {
            self.active_device.replace(None);
        }
        let mut candidates = self
            .device_states
            .borrow()
            .iter()
            .filter(|device| {
                device.connected
                    && !device.running
                    && (device.paused_work.is_some() || device.queue.snapshot().queued_jobs > 0)
            })
            .map(|device| device.descriptor.id.clone())
            .collect::<Vec<_>>();
        candidates.sort();
        if let Some(next) = candidates.first() {
            self.start_or_resume(next);
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

fn build_device_tracks(
    conn: &Connection,
    transfer_plan: &[TransferPlanEntry],
    files: &[reprise_core::device_sync::DeviceFileRecord],
    delta: &SyncDelta,
) -> Vec<DeviceTrackView> {
    let files_by_id = files
        .iter()
        .map(|file| (file.track_id, file))
        .collect::<HashMap<_, _>>();
    let selected = transfer_plan
        .iter()
        .map(|entry| entry.track.id)
        .collect::<HashSet<_>>();
    let queued = delta.to_copy.iter().copied().collect::<HashSet<_>>();
    let removing = delta.to_remove.iter().copied().collect::<HashSet<_>>();
    let mut tracks = transfer_plan
        .iter()
        .map(|entry| {
            let file = files_by_id.get(&entry.track.id);
            DeviceTrackView {
                track_id: entry.track.id,
                title: entry.track.title.clone(),
                artist: entry.track.artist.clone(),
                device_path: entry.device_path.clone(),
                size: entry.expected_bytes,
                duration_ms: entry.track.duration_ms,
                status: if queued.contains(&entry.track.id) {
                    DeviceTrackStatus::Queued
                } else {
                    DeviceTrackStatus::Synced
                },
                pinned: file.is_some_and(|file| file.pinned),
            }
        })
        .collect::<Vec<_>>();
    for file in files
        .iter()
        .filter(|file| !selected.contains(&file.track_id))
    {
        let metadata = conn
            .query_row(
                "SELECT title, artist, duration_ms FROM tracks WHERE id = ?1",
                [file.track_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or_else(|_| (file.device_path.clone(), String::new(), 0));
        tracks.push(DeviceTrackView {
            track_id: file.track_id,
            title: metadata.0,
            artist: metadata.1,
            device_path: file.device_path.clone(),
            size: file.size,
            duration_ms: metadata.2,
            status: if removing.contains(&file.track_id) {
                DeviceTrackStatus::Remove
            } else {
                DeviceTrackStatus::Synced
            },
            pinned: file.pinned,
        });
    }
    tracks.sort_by(|left, right| left.title.cmp(&right.title));
    tracks
}

#[path = "device_sync_legacy_queue.rs"]
mod legacy_queue;
#[path = "device_sync_planned.rs"]
mod planned;

use legacy_queue::{remaining_work_bytes, run_work};
#[cfg(test)]
pub(super) use planned::SyncStartError;
