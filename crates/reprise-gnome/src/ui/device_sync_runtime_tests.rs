use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::{SyncPhase, SyncSnapshot};
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceContents, DeviceDescriptor};
use rusqlite::Connection;

use super::device_sync_runtime::*;

type TestFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
type DeviceSubscriber = Rc<dyn Fn(Vec<DeviceDescriptor>)>;

#[derive(Default)]
struct FakeState {
    devices: RefCell<Vec<DeviceDescriptor>>,
    subscribers: RefCell<Vec<DeviceSubscriber>>,
    copy_order: RefCell<Vec<(String, String)>>,
    active_by_device: RefCell<HashMap<String, usize>>,
    max_by_device: RefCell<HashMap<String, usize>>,
    active_total: Cell<usize>,
    max_total: Cell<usize>,
    playlists: RefCell<Vec<(String, String)>>,
}

#[derive(Clone)]
struct FakeBackend {
    state: Rc<FakeState>,
    delay_ms: u64,
    available_bytes: Option<u64>,
}

impl FakeBackend {
    fn new(devices: Vec<DeviceDescriptor>, delay_ms: u64) -> Self {
        let state = Rc::new(FakeState::default());
        state.devices.replace(devices);
        Self {
            state,
            delay_ms,
            available_bytes: Some(1_000_000),
        }
    }

    fn with_available_bytes(mut self, available_bytes: Option<u64>) -> Self {
        self.available_bytes = available_bytes;
        self
    }

    fn set_devices(&self, devices: &[DeviceDescriptor]) {
        self.state.devices.replace(devices.to_owned());
        let subscribers = self.state.subscribers.borrow().clone();
        for subscriber in subscribers {
            subscriber(devices.to_owned());
        }
    }
}

impl DeviceBackend for FakeBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        self.state.devices.borrow().clone()
    }

    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {
        self.state.subscribers.borrow_mut().push(callback);
    }

    fn inspect(&self, _root_uri: String) -> TestFuture<(DeviceContents, Option<u64>)> {
        let available_bytes = self.available_bytes;
        Box::pin(async move { Ok((DeviceContents::default(), available_bytes)) })
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
            gtk4::glib::timeout_future(Duration::from_millis(delay_ms)).await;
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

    fn read_playlist(
        &self,
        _root_uri: String,
        _name: String,
    ) -> TestFuture<Vec<reprise_core::library::m3u::M3uEntry>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn replace_playlist(
        &self,
        device_id: String,
        _root_uri: String,
        name: String,
        _contents: Vec<u8>,
    ) -> TestFuture<()> {
        let state = self.state.clone();
        Box::pin(async move {
            state.playlists.borrow_mut().push((device_id, name));
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

async fn settle_until(runtime: &DeviceSyncRuntime, device_id: &str, phase: SyncPhase) {
    for _ in 0..1_000 {
        if snapshot(runtime, device_id).phase == phase {
            return;
        }
        gtk4::glib::timeout_future(Duration::from_millis(5)).await;
    }
    panic!("device sync did not reach {phase:?}");
}

fn snapshot(runtime: &DeviceSyncRuntime, id: &str) -> SyncSnapshot {
    runtime
        .devices()
        .into_iter()
        .find(|device| device.id == id)
        .unwrap()
        .snapshot
}

#[test]
fn rapid_jobs_for_one_device_copy_strictly_fifo_without_overlap() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 2));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        runtime.enqueue("a", "First", &[1, 2]).unwrap();
        runtime.enqueue("a", "Second", &[3]).unwrap();
        settle().await;
        assert_eq!(backend.state.max_by_device.borrow().get("a"), Some(&1));
        let targets = backend
            .state
            .copy_order
            .borrow()
            .iter()
            .map(|(_, target)| target.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            ["First/1-1.flac", "First/2-2.flac", "Second/3-3.flac"]
        );
    });
}

#[test]
fn different_devices_may_copy_concurrently() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("a", true), descriptor("b", true)],
            10,
        ));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        runtime.enqueue("a", "A", &[1]).unwrap();
        runtime.enqueue("b", "B", &[2]).unwrap();
        settle().await;
        assert_eq!(backend.state.max_total.get(), 2);
    });
}

#[test]
fn subscriber_receives_initial_state_and_progress_updates() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 4));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        let states = Rc::new(RefCell::new(Vec::new()));
        let observed = states.clone();
        let _subscription = runtime.subscribe(Rc::new(move |state| {
            observed.borrow_mut().push(state);
        }));
        assert_eq!(states.borrow().len(), 1);
        assert_eq!(states.borrow()[0].devices.len(), 1);
        runtime.enqueue("a", "A", &[1]).unwrap();
        settle().await;
        assert!(states.borrow().len() >= 4);
        assert_eq!(snapshot(&runtime, "a").phase, SyncPhase::Complete);
    });
}

#[test]
fn device_view_projects_descriptor_scan_and_idle_state() {
    run(async {
        let (_temp, conn) = fixture();
        let descriptor = descriptor("a", true);
        let expected_icon = descriptor.icon.clone();
        let backend = Rc::new(FakeBackend::new(vec![descriptor], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let device = runtime.devices().remove(0);
        assert_eq!(device.id, "a");
        assert_eq!(device.name, "Phone a");
        assert_eq!(device.icon, expected_icon);
        assert!(device.connected);
        assert_eq!(device.available_bytes, Some(1_000_000));
        assert!(device.contents.files.is_empty());
        assert!(!device.scanning);
        assert!(device.scan_error.is_none());
        assert_eq!(device.snapshot.phase, SyncPhase::Idle);
    });
}

#[test]
fn active_snapshot_reports_file_bytes_track_count_and_queued_job() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        runtime.enqueue("a", "A", &[1, 2]).unwrap();
        runtime.enqueue("a", "B", &[3]).unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        let active = snapshot(&runtime, "a");
        assert_eq!(active.phase, SyncPhase::Copying);
        assert_eq!(active.current_bytes, 50);
        assert_eq!(active.current_total, Some(100));
        assert_eq!(active.total_tracks, 2);
        assert_eq!(active.queued_jobs, 1);
        settle().await;
    });
}

#[test]
fn cancelling_current_job_keeps_and_runs_the_waiting_job() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        let observed = Rc::new(RefCell::new(Vec::new()));
        let snapshots = observed.clone();
        let _subscription = runtime.subscribe(Rc::new(move |state| {
            if let Some(device) = state.devices.first() {
                snapshots.borrow_mut().push(device.snapshot.clone());
            }
        }));
        runtime.enqueue("a", "Cancel", &[1, 2]).unwrap();
        runtime.enqueue("a", "Keep", &[3]).unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        runtime.cancel_current("a");
        settle().await;
        assert!(backend
            .state
            .copy_order
            .borrow()
            .iter()
            .any(|(_, target)| target == "Keep/3-3.flac"));
        assert!(observed.borrow().iter().all(|snapshot| {
            snapshot.current_name.as_deref() != Some("3.flac") || snapshot.current_bytes <= 50
        }));
    });
}

#[test]
fn stable_device_disconnect_pauses_and_replug_resumes_current_track() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 15));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        runtime.enqueue("a", "A", &[1]).unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.set_devices(&[]);
        gtk4::glib::timeout_future(Duration::from_millis(20)).await;
        assert_eq!(snapshot(&runtime, "a").phase, SyncPhase::PausedDisconnected);
        backend.set_devices(&[descriptor("a", true)]);
        settle().await;
        assert_eq!(snapshot(&runtime, "a").phase, SyncPhase::Complete);
    });
}

#[test]
fn uri_only_device_does_not_claim_safe_resume_after_disconnect() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("uri", false)], 15));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        runtime.enqueue("uri", "A", &[1]).unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        backend.set_devices(&[]);
        gtk4::glib::timeout_future(Duration::from_millis(30)).await;
        assert_eq!(snapshot(&runtime, "uri").phase, SyncPhase::Failed);
        backend.set_devices(&[descriptor("uri", false)]);
        settle().await;
        assert!(backend.state.copy_order.borrow().is_empty());
    });
}

#[test]
fn invalid_device_or_empty_resolution_is_rejected_without_a_job() {
    run(async {
        let (temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        assert!(matches!(
            runtime.enqueue("missing", "A", &[1]),
            Err(EnqueueError::UnknownDevice)
        ));
        std::fs::remove_file(temp.path().join("1.flac")).unwrap();
        assert!(matches!(
            runtime.enqueue("a", "A", &[1, 999]),
            Err(EnqueueError::NoUsableTracks)
        ));
    });
}

#[test]
fn known_insufficient_space_rejects_the_job_before_copying() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1).with_available_bytes(Some(150)),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        assert_eq!(
            runtime.enqueue("a", "Too Large", &[1, 2]),
            Err(EnqueueError::InsufficientSpace {
                required_bytes: 200,
                available_bytes: 150,
            })
        );
        settle().await;
        assert!(backend.state.copy_order.borrow().is_empty());
        assert_eq!(snapshot(&runtime, "a").phase, SyncPhase::Idle);
        assert_eq!(snapshot(&runtime, "a").queued_jobs, 0);
    });
}

#[test]
fn queued_jobs_reserve_space_for_later_actions_on_the_same_device() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 20).with_available_bytes(Some(150)),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.enqueue("a", "First", &[1]).unwrap();
        assert_eq!(
            runtime.enqueue("a", "Second", &[2]),
            Err(EnqueueError::InsufficientSpace {
                required_bytes: 100,
                available_bytes: 50,
            })
        );
        settle().await;
        assert_eq!(backend.state.copy_order.borrow().len(), 1);
    });
}

#[test]
fn cancelling_a_job_releases_its_reserved_space() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 20).with_available_bytes(Some(150)),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.enqueue("a", "Cancel", &[1]).unwrap();
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;
        runtime.cancel_current("a");
        settle().await;

        assert_eq!(runtime.enqueue("a", "After Cancel", &[2]), Ok(1));
        settle().await;
        assert_eq!(snapshot(&runtime, "a").phase, SyncPhase::Complete);
    });
}

#[test]
fn playlist_drafts_are_sanitized_deduplicated_and_do_no_device_io() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        assert_eq!(
            runtime.create_playlist_draft("a", "../ Road / Mix"),
            Some("Road Mix".into())
        );
        assert_eq!(
            runtime.create_playlist_draft("a", "Road Mix"),
            Some("Road Mix".into())
        );
        assert_eq!(runtime.devices()[0].draft_playlists, ["Road Mix"]);
        assert!(backend.state.copy_order.borrow().is_empty());
        assert!(backend.state.playlists.borrow().is_empty());
    });
}

#[test]
fn unplugged_idle_device_leaves_the_detected_device_list() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        backend.set_devices(&[]);
        assert!(runtime.devices().is_empty());
    });
}

#[test]
fn local_gio_backend_runs_two_jobs_in_order_with_monotone_progress_and_m3u8() {
    run(async {
        let (_sources, conn) = fixture();
        let device_root = tempfile::tempdir().unwrap();
        let backend = Rc::new(
            super::device_sync_smoke::SmokeDeviceBackend::for_root(device_root.path()).unwrap(),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let observed = snapshots.clone();
        let _subscription = runtime.subscribe(Rc::new(move |state| {
            if let Some(device) = state.devices.first() {
                observed.borrow_mut().push(device.snapshot.clone());
            }
        }));
        runtime
            .enqueue(super::device_sync_smoke::DEVICE_ID, "Road", &[1, 2])
            .unwrap();
        runtime
            .enqueue(super::device_sync_smoke::DEVICE_ID, "Road", &[3])
            .unwrap();
        settle().await;

        for id in 1..=3 {
            assert!(device_root
                .path()
                .join(format!("Music/Reprise/Road/{id}-{id}.flac"))
                .is_file());
        }
        let playlist =
            std::fs::read_to_string(device_root.path().join("Music/Reprise/Road.m3u8")).unwrap();
        let paths = reprise_core::library::m3u::parse_m3u(&playlist)
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        assert_eq!(paths, ["Road/1-1.flac", "Road/2-2.flac", "Road/3-3.flac"]);

        for total_tracks in [2, 1] {
            let progress = snapshots
                .borrow()
                .iter()
                .filter(|snapshot| snapshot.total_tracks == total_tracks)
                .map(|snapshot| snapshot.completed_bytes + snapshot.current_bytes)
                .collect::<Vec<_>>();
            assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
        }
        assert_eq!(
            snapshot(&runtime, super::device_sync_smoke::DEVICE_ID).phase,
            SyncPhase::Complete
        );
    });
}

#[test]
fn local_gio_cancel_removes_partial_and_runs_the_waiting_job() {
    run(async {
        let (sources, conn) = fixture();
        std::fs::write(
            sources.path().join("4.flac"),
            vec![4_u8; 16 * 1_024 * 1_024],
        )
        .unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let backend = Rc::new(
            super::device_sync_smoke::SmokeDeviceBackend::for_root(device_root.path()).unwrap(),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);

        runtime
            .enqueue(super::device_sync_smoke::DEVICE_ID, "Done", &[1])
            .unwrap();
        settle_until(
            &runtime,
            super::device_sync_smoke::DEVICE_ID,
            SyncPhase::Complete,
        )
        .await;

        let cancelled = Rc::new(Cell::new(false));
        let cancelled_for_callback = cancelled.clone();
        let runtime_for_callback = Rc::downgrade(&runtime);
        let _subscription = runtime.subscribe(Rc::new(move |state| {
            let Some(device) = state.devices.first() else {
                return;
            };
            if device.snapshot.current_name.as_deref() == Some("4.flac")
                && device.snapshot.current_bytes > 0
                && !cancelled_for_callback.replace(true)
            {
                if let Some(runtime) = runtime_for_callback.upgrade() {
                    runtime.cancel_current(super::device_sync_smoke::DEVICE_ID);
                }
            }
        }));
        runtime
            .enqueue(super::device_sync_smoke::DEVICE_ID, "Cancel", &[4])
            .unwrap();
        runtime
            .enqueue(super::device_sync_smoke::DEVICE_ID, "Keep", &[2])
            .unwrap();
        settle_until(
            &runtime,
            super::device_sync_smoke::DEVICE_ID,
            SyncPhase::Complete,
        )
        .await;

        assert!(cancelled.get());
        assert!(device_root
            .path()
            .join("Music/Reprise/Done/1-1.flac")
            .is_file());
        assert!(device_root
            .path()
            .join("Music/Reprise/Keep/2-2.flac")
            .is_file());
        assert!(!device_root
            .path()
            .join("Music/Reprise/Cancel/4-4.flac")
            .exists());
        assert!(!device_root
            .path()
            .join("Music/Reprise/Cancel/4-4.flac.reprise-part")
            .exists());
    });
}
