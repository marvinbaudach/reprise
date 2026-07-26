//! Explicit local-root hook for isolated device synchronization smoke runs.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::{DeviceStorageInspection, SyncPhase};
use reprise_core::library::m3u::M3uEntry;
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor, DeviceStorage};
use reprise_platform_linux::device_transfer::{
    probe_mp3_transcode_capability, transcode_to_mp3, Mp3TranscodeRequest, TranscodedFile,
};
use rusqlite::Connection;

use super::device_sync_runtime::{BackendFuture, DeviceBackend, DeviceSyncRuntime, Subscription};

pub(in crate::ui) const ROOT_ENV: &str = "REPRISE_SMOKE_DEVICE_ROOT";
const IDS_ENV: &str = "REPRISE_SMOKE_DEVICE_TRACK_IDS";
const PLAYLIST_ENV: &str = "REPRISE_SMOKE_DEVICE_PLAYLIST";
pub(in crate::ui) const DEVICE_ID: &str = "reprise-smoke-device";

pub(in crate::ui) struct SmokeDeviceBackend {
    descriptor: DeviceDescriptor,
}

impl SmokeDeviceBackend {
    pub(in crate::ui) fn for_root(root: &Path) -> Option<Self> {
        let root = safe_smoke_root(root)?;
        Some(Self {
            descriptor: DeviceDescriptor {
                id: DEVICE_ID.into(),
                name: "Android Smoke Device".into(),
                root_uri: gio::File::for_path(root).uri().into(),
                icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
                reconnectable: true,
            },
        })
    }

    fn storage(root_uri: &str) -> DeviceStorage {
        DeviceStorage::from_uri(root_uri)
    }
}

impl DeviceBackend for SmokeDeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor.clone()]
    }

    fn subscribe_devices(&self, _callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {}

    fn inspect(&self, root_uri: String) -> BackendFuture<DeviceStorageInspection> {
        Box::pin(async move {
            let storage = Self::storage(&root_uri);
            storage.inspect().await.map_err(|error| error.to_string())
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
            Self::storage(&root_uri)
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
            Self::storage(&root_uri)
                .read_playlist(&name)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn replace_track(
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
            Self::storage(&root_uri)
                .replace_track(
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

    fn cleanup_partials(&self, root_uri: String) -> BackendFuture<u32> {
        Box::pin(async move {
            Self::storage(&root_uri)
                .cleanup_partials()
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn delete_track(&self, root_uri: String, relative_target: String) -> BackendFuture<bool> {
        Box::pin(async move {
            Self::storage(&root_uri)
                .delete_track(&relative_target)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn probe_mp3_transcode(&self) -> Result<(), String> {
        probe_mp3_transcode_capability().map_err(|error| error.to_string())
    }

    fn transcode_track(
        &self,
        request: Mp3TranscodeRequest,
        cancelled: Arc<AtomicBool>,
    ) -> BackendFuture<TranscodedFile> {
        Box::pin(async move {
            let (sender, receiver) = async_channel::bounded(1);
            std::thread::Builder::new()
                .name("reprise-smoke-device-mp3-encoder".into())
                .spawn(move || {
                    let output = request.output.clone();
                    let result =
                        transcode_to_mp3(&request, &cancelled).map_err(|error| error.to_string());
                    if sender.send_blocking(result).is_err() {
                        let _ = std::fs::remove_file(output);
                    }
                })
                .map_err(|error| error.to_string())?;
            receiver
                .recv()
                .await
                .map_err(|_| "MP3 encoder stopped without a result".to_string())?
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
            Self::storage(&root_uri)
                .replace_playlist(&name, contents)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

pub(in crate::ui) fn runtime_from_env(
    conn: &Rc<RefCell<Connection>>,
) -> Option<Rc<DeviceSyncRuntime>> {
    let root = std::env::var_os(ROOT_ENV).map(PathBuf::from)?;
    let backend = Rc::new(SmokeDeviceBackend::for_root(&root)?);
    Some(DeviceSyncRuntime::with_backend(conn, backend))
}

pub(in crate::ui) fn arm(runtime: &Rc<DeviceSyncRuntime>) {
    if std::env::var_os(ROOT_ENV).is_none() {
        return;
    }
    let Some(ids) = std::env::var(IDS_ENV)
        .ok()
        .and_then(|value| parse_ids(&value))
    else {
        tracing::warn!("device sync smoke not armed: no valid track ids");
        return;
    };
    let playlist = std::env::var(PLAYLIST_ENV).unwrap_or_else(|_| "Smoke Playlist".into());
    let started = Rc::new(Cell::new(false));
    let log_subscription = runtime.subscribe(Rc::new(move |state| {
        if let Some(device) = state.devices.iter().find(|device| device.id == DEVICE_ID) {
            tracing::info!(
                phase = ?device.snapshot.phase,
                file = ?device.snapshot.current_name,
                current_bytes = device.snapshot.current_bytes,
                completed_tracks = device.snapshot.completed_tracks,
                queued_jobs = device.snapshot.queued_jobs,
                "device sync smoke progress"
            );
        }
    }));
    retain_until_terminal(runtime, log_subscription, &started);

    let runtime = runtime.clone();
    let started_for_enqueue = started.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_secs(2), move || {
        let split = ids.len().div_ceil(2);
        let first = &ids[..split];
        let second = &ids[split..];
        let first_result = runtime.enqueue(DEVICE_ID, &playlist, first);
        let second_result = if second.is_empty() {
            Ok(0)
        } else {
            runtime.enqueue(DEVICE_ID, &playlist, second)
        };
        started_for_enqueue.set(first_result.is_ok() && second_result.is_ok());
        tracing::info!(?first_result, ?second_result, "device sync smoke enqueued");
    });
}

fn retain_until_terminal(
    runtime: &Rc<DeviceSyncRuntime>,
    subscription: Subscription,
    started: &Rc<Cell<bool>>,
) {
    let runtime = Rc::downgrade(runtime);
    let subscription = Rc::new(RefCell::new(Some(subscription)));
    let started = started.clone();
    gtk4::glib::timeout_add_local(Duration::from_millis(200), move || {
        let Some(runtime) = runtime.upgrade() else {
            subscription.borrow_mut().take();
            return gtk4::glib::ControlFlow::Break;
        };
        let terminal = runtime.devices().into_iter().any(|device| {
            device.id == DEVICE_ID
                && device.snapshot.queued_jobs == 0
                && matches!(
                    device.snapshot.phase,
                    SyncPhase::Complete | SyncPhase::Failed
                )
        });
        if started.get() && terminal {
            subscription.borrow_mut().take();
            gtk4::glib::ControlFlow::Break
        } else {
            gtk4::glib::ControlFlow::Continue
        }
    });
}

fn safe_smoke_root(root: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let temp = std::env::temp_dir().canonicalize().ok()?;
    (root != temp && root.starts_with(temp) && root.is_dir()).then_some(root)
}

fn parse_ids(value: &str) -> Option<Vec<i64>> {
    let ids = value
        .split(',')
        .map(str::trim)
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (!ids.is_empty() && ids.iter().all(|id| *id > 0)).then_some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_guard_accepts_only_an_existing_child_of_the_temp_directory() {
        let root = tempfile::tempdir().unwrap();
        assert!(safe_smoke_root(root.path()).is_some());
        assert!(safe_smoke_root(Path::new("/")).is_none());
        assert!(safe_smoke_root(&root.path().join("missing")).is_none());
    }

    #[test]
    fn smoke_track_ids_must_be_nonempty_positive_integers() {
        assert_eq!(parse_ids("1, 2,3"), Some(vec![1, 2, 3]));
        assert!(parse_ids("").is_none());
        assert!(parse_ids("0,1").is_none());
        assert!(parse_ids("a,1").is_none());
    }
}
