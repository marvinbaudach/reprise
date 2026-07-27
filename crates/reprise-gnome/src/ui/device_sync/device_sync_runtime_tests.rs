use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::settings::save_settings;
use reprise_core::device_sync::{
    DeviceSelection, DeviceSettings, DeviceStorageAccess, DeviceStorageInspection,
    DeviceStorageSnapshot, SelectionSource,
};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor};
use reprise_platform_linux::device_transfer::{TranscodeProfile, TranscodeRequest, TranscodedFile};
use rusqlite::Connection;

use super::device_sync_runtime::*;

type TestFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
type DeviceSubscriber = Rc<dyn Fn(Vec<DeviceDescriptor>)>;
type DeleteObserver = Rc<dyn Fn(&str)>;

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
struct FakeState {
    devices: RefCell<Vec<DeviceDescriptor>>,
    subscribers: RefCell<Vec<DeviceSubscriber>>,
    copy_order: RefCell<Vec<(String, String)>>,
    copy_attempts: Cell<usize>,
    active_by_device: RefCell<HashMap<String, usize>>,
    max_by_device: RefCell<HashMap<String, usize>>,
    active_total: Cell<usize>,
    max_total: Cell<usize>,
    playlists: RefCell<Vec<(String, String, Vec<u8>)>>,
    deleted: RefCell<Vec<String>>,
    planned_operations: RefCell<Vec<(String, &'static str)>>,
    available_bytes: Cell<Option<u64>>,
    total_bytes: Cell<Option<u64>>,
    storage_access: Cell<DeviceStorageAccess>,
    transcode_probe_error: RefCell<Option<String>>,
    copy_gate: RefCell<Option<CopyGate>>,
    playlist_error: RefCell<Option<String>>,
    playlist_gate: RefCell<Option<PlaylistGate>>,
    inspection_gate: RefCell<Option<InspectionGate>>,
    inspection_error: RefCell<Option<String>>,
    delete_observer: RefCell<Option<DeleteObserver>>,
}

#[derive(Clone)]
struct FakeBackend {
    state: Rc<FakeState>,
    delay_ms: u64,
}

impl FakeBackend {
    fn new(devices: Vec<DeviceDescriptor>, delay_ms: u64) -> Self {
        let state = Rc::new(FakeState::default());
        state.devices.replace(devices);
        state.available_bytes.set(Some(1_000_000));
        state.total_bytes.set(Some(2_000_000));
        Self { state, delay_ms }
    }

    fn with_available_bytes(self, available_bytes: Option<u64>) -> Self {
        self.state.available_bytes.set(available_bytes);
        self
    }

    fn with_storage_access(self, access: DeviceStorageAccess) -> Self {
        self.state.storage_access.set(access);
        self
    }

    fn with_transcode_probe_error(self, error: &str) -> Self {
        self.state.transcode_probe_error.replace(Some(error.into()));
        self
    }

    fn with_playlist_error(self, error: &str) -> Self {
        self.state.playlist_error.replace(Some(error.into()));
        self
    }

    fn set_available_bytes(&self, available_bytes: Option<u64>) {
        self.state.available_bytes.set(available_bytes);
    }

    fn set_devices(&self, devices: &[DeviceDescriptor]) {
        self.state.devices.replace(devices.to_owned());
        let subscribers = self.state.subscribers.borrow().clone();
        for subscriber in subscribers {
            subscriber(devices.to_owned());
        }
    }

    fn gate_copies(
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

    fn gate_playlist(&self) -> (async_channel::Receiver<()>, async_channel::Sender<()>) {
        let (started, started_rx) = async_channel::bounded(1);
        let (release, release_rx) = async_channel::bounded(1);
        self.state.playlist_gate.replace(Some(PlaylistGate {
            started,
            release: release_rx,
        }));
        (started_rx, release)
    }

    fn gate_next_inspection(&self) -> (async_channel::Receiver<()>, async_channel::Sender<()>) {
        let (started, started_rx) = async_channel::bounded(1);
        let (release, release_rx) = async_channel::bounded(1);
        self.state.inspection_gate.replace(Some(InspectionGate {
            started,
            release: release_rx,
        }));
        (started_rx, release)
    }

    fn fail_next_inspection(&self, error: &str) {
        self.state.inspection_error.replace(Some(error.into()));
    }

    fn observe_deletes(&self, observer: DeleteObserver) {
        self.state.delete_observer.replace(Some(observer));
    }
}

impl DeviceBackend for FakeBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        self.state.devices.borrow().clone()
    }

    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {
        self.state.subscribers.borrow_mut().push(callback);
    }

    fn inspect(&self, _root_uri: String) -> TestFuture<DeviceStorageInspection> {
        let available_bytes = self.state.available_bytes.get();
        let total_bytes = self.state.total_bytes.get();
        let storage_access = self.state.storage_access.get();
        let gate = self.state.inspection_gate.borrow_mut().take();
        let inspection_error = self.state.inspection_error.borrow_mut().take();
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
            })
        })
    }

    fn copy_track(
        &self,
        device_id: String,
        _root_uri: String,
        _source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> TestFuture<CopyOutcome> {
        let state = self.state.clone();
        let delay_ms = self.delay_ms;
        Box::pin(async move {
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
            state
                .copy_order
                .borrow_mut()
                .push((device_id, relative_target));
            Ok(CopyOutcome::Copied)
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

    fn delete_track(&self, _root_uri: String, relative_target: String) -> TestFuture<bool> {
        let state = self.state.clone();
        Box::pin(async move {
            let observer = state.delete_observer.borrow().clone();
            if let Some(observer) = observer {
                observer(&relative_target);
            }
            state.deleted.borrow_mut().push(relative_target);
            Ok(true)
        })
    }

    fn replace_playlist(
        &self,
        device_id: String,
        _root_uri: String,
        name: String,
        contents: Vec<u8>,
    ) -> TestFuture<()> {
        let state = self.state.clone();
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
}

fn descriptor(id: &str, reconnectable: bool) -> DeviceDescriptor {
    DeviceDescriptor {
        id: id.into(),
        name: format!("Phone {id}"),
        root_uri: format!("mtp://{id}"),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
        reconnectable,
    }
}

fn fixture() -> (tempfile::TempDir, Rc<RefCell<Connection>>) {
    let temp = tempfile::tempdir().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for id in 1..=4 {
        let path = temp.path().join(format!("{id}.flac"));
        std::fs::write(&path, vec![id as u8; 100]).unwrap();
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,duration_ms,added_at) VALUES (?1,?2,?3,'Artist',1000,0)",
            rusqlite::params![id, path.to_string_lossy(), format!("Track {id}")],
        )
        .unwrap();
    }
    (temp, Rc::new(RefCell::new(conn)))
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

fn select_road_playlist(conn: &Rc<RefCell<Connection>>, ids: &[i64]) {
    conn.borrow()
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 0)",
            [],
        )
        .unwrap();
    for (position, track_id) in ids.iter().enumerate() {
        conn.borrow()
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (10, ?1, ?2)",
                rusqlite::params![track_id, position as i64],
            )
            .unwrap();
    }
    save_road_settings(conn, "a");
}

fn save_road_settings(conn: &Rc<RefCell<Connection>>, device_id: &str) {
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
        },
    )
    .unwrap();
}

#[path = "device_sync_compact_tests.rs"]
mod compact_tests;
#[path = "device_sync_planned_tests.rs"]
mod planned_tests;
#[path = "device_sync_readback_tests.rs"]
mod readback_tests;
#[path = "device_sync_safety_tests.rs"]
mod safety_tests;
#[path = "device_sync_transfer_profile_tests.rs"]
mod transfer_profile_tests;
