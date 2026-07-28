//! Deterministic MTP-phone simulation for isolated synchronization E2E runs.
//!
//! The simulator substitutes the production MTP/GIO backend at its application
//! boundary while keeping the real storage, transcode, inventory, playlist,
//! progress, cancellation, and post-sync readback code paths. Its storage is
//! an explicitly guarded temporary directory, so it needs neither USB
//! hardware nor access to the user's library or Reprise database.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use reprise_core::device_sync::DeviceStorageInspection;
use reprise_platform_linux::device_sync::{CopyOutcome, DeviceDescriptor, DeviceStorage};
use reprise_platform_linux::device_transfer::{
    probe_transcode_capability, transcode_audio, TranscodeProfile, TranscodeRequest, TranscodedFile,
};
use rusqlite::Connection;

use super::device_sync_runtime::{BackendFuture, DeviceBackend, DeviceSyncRuntime, Subscription};

pub(in crate::ui) const ROOT_ENV: &str = "REPRISE_SMOKE_DEVICE_ROOT";
const PLAYLIST_ENV: &str = "REPRISE_SMOKE_DEVICE_PLAYLIST";
const UI_ONLY_ENV: &str = "REPRISE_SMOKE_DEVICE_UI_ONLY";
pub(in crate::ui) const DEVICE_ID: &str = "reprise-smoke-device";
pub(in crate::ui) const DEVICE_NAME: &str = "Simulated MTP Phone";

pub(in crate::ui) struct SimulatedMtpDeviceBackend {
    descriptors: Vec<DeviceDescriptor>,
}

impl SimulatedMtpDeviceBackend {
    pub(in crate::ui) fn for_root(root: &Path) -> Option<Self> {
        Self::for_devices(vec![(
            DEVICE_ID.into(),
            DEVICE_NAME.into(),
            root.to_path_buf(),
        )])
    }

    pub(in crate::ui) fn for_devices(devices: Vec<(String, String, PathBuf)>) -> Option<Self> {
        let mut ids = std::collections::HashSet::new();
        let mut roots = std::collections::HashSet::new();
        let mut descriptors = Vec::with_capacity(devices.len());
        for (id, name, root) in devices {
            let root = safe_smoke_root(&root)?;
            if id.trim().is_empty()
                || name.trim().is_empty()
                || !ids.insert(id.clone())
                || !roots.insert(root.clone())
            {
                return None;
            }
            descriptors.push(DeviceDescriptor {
                id,
                name,
                root_uri: gio::File::for_path(root).uri().into(),
                icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
                reconnectable: true,
            });
        }
        (!descriptors.is_empty()).then_some(Self { descriptors })
    }

    fn storage(root_uri: &str) -> DeviceStorage {
        DeviceStorage::from_uri(root_uri)
    }
}

impl DeviceBackend for SimulatedMtpDeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        self.descriptors.clone()
    }

    fn subscribe_devices(&self, _callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {}

    fn inspect(&self, root_uri: String) -> BackendFuture<DeviceStorageInspection> {
        Box::pin(async move {
            let storage = Self::storage(&root_uri);
            storage.inspect().await.map_err(|error| error.to_string())
        })
    }

    fn replace_track(
        &self,
        _device_id: String,
        root_uri: String,
        target_path: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async move {
            Self::storage(&root_uri)
                .replace_managed(
                    &target_path,
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

    fn cleanup_partials(&self, root_uri: String, target_path: String) -> BackendFuture<u32> {
        Box::pin(async move {
            Self::storage(&root_uri)
                .cleanup_partials_in(&target_path)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn delete_track(
        &self,
        root_uri: String,
        target_path: String,
        relative_target: String,
    ) -> BackendFuture<bool> {
        Box::pin(async move {
            Self::storage(&root_uri)
                .delete_managed(&target_path, &relative_target)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn probe_transcode(&self, profile: TranscodeProfile) -> Result<(), String> {
        probe_transcode_capability(profile).map_err(|error| error.to_string())
    }

    fn transcode_track(
        &self,
        request: TranscodeRequest,
        cancelled: Arc<AtomicBool>,
    ) -> BackendFuture<TranscodedFile> {
        Box::pin(async move {
            let (sender, receiver) = async_channel::bounded(1);
            std::thread::Builder::new()
                .name("reprise-smoke-device-audio-encoder".into())
                .spawn(move || {
                    let output = request.output.clone();
                    let result =
                        transcode_audio(&request, &cancelled).map_err(|error| error.to_string());
                    if sender.send_blocking(result).is_err() {
                        let _ = std::fs::remove_file(output);
                    }
                })
                .map_err(|error| error.to_string())?;
            receiver
                .recv()
                .await
                .map_err(|_| "audio encoder stopped without a result".to_string())?
        })
    }

    fn replace_playlist(
        &self,
        _device_id: String,
        root_uri: String,
        target_path: String,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()> {
        Box::pin(async move {
            Self::storage(&root_uri)
                .replace_playlist(&target_path, &name, contents)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

pub(in crate::ui) fn runtime_from_env(
    conn: &Rc<RefCell<Connection>>,
) -> Option<Rc<DeviceSyncRuntime>> {
    let root = std::env::var_os(ROOT_ENV).map(PathBuf::from)?;
    let backend = Rc::new(SimulatedMtpDeviceBackend::for_root(&root)?);
    Some(DeviceSyncRuntime::with_backend(conn, backend))
}

pub(in crate::ui) fn arm(runtime: &Rc<DeviceSyncRuntime>) {
    if std::env::var_os(ROOT_ENV).is_none() {
        return;
    }
    let playlist = std::env::var(PLAYLIST_ENV).unwrap_or_else(|_| "Smoke Playlist".into());
    let started = Rc::new(Cell::new(false));
    let log_subscription = runtime.subscribe(Rc::new(move |state| {
        if let Some(device) = state.devices.iter().find(|device| device.id == DEVICE_ID) {
            tracing::info!(
                phase = ?device.sync_phase,
                changes = device.page.changes.additions
                    + device.page.changes.replacements
                    + device.page.changes.removals,
                bytes_per_second = device.bytes_per_second,
                "device sync smoke progress"
            );
        }
    }));
    retain_until_terminal(runtime, log_subscription, &started);
    if std::env::var_os(UI_ONLY_ENV).is_some() {
        tracing::info!("device sync smoke is waiting for UI actions");
        return;
    }

    let runtime = runtime.clone();
    let started_for_sync = started.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_secs(2), move || {
        started_for_sync.set(true);
        let options = match runtime.selection_options() {
            Ok(options) => options,
            Err(error) => {
                tracing::warn!(%error, "device sync smoke could not list playlists");
                return;
            }
        };
        let mut matches = options.into_iter().filter(|option| option.name == playlist);
        let Some(option) = matches.next() else {
            tracing::warn!(playlist, "device sync smoke playlist does not exist");
            return;
        };
        if matches.next().is_some() {
            tracing::warn!(playlist, "device sync smoke playlist name is ambiguous");
            return;
        }
        let result = runtime
            .set_playlist_selected(DEVICE_ID, option.source, true)
            .and_then(|()| {
                runtime
                    .sync_now(DEVICE_ID)
                    .map_err(|error| error.to_string())
            });
        tracing::info!(?result, "device sync smoke started");
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
                && device.sync_phase == super::device_sync_runtime::PlannedSyncPhase::Idle
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
    fn smoke_backend_exposes_a_connected_simulated_mtp_phone() {
        let root = tempfile::tempdir().unwrap();
        let backend = SimulatedMtpDeviceBackend::for_root(root.path()).unwrap();
        let devices = backend.devices();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Simulated MTP Phone");
        assert!(devices[0].reconnectable);
        assert_eq!(devices[0].root_uri, gio::File::for_path(root.path()).uri());
    }

    #[test]
    fn simulator_projects_multiple_independent_mtp_phones() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let backend = SimulatedMtpDeviceBackend::for_devices(vec![
            (
                "simulated-phone-a".into(),
                "Simulated Phone A".into(),
                first.path().to_path_buf(),
            ),
            (
                "simulated-phone-b".into(),
                "Simulated Phone B".into(),
                second.path().to_path_buf(),
            ),
        ])
        .unwrap();

        let devices = backend.devices();
        assert_eq!(
            devices
                .iter()
                .map(|device| (device.id.as_str(), device.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("simulated-phone-a", "Simulated Phone A"),
                ("simulated-phone-b", "Simulated Phone B"),
            ]
        );
        assert_ne!(devices[0].root_uri, devices[1].root_uri);
    }
}
