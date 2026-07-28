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
    DeviceStorageSnapshot, ManagedDeviceFile, SelectionSource,
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
    /// Every `replace_track`/`delete_track` call that reached this double,
    /// recorded as `(target_path, relative_path)` — the seam's proof that
    /// the right named target (`MTP-18`) was used, without touching a real
    /// or simulated filesystem.
    managed_copies: RefCell<Vec<(String, String)>>,
    managed_deleted: RefCell<Vec<(String, String)>>,
    podcast_files: RefCell<Vec<ManagedDeviceFile>>,
    youtube_files: RefCell<Vec<ManagedDeviceFile>>,
    ejected: RefCell<Vec<String>>,
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
        let podcast_files = self.state.podcast_files.borrow().clone();
        let youtube_files = self.state.youtube_files.borrow().clone();
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

    fn replace_track(
        &self,
        device_id: String,
        _root_uri: String,
        target_path: String,
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
                .push((device_id, relative_target.clone()));
            state
                .managed_copies
                .borrow_mut()
                .push((target_path, relative_target));
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

    fn delete_track(
        &self,
        _root_uri: String,
        target_path: String,
        relative_target: String,
    ) -> TestFuture<bool> {
        let state = self.state.clone();
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
        _target_path: String,
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

    fn eject(&self, device_id: String) -> TestFuture<bool> {
        let state = self.state.clone();
        Box::pin(async move {
            state.ejected.borrow_mut().push(device_id);
            Ok(true)
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

#[test]
fn mtp_24_podcast_and_youtube_audio_are_always_copied_1_to_1_never_transcoded() {
    run(async {
        let (downloads, conn) = fixture();
        // Named with a `.flac` extension on purpose: if this ever went
        // through the music transfer-profile branch it would be flagged
        // as lossless and transcoded. Podcast/YouTube audio must never
        // take that branch (`MTP-24`) — it copies whatever bytes exist.
        let episode_path = downloads.path().join("episode.flac");
        std::fs::write(&episode_path, b"already-opus-bytes").unwrap();
        conn.borrow()
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'youtube', 'https://example.test/channel', 'Channel', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a');",
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'yt-100', 'Video', 'https://example.test/video.webm',
                         ?1, 18, 1)",
                [episode_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(
            backend.state.managed_copies.borrow().as_slice(),
            [(
                "/Music/Reprise-YouTube".to_string(),
                "Channel/100-Video.flac".to_string()
            )]
        );
        assert!(
            backend
                .state
                .planned_operations
                .borrow()
                .iter()
                .all(|(_, kind)| *kind != "transcode"),
            "podcast/YouTube audio must never be transcoded"
        );
    });
}

#[test]
fn pod_12_planned_sync_copies_selected_rss_and_youtube_each_to_its_own_target() {
    run(async {
        let (downloads, conn) = fixture();
        let rss_path = downloads.path().join("rss.mp3");
        let youtube_path = downloads.path().join("youtube.mp3");
        std::fs::write(&rss_path, b"rss audio").unwrap();
        std::fs::write(&youtube_path, b"youtube").unwrap();
        conn.borrow()
            .execute_batch(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES
                 (10, 'rss', 'https://example.test/rss', 'RSS Show', 0, 1, 1),
                 (11, 'youtube', 'https://example.test/youtube', 'Video', 0, 1, 1);
                 INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a'), (11, 'a');",
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'Episode', 'https://example.test/rss.mp3',
                         ?1, 9, 1)",
                [rss_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, downloaded_path,
                  downloaded_bytes, first_seen_at)
                 VALUES (101, 11, 'yt-101', 'Video', 'https://example.test/youtube.webm',
                         ?1, 7, 1)",
                [youtube_path.to_string_lossy().as_ref()],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.podcast_files.replace(vec![ManagedDeviceFile {
            relative_path: "Old Show/99-Old.mp3".into(),
            size_bytes: 4,
        }]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let page = runtime.devices().remove(0).page;
        assert!(page.blockers.is_empty());
        // Both the RSS episode (9 bytes) and the YouTube episode (7 bytes)
        // are wanted — YouTube is no longer defensively cleared (POD-12).
        assert_eq!(page.changes.transfer_bytes, 16);
        assert!(page.controls.can_start);

        runtime.sync_now("a").unwrap();
        settle().await;

        let mut copies = backend.state.managed_copies.borrow().clone();
        copies.sort();
        assert_eq!(
            copies,
            [
                (
                    "/Music/Reprise-YouTube".to_string(),
                    "Video/101-Video.mp3".to_string()
                ),
                (
                    "/Podcasts/Reprise".to_string(),
                    "RSS Show/100-Episode.mp3".to_string()
                ),
            ]
        );
        assert_eq!(
            backend.state.managed_deleted.borrow().as_slice(),
            [(
                "/Podcasts/Reprise".to_string(),
                "Old Show/99-Old.mp3".to_string()
            )]
        );
    });
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

/// `MTP-30`: seeds a device-settings row with the switch off and no
/// playlist selection, for tests that set up their own podcast/YouTube work
/// directly via SQL and then drive `sync_now` manually — without this, the
/// default-on switch (`DEFAULT 1`, schema v44) would start a sync on
/// connect before the test's own `sync_now` call runs, doubling every copy.
fn disable_auto_start(conn: &Rc<RefCell<Connection>>, device_id: &str) {
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: device_id.into(),
            device_name: format!("Phone {device_id}"),
            selection: DeviceSelection::Sources(Vec::new()),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            sync_automatically: false,
        },
    )
    .unwrap();
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
            // `MTP-30`: most tests using this fixture orchestrate `sync_now`
            // manually (gates, cancellation races, progress observation) and
            // must not race an automatic start on connect. `MTP-30`'s own
            // tests (`device_sync_auto_start_tests.rs`) set this explicitly
            // instead of relying on this shared fixture.
            sync_automatically: false,
        },
    )
    .unwrap();
}

#[path = "device_sync_auto_start_tests.rs"]
mod auto_start_tests;
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
