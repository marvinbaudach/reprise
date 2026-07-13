// The runtime is composed in Task 3; its enqueue/subscription API becomes a
// production UI consumer in the immediately following synchronization-page
// task. Keep the per-task all-targets gate clean across that deliberate seam.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::{Rc, Weak};

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::{
    merge_playlist_entries, track_relative_path, DeviceQueue, SyncJob, SyncPhase, SyncSnapshot,
    SyncTrack, TrackOutcome,
};
use reprise_core::library::m3u::{M3uEntry, M3uExportEntry};
use reprise_platform_linux::device_sync::{
    CopyOutcome, DeviceContents, DeviceDescriptor, DeviceMonitor, DeviceStorage,
};
use rusqlite::Connection;

pub type BackendFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
type StateCallback = Rc<dyn Fn(DeviceSyncState)>;

pub trait DeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor>;
    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>);
    fn inspect(&self, root_uri: String) -> BackendFuture<(DeviceContents, Option<u64>)>;
    #[allow(clippy::too_many_arguments)]
    fn copy_track(
        &self,
        device_id: String,
        root_uri: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome>;
    fn read_playlist(&self, root_uri: String, name: String) -> BackendFuture<Vec<M3uEntry>>;
    fn replace_playlist(
        &self,
        device_id: String,
        root_uri: String,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()>;
}

struct GioDeviceBackend {
    monitor: DeviceMonitor,
}

impl GioDeviceBackend {
    fn new(monitor: DeviceMonitor) -> Self {
        Self { monitor }
    }
}

impl DeviceBackend for GioDeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        self.monitor.devices()
    }

    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {
        self.monitor.subscribe(callback);
    }

    fn inspect(&self, root_uri: String) -> BackendFuture<(DeviceContents, Option<u64>)> {
        Box::pin(async move {
            let storage = DeviceStorage::from_uri(&root_uri);
            let contents = storage.inspect().await.map_err(|error| error.to_string())?;
            let available = storage
                .available_bytes()
                .await
                .map_err(|error| error.to_string())?;
            Ok((contents, available))
        })
    }

    fn copy_track(
        &self,
        _device_id: String,
        root_uri: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .copy_track(
                    &gio::File::for_path(source_path),
                    &relative_target,
                    expected_size,
                    &cancellable,
                    move |copied, total| progress(copied, total),
                )
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn read_playlist(&self, root_uri: String, name: String) -> BackendFuture<Vec<M3uEntry>> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .read_playlist(&name)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn replace_playlist(
        &self,
        _device_id: String,
        root_uri: String,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .replace_playlist(&name, contents)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

#[derive(Clone, Debug)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub root_uri: String,
    pub icon: gio::Icon,
    pub reconnectable: bool,
    pub connected: bool,
    pub available_bytes: Option<u64>,
    pub contents: DeviceContents,
    pub scanning: bool,
    pub scan_error: Option<String>,
    pub draft_playlists: Vec<String>,
    pub snapshot: SyncSnapshot,
}

#[derive(Clone, Debug, Default)]
pub struct DeviceSyncState {
    pub devices: Vec<DeviceView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnqueueError {
    UnknownDevice,
    NoUsableTracks,
    Database(String),
}

impl fmt::Display for EnqueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownDevice => formatter.write_str("device is not connected"),
            Self::NoUsableTracks => formatter.write_str("no available tracks were selected"),
            Self::Database(error) => {
                write!(formatter, "could not resolve selected tracks: {error}")
            }
        }
    }
}

impl std::error::Error for EnqueueError {}

pub struct Subscription {
    cancel: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.borrow_mut().take() {
            cancel();
        }
    }
}

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
}

impl DeviceState {
    fn new(descriptor: DeviceDescriptor) -> Self {
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
        }
    }

    fn view(&self) -> DeviceView {
        DeviceView {
            id: self.descriptor.id.clone(),
            name: self.descriptor.name.clone(),
            root_uri: self.descriptor.root_uri.clone(),
            icon: self.descriptor.icon.clone(),
            reconnectable: self.descriptor.reconnectable,
            connected: self.connected,
            available_bytes: self.available_bytes,
            contents: self.contents.clone(),
            scanning: self.scanning,
            scan_error: self.scan_error.clone(),
            draft_playlists: self.draft_playlists.clone(),
            snapshot: self.queue.snapshot(),
        }
    }
}

pub struct DeviceSyncRuntime {
    conn: Rc<RefCell<Connection>>,
    backend: Rc<dyn DeviceBackend>,
    device_states: RefCell<HashMap<String, DeviceState>>,
    subscribers: RefCell<HashMap<u64, StateCallback>>,
    next_subscription_id: Cell<u64>,
    next_job_id: Cell<u64>,
    weak_self: RefCell<Weak<Self>>,
}

impl DeviceSyncRuntime {
    pub fn new(conn: &Rc<RefCell<Connection>>, monitor: DeviceMonitor) -> Rc<Self> {
        Self::with_backend(conn, Rc::new(GioDeviceBackend::new(monitor)))
    }

    pub fn with_backend(
        conn: &Rc<RefCell<Connection>>,
        backend: Rc<dyn DeviceBackend>,
    ) -> Rc<Self> {
        let runtime = Rc::new(Self {
            conn: conn.clone(),
            backend,
            device_states: RefCell::new(HashMap::new()),
            subscribers: RefCell::new(HashMap::new()),
            next_subscription_id: Cell::new(1),
            next_job_id: Cell::new(1),
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
            .values()
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
        let connected = self
            .device_states
            .borrow()
            .get(device_id)
            .is_some_and(|device| device.connected);
        if !connected {
            return Err(EnqueueError::UnknownDevice);
        }
        let tracks = reprise_core::queries::query_sync_tracks(&self.conn.borrow(), ids)
            .map_err(|error| EnqueueError::Database(error.to_string()))?;
        if tracks.is_empty() {
            return Err(EnqueueError::NoUsableTracks);
        }
        let count = tracks.len();
        let job_id = self.next_job_id.get();
        self.next_job_id.set(job_id.saturating_add(1));
        if let Some(device) = self.device_states.borrow_mut().get_mut(device_id) {
            device.queue.enqueue(SyncJob {
                id: job_id,
                playlist: reprise_core::device_sync::safe_component(playlist, "Playlist"),
                tracks,
            });
        }
        self.notify();
        self.start_or_resume(device_id);
        Ok(count)
    }

    pub fn cancel_current(self: &Rc<Self>, device_id: &str) {
        let mut start_next = false;
        if let Some(device) = self.device_states.borrow_mut().get_mut(device_id) {
            device.interrupted_disconnect = false;
            device.queue.request_cancel();
            if let Some(cancellable) = device.cancellable.take() {
                cancellable.cancel();
            } else if device.paused_work.take().is_some() {
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
            let device = devices.get_mut(device_id)?;
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

    pub fn refresh_contents(self: &Rc<Self>, device_id: &str) {
        let request = {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices.get_mut(device_id) else {
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
            if let Some(device) = runtime.device_states.borrow_mut().get_mut(&id) {
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
            runtime.notify();
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
        let mut refresh = Vec::new();
        {
            let mut states = self.device_states.borrow_mut();
            for (id, state) in states.iter_mut() {
                if incoming.contains_key(id) || !state.connected {
                    continue;
                }
                state.connected = false;
                state.scanning = false;
                state.scan_generation = state.scan_generation.saturating_add(1);
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
            states.retain(|id, state| {
                incoming.contains_key(id)
                    || state.running
                    || state.paused_work.is_some()
                    || state.queue.snapshot().queued_jobs > 0
                    || matches!(
                        state.queue.snapshot().phase,
                        SyncPhase::PausedDisconnected | SyncPhase::Failed
                    )
            });
            for (id, descriptor) in incoming {
                if let Some(state) = states.get_mut(&id) {
                    let was_connected = state.connected;
                    state.descriptor = descriptor;
                    state.connected = true;
                    if !was_connected && state.paused_work.is_some() && !state.running {
                        resume.push(id);
                    }
                } else {
                    states.insert(id.clone(), DeviceState::new(descriptor));
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
    }

    fn start_or_resume(self: &Rc<Self>, device_id: &str) {
        let start = {
            let mut states = self.device_states.borrow_mut();
            let Some(device) = states.get_mut(device_id) else {
                return;
            };
            if device.running || !device.connected {
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

async fn run_work(
    weak: Weak<DeviceSyncRuntime>,
    device_id: String,
    generation: u64,
    mut work: Work,
    cancellable: gio::Cancellable,
) {
    while work.next_track < work.job.tracks.len() {
        let Some(runtime) = weak.upgrade() else {
            return;
        };
        if cancellable.is_cancelled() {
            finish_interrupted(&runtime, &device_id, generation, work);
            return;
        }
        let track = work.job.tracks[work.next_track].clone();
        let root_uri = {
            let mut states = runtime.device_states.borrow_mut();
            let Some(device) = states.get_mut(&device_id) else {
                return;
            };
            if device.generation != generation || !device.connected {
                None
            } else {
                device
                    .queue
                    .begin_track(&track.original_name, Some(track.size_bytes));
                Some(device.descriptor.root_uri.clone())
            }
        };
        let Some(root_uri) = root_uri else {
            finish_interrupted(&runtime, &device_id, generation, work);
            return;
        };
        runtime.notify();
        let relative_target = track_relative_path(&work.job.playlist, &track);
        let progress_runtime = Rc::downgrade(&runtime);
        let progress_id = device_id.clone();
        let progress: Rc<dyn Fn(u64, u64)> = Rc::new(move |copied, _total| {
            let Some(runtime) = progress_runtime.upgrade() else {
                return;
            };
            let updated = {
                let mut states = runtime.device_states.borrow_mut();
                let Some(device) = states.get_mut(&progress_id) else {
                    return;
                };
                if device.generation != generation {
                    return;
                }
                device.queue.set_track_bytes(copied);
                true
            };
            if updated {
                runtime.notify();
            }
        });
        let result = runtime
            .backend
            .copy_track(
                device_id.clone(),
                root_uri,
                track.source_path.clone(),
                relative_target.clone(),
                track.size_bytes,
                cancellable.clone(),
                progress,
            )
            .await;
        if cancellable.is_cancelled() {
            finish_interrupted(&runtime, &device_id, generation, work);
            return;
        }
        match result {
            Ok(outcome) => {
                if let Some(device) = runtime.device_states.borrow_mut().get_mut(&device_id) {
                    if device.generation != generation {
                        return;
                    }
                    device.queue.set_track_bytes(track.size_bytes);
                    device.queue.finish_track(match outcome {
                        CopyOutcome::Copied => TrackOutcome::Copied,
                        CopyOutcome::Skipped => TrackOutcome::Skipped,
                    });
                }
                work.appended.push(export_entry(&track, relative_target));
            }
            Err(error) => {
                tracing::warn!(device_id, %error, "device track copy failed");
                if let Some(device) = runtime.device_states.borrow_mut().get_mut(&device_id) {
                    if device.generation != generation {
                        return;
                    }
                    device.queue.finish_track(TrackOutcome::Failed);
                }
            }
        }
        work.next_track += 1;
        runtime.notify();
    }
    finish_playlist(&weak, &device_id, generation, work, cancellable).await;
}

fn finish_interrupted(
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    generation: u64,
    work: Work,
) {
    let mut continue_queue = false;
    if let Some(device) = runtime.device_states.borrow_mut().get_mut(device_id) {
        if device.generation != generation {
            return;
        }
        device.running = false;
        device.cancellable = None;
        if device.interrupted_disconnect && device.descriptor.reconnectable {
            device.paused_work = Some(work);
            device.queue.pause_disconnected();
            continue_queue = device.connected;
        } else if device.interrupted_disconnect {
            device
                .queue
                .fail_job("Device disconnected; reconnect and enqueue again");
        } else {
            device.queue.finish_job();
            continue_queue = device.connected;
        }
        device.interrupted_disconnect = false;
    }
    runtime.notify();
    if continue_queue {
        runtime.start_or_resume(device_id);
    }
}

async fn finish_playlist(
    weak: &Weak<DeviceSyncRuntime>,
    device_id: &str,
    generation: u64,
    work: Work,
    cancellable: gio::Cancellable,
) {
    let Some(runtime) = weak.upgrade() else {
        return;
    };
    let interrupted = runtime
        .device_states
        .borrow()
        .get(device_id)
        .is_none_or(|device| {
            !device.connected || device.interrupted_disconnect || cancellable.is_cancelled()
        });
    if interrupted {
        finish_interrupted(&runtime, device_id, generation, work);
        return;
    }
    let root_uri = {
        let states = runtime.device_states.borrow();
        let Some(device) = states.get(device_id) else {
            return;
        };
        device.descriptor.root_uri.clone()
    };
    let result = async {
        let existing = runtime
            .backend
            .read_playlist(root_uri.clone(), work.job.playlist.clone())
            .await?;
        let contents = merge_playlist_entries(&existing, &work.appended).into_bytes();
        runtime
            .backend
            .replace_playlist(
                device_id.to_string(),
                root_uri,
                work.job.playlist.clone(),
                contents,
            )
            .await
    }
    .await;
    let interrupted = runtime
        .device_states
        .borrow()
        .get(device_id)
        .is_some_and(|device| device.interrupted_disconnect || cancellable.is_cancelled());
    if interrupted {
        finish_interrupted(&runtime, device_id, generation, work);
        return;
    }
    if let Some(device) = runtime.device_states.borrow_mut().get_mut(device_id) {
        if device.generation != generation {
            return;
        }
        device.running = false;
        device.cancellable = None;
        match result {
            Ok(()) => device.queue.finish_job(),
            Err(error) => device.queue.fail_job(error),
        }
    }
    runtime.notify();
    runtime.refresh_contents(device_id);
    runtime.start_or_resume(device_id);
}

fn export_entry(track: &SyncTrack, path: String) -> M3uExportEntry {
    let display = if track.artist.trim().is_empty() {
        track.title.clone()
    } else {
        format!("{} - {}", track.artist, track.title)
    };
    M3uExportEntry {
        path,
        duration_secs: track.duration_ms.max(0) / 1_000,
        display,
    }
}
