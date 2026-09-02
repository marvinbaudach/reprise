use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gio::prelude::*;
use reprise_core::device_sync::safe_component;
use reprise_core::device_sync::StorageId;
use reprise_core::library::m3u::{parse_m3u, M3uEntry};

pub use reprise_core::device_sync::{DeviceStorageInspection, DeviceStorageSnapshot};

#[path = "device_sync_directories.rs"]
mod directories;
#[path = "device_sync_errors.rs"]
mod errors;
#[path = "device_sync_identity.rs"]
mod identity;
#[path = "device_sync_paths.rs"]
mod paths;
#[path = "device_sync_projection.rs"]
mod projection;
#[path = "device_sync_read.rs"]
mod read;
use directories::{child_of, ensure_directory};
pub use errors::{CopyOutcome, DeviceIoError, WriteStep};
pub use identity::{
    descriptor_from_mount, is_placeholder_name, project_descriptor, usb_facts_for_address,
    usb_serial_from_sysfs, DeviceDescriptor, UsbFacts,
};
#[cfg(test)]
pub(crate) use identity::{mount_display_name, usb_serial_from_volume_identifier};

const ENUMERATE_ATTRIBUTES: &str = "standard::name,standard::type,standard::size";
const ENUMERATE_BATCH_SIZE: i32 = 64;
const PARTIAL_SUFFIX: &str = ".part";

type DeviceCallback = Rc<dyn Fn(Vec<DeviceDescriptor>)>;

struct ResolvedDirectories {
    /// Every adopted component from the storage root down, for `child_of`.
    components: Vec<String>,
    /// Only the adopted components below the sync target folder.
    relative: Vec<String>,
}

#[derive(Clone)]
pub struct DeviceMonitor {
    monitor: gio::VolumeMonitor,
    callbacks: Rc<RefCell<Vec<DeviceCallback>>>,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        let monitor = gio::VolumeMonitor::get();
        let callbacks = Rc::new(RefCell::new(Vec::new()));
        let signal_callbacks = callbacks.clone();
        monitor.connect_mount_added(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        let signal_callbacks = callbacks.clone();
        monitor.connect_mount_changed(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        let signal_callbacks = callbacks.clone();
        monitor.connect_mount_removed(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        // Devices are projected volume-first (see `projected_devices`), so a
        // volume appearing/vanishing — or gaining its mount — must re-notify
        // even when no top-level mount event fires alongside it.
        let signal_callbacks = callbacks.clone();
        monitor.connect_volume_added(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        let signal_callbacks = callbacks.clone();
        monitor.connect_volume_changed(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        let signal_callbacks = callbacks.clone();
        monitor.connect_volume_removed(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        Self { monitor, callbacks }
    }

    pub fn devices(&self) -> Vec<DeviceDescriptor> {
        projected_devices(&self.monitor)
    }

    pub fn subscribe(&self, callback: DeviceCallback) {
        callback(self.devices());
        self.callbacks.borrow_mut().push(callback);
    }

    /// Ejects or unmounts the matching MTP device. Returns `false` when the device disappeared between the UI action and this lookup.
    pub async fn eject(&self, id: &str) -> Result<bool, DeviceIoError> {
        let mount = self.monitor.mounts().into_iter().find(|mount| {
            descriptor_from_mount(mount).is_some_and(|descriptor| descriptor.id == id)
        });
        let Some(mount) = mount else {
            return Ok(false);
        };
        if mount.can_eject() {
            mount
                .eject_with_operation_future(
                    gio::MountUnmountFlags::NONE,
                    None::<&gio::MountOperation>,
                )
                .await?;
        } else if mount.can_unmount() {
            mount
                .unmount_with_operation_future(
                    gio::MountUnmountFlags::NONE,
                    None::<&gio::MountOperation>,
                )
                .await?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}

impl Default for DeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Enumerates MTP devices **volume-first**, the way GNOME apps are expected to: GVfs models a phone as a `GProxyVolume` ("Pixel 10 Pro XL",
/// themed `[phone]` icon, `activation_root=mtp://…`) whose mount is a `GProxyShadowMount` attached to the volume, while the underlying
/// `GDaemonMount` is a top-level mount named just "mtp" with a multimedia-player icon and the SHADOWED flag set. Enumerating raw mounts is
/// therefore order-dependent: depending on when the proxy monitor registers, `monitor.mounts()` can contain both entries (the shadowed one
/// used to win and label a Pixel "mtp"), or only the shadowed daemon mount (filtering it left zero devices). At startup, the volume-to-mount
/// link and shadow flag can both arrive after the volume and mount themselves. Their matching root URIs are available immediately, so that
/// stable relationship decides ownership. The volume remains the source of identity, name, and icon; unshadowed `mtp://` mounts with no
/// matching volume root remain a fallback for exotic backends.
fn projected_devices(monitor: &gio::VolumeMonitor) -> Vec<DeviceDescriptor> {
    let mut volume_projections = Vec::new();
    let mut volume_descriptors = Vec::new();
    for volume in monitor.volumes() {
        let Some(root) = volume.activation_root() else {
            continue;
        };
        let root_uri = root.uri();
        if !root_uri.starts_with("mtp://") {
            continue;
        }
        let uuid = volume.uuid();
        let unix_device = volume.identifier(gio::VOLUME_IDENTIFIER_KIND_UNIX_DEVICE);
        let facts = identity::usb_facts_from_volume_identifier(
            unix_device.as_deref(),
            &root_uri,
            Path::new("/sys/bus/usb/devices"),
        );
        let Some(mut descriptor) =
            project_descriptor(&root_uri, uuid.as_deref(), &facts, &volume.name())
        else {
            continue;
        };
        descriptor.icon = volume.icon();
        volume_projections.push(projection::VolumeProjection {
            name: descriptor.name.clone(),
            root_uri: descriptor.root_uri.clone(),
            persistent_id: descriptor.persistent_id.clone(),
        });
        volume_descriptors.push(descriptor);
    }

    let mut mount_projections = Vec::new();
    let mut mount_descriptors = Vec::new();
    for mount in monitor.mounts() {
        let root_uri = mount.root().uri().to_string();
        if !root_uri.starts_with("mtp://") {
            continue;
        }
        let shadowed = mount.is_shadowed();
        let descriptor = if shadowed {
            None
        } else {
            descriptor_from_mount(&mount)
        };
        mount_projections.push(projection::MountProjection {
            name: descriptor
                .as_ref()
                .map_or_else(|| mount.name().to_string(), |value| value.name.clone()),
            root_uri,
            persistent_id: descriptor
                .as_ref()
                .and_then(|value| value.persistent_id.clone()),
            shadowed,
        });
        mount_descriptors.push(descriptor);
    }

    for volume in &volume_projections {
        let mounted = mount_projections
            .iter()
            .any(|mount| mount.root_uri == volume.root_uri);
        tracing::debug!(
            name = %volume.name,
            root = %volume.root_uri,
            mounted,
            "device sync: MTP volume observed"
        );
    }

    let devices = projection::project_devices(&volume_projections, &mount_projections)
        .into_iter()
        .filter_map(|projected| match projected.source {
            projection::ProjectionSource::Volume(index) => volume_descriptors.get(index).cloned(),
            projection::ProjectionSource::Mount(index) => {
                let descriptor = mount_descriptors.get(index)?.clone()?;
                tracing::debug!(
                    name = %projected.name,
                    root = %projected.root_uri,
                    persistent_id = ?projected.persistent_id,
                    "device sync: unshadowed MTP mount without a volume"
                );
                Some(descriptor)
            }
        })
        .collect::<Vec<_>>();
    tracing::debug!(count = devices.len(), "device sync: projected MTP devices");
    devices
}

fn notify_subscribers(
    monitor: &gio::VolumeMonitor,
    subscribers: &Rc<RefCell<Vec<DeviceCallback>>>,
) {
    let devices = projected_devices(monitor);
    let subscribers = subscribers.borrow().clone();
    for subscriber in subscribers {
        subscriber(devices.clone());
    }
}

#[path = "device_sync_inspection.rs"]
mod inspection;
#[path = "device_sync_browser.rs"]
mod target_browser;

use paths::{is_audio_file, join_relative, safe_relative_components, safe_target_components};

#[derive(Clone)]
pub struct DeviceStorage {
    root: gio::File,
    /// Cached result of [`Self::storage_root`] — resolving it enumerates the device root, which is a round-trip worth doing once per instance.
    storage: RefCell<Option<gio::File>>,
}

impl DeviceStorage {
    pub fn from_root(root: &gio::File) -> Self {
        Self {
            root: root.clone(),
            storage: RefCell::new(None),
        }
    }

    pub fn from_uri(uri: &str) -> Self {
        Self::from_root(&gio::File::for_uri(uri))
    }

    /// The directory that holds `Music/Reprise`.
    ///
    /// Android MTP does not expose a filesystem at the device root: the root lists *storage volumes* ("Internal shared storage", plus an SD
    /// card on some devices) and is itself read-only, so creating `Music` there fails with "Cannot make directory in this location" — which
    /// made every copy fail. The managed folder must live inside a storage volume, which is also where every other app (and the phone's own
    /// media scanner) expects `Music/` to be.
    ///
    /// Only `mtp://` roots are resolved this way; other roots (the local directories the tests use)
    /// already are the storage and are returned unchanged.
    async fn storage_root(&self) -> Result<gio::File, DeviceIoError> {
        if let Some(cached) = self.storage.borrow().clone() {
            return Ok(cached);
        }
        if self.root.uri_scheme().as_deref() != Some("mtp") {
            return Ok(self.root.clone());
        }
        let enumerator = self
            .root
            .enumerate_children_future(
                ENUMERATE_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await?;
        let mut volumes = Vec::new();
        loop {
            let batch = enumerator
                .next_files_future(ENUMERATE_BATCH_SIZE, gio::glib::Priority::DEFAULT)
                .await?;
            if batch.is_empty() {
                break;
            }
            for info in batch {
                if info.file_type() == gio::FileType::Directory {
                    volumes.push(info.name().to_string_lossy().into_owned());
                }
            }
        }
        // Prefer the internal storage when a card is also present; otherwise
        // take the only volume. A root without volumes is left as-is so the
        // caller still gets a sensible (if failing) error from the operation.
        let chosen = choose_storage_volume(&volumes);
        let resolved = match chosen {
            Some(name) => {
                tracing::debug!(volume = %name, "device sync: resolved MTP storage volume");
                self.root.child(name)
            }
            None => {
                tracing::warn!("device sync: MTP root exposes no storage volume");
                self.root.clone()
            }
        };
        *self.storage.borrow_mut() = Some(resolved.clone());
        Ok(resolved)
    }

    /// The storage volume one sync target's I/O actually runs against (`MTP-23`): the explicit `storage_id` the folder browser resolved and
    /// persisted for it (`MTP-31`/`MTP-32`), re-resolved fresh — MTP handles are not stable across reconnects, see the module docs — or, for a
    /// target that has never been repointed (`storage_id` still `None`), the same "prefer internal, else the only volume" default
    /// [`Self::storage_root`] always used before the folder browser existed. Every transfer and inspection call routes through this so a
    /// target's persisted choice is what receives the bytes, not whatever the default would guess.
    async fn resolve_target_storage(
        &self,
        storage_id: Option<StorageId>,
    ) -> Result<gio::File, DeviceIoError> {
        match storage_id {
            Some(storage_id) => self.resolve_storage_root(storage_id).await,
            None => self.storage_root().await,
        }
    }

    /// Removes transfer remnants left by a disconnect or process exit under the sync target's folder (`target_path`, `MTP-23`). Only files
    /// below that folder with the dedicated `.part` suffix are touched; every other file on the device remains outside our ownership.
    pub async fn cleanup_partials_in(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        partial_paths: &[String],
    ) -> Result<u32, DeviceIoError> {
        let storage = self.resolve_target_storage(storage_id).await?;
        let managed_root = Self::managed_child(&storage, target_path, &[])?;
        let mut removed = 0_u32;
        for relative_path in partial_paths {
            if !relative_path.ends_with(PARTIAL_SUFFIX) {
                continue;
            }
            let Ok(components) = safe_relative_components(relative_path) else {
                tracing::warn!(
                    path = %relative_path,
                    "device sync: ignored an invalid listed partial path"
                );
                continue;
            };
            let partial = components
                .iter()
                .fold(managed_root.clone(), |parent, component| {
                    parent.child(component)
                });
            match partial.delete_future(gio::glib::Priority::DEFAULT).await {
                Ok(()) => removed = removed.saturating_add(1),
                Err(error) if error.matches(gio::IOErrorEnum::NotFound) => {}
                Err(error) => {
                    warn_cleanup_failure(&managed_root, &partial, &error, "delete partial file");
                }
            }
        }
        Ok(removed)
    }

    /// Deletes one file under a sync target's folder (`target_path`, `MTP-23`). A missing target is already in the desired state and is reported as `false`.
    pub async fn delete_managed(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        relative_path: &str,
    ) -> Result<bool, DeviceIoError> {
        let components = safe_relative_components(relative_path)?;
        let storage = self.resolve_target_storage(storage_id).await?;
        let target = Self::managed_child(&storage, target_path, &components)?;
        match target.delete_future(gio::glib::Priority::DEFAULT).await {
            Ok(()) => Ok(true),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Copies (or overwrites) one file under a sync target's folder (`target_path`, `MTP-23`), always replacing any existing file at the
    /// destination even when its byte count happens to be unchanged.
    #[allow(clippy::too_many_arguments)]
    pub async fn replace_managed<P>(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        source: &gio::File,
        relative_path: &str,
        expected_size: u64,
        cancellable: &gio::Cancellable,
        progress: P,
    ) -> Result<CopyOutcome, DeviceIoError>
    where
        P: FnMut(u64, u64) + 'static,
    {
        let components = safe_relative_components(relative_path)?;
        let storage = self
            .resolve_target_storage(storage_id)
            .await
            .map_err(|error| error.during(WriteStep::ResolveStorage))?;
        let directories = self
            .ensure_managed_directories(&storage, target_path, &components[..components.len() - 1])
            .await
            .map_err(|error| error.during(WriteStep::CreateDirectories))?;
        let directory = child_of(&storage, &directories.components);
        let target_name = components.last().expect("validated nonempty path");
        let target = directory.child(target_name);
        let mut resolved_path = directories.relative;
        resolved_path.push(target_name.clone());
        let progress = Rc::new(RefCell::new(progress));
        let callback_progress = progress.clone();
        let (sender, receiver) = async_channel::bounded(1);
        source.copy_async(
            &target,
            gio::FileCopyFlags::OVERWRITE,
            gio::glib::Priority::DEFAULT,
            Some(cancellable),
            Some(Box::new(move |copied, total| {
                (callback_progress.borrow_mut())(copied.max(0) as u64, total.max(0) as u64);
            })),
            move |result| {
                let _ = sender.try_send(result);
            },
        );
        let copied = match receiver.recv().await {
            Ok(copied) => copied,
            Err(_) => {
                delete_if_present(&target).await;
                return Err(DeviceIoError::InvalidRelativePath.during(WriteStep::CopyTarget));
            }
        };
        finish_managed_copy(&target, cancellable, copied, expected_size).await?;
        (progress.borrow_mut())(expected_size, expected_size);
        Ok(CopyOutcome::Copied {
            relative_path: resolved_path.join("/"),
        })
    }

    pub async fn replace_playlist(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        playlist: &str,
        contents: Vec<u8>,
    ) -> Result<(), DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        let storage = self
            .resolve_target_storage(storage_id)
            .await
            .map_err(|error| error.during(WriteStep::CreateDirectories))?;
        let directories = self
            .ensure_managed_directories(&storage, target_path, &[])
            .await
            .map_err(|error| error.during(WriteStep::CreateDirectories))?;
        let directory = child_of(&storage, &directories.components);
        let final_file = directory.child(format!("{playlist}.m3u8"));
        let partial = directory.child(format!("{playlist}.m3u8{PARTIAL_SUFFIX}"));
        let expected_size = contents.len() as u64;
        partial
            .replace_contents_future(
                contents,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await
            .map_err(|(_, error)| DeviceIoError::Io(error).during(WriteStep::Publish))?;
        // A rewritten playlist always overwrites its predecessor, so this is
        // the path that meets the broken rename on every single run.
        publish(&partial, &final_file, expected_size).await
    }

    pub async fn read_playlist(
        &self,
        target_path: &str,
        playlist: &str,
    ) -> Result<Vec<M3uEntry>, DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        let storage = self.storage_root().await?;
        let file = Self::managed_child(&storage, target_path, &[format!("{playlist}.m3u8")])?;
        match file.load_contents_future().await {
            Ok((bytes, _)) => Ok(parse_m3u(&String::from_utf8_lossy(&bytes))),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// Creates every directory above a managed file and reports the spelling each
    /// one actually has on the device.
    ///
    /// The returned components replace the desired ones for the file itself:
    /// gvfs matches MTP folder names exactly, so adopting a resident spelling
    /// without rebuilding the file path underneath it only moves the failure one
    /// step later.
    async fn ensure_managed_directories(
        &self,
        storage: &gio::File,
        target_path: &str,
        relative_directories: &[String],
    ) -> Result<ResolvedDirectories, DeviceIoError> {
        let mut current = storage.clone();
        let mut components = Vec::new();
        for component in safe_target_components(target_path)? {
            let component = ensure_directory(&current, component).await?;
            current = current.child(&component);
            components.push(component);
        }
        let mut relative = Vec::new();
        for component in relative_directories.iter().cloned() {
            let component = ensure_directory(&current, component).await?;
            current = current.child(&component);
            components.push(component.clone());
            relative.push(component);
        }
        Ok(ResolvedDirectories {
            components,
            relative,
        })
    }

    /// `<storage>/<target_path>/<relative…>`, e.g. `<storage>/Music/Selected/<relative…>`. Takes the
    /// storage root resolved by [`Self::storage_root`] rather than reaching for `self.root`, which on
    /// MTP is the (unwritable) volume list.
    fn managed_child(
        storage: &gio::File,
        target_path: &str,
        relative_components: &[String],
    ) -> Result<gio::File, DeviceIoError> {
        Ok(safe_target_components(target_path)?
            .into_iter()
            .chain(relative_components.iter().cloned())
            .fold(storage.clone(), |parent, component| parent.child(component)))
    }
}

fn warn_cleanup_failure(
    root: &gio::File,
    path: &gio::File,
    error: &gio::glib::Error,
    action: &str,
) {
    let path = root.relative_path(path).map_or_else(
        || path.uri().to_string(),
        |path| path.to_string_lossy().into_owned(),
    );
    tracing::warn!(path = %path, error = %error, action, "device sync: could not clean partial files");
}

fn choose_storage_volume(volumes: &[String]) -> Option<String> {
    volumes
        .iter()
        .find(|name| name.to_lowercase().contains("internal"))
        .or_else(|| {
            volumes.iter().find(|name| {
                let name = name.to_lowercase();
                !name.contains("sd") && !name.contains("card")
            })
        })
        .or_else(|| volumes.first())
        .cloned()
}

async fn target_size(file: &gio::File) -> Result<Option<u64>, DeviceIoError> {
    match file
        .query_info_future(
            gio::FILE_ATTRIBUTE_STANDARD_SIZE,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::glib::Priority::DEFAULT,
        )
        .await
    {
        Ok(info) => Ok(Some(info.size().max(0) as u64)),
        Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn finish_managed_copy(
    target: &gio::File,
    cancellable: &gio::Cancellable,
    copied: Result<(), gio::glib::Error>,
    expected_size: u64,
) -> Result<(), DeviceIoError> {
    if let Err(error) = copied {
        delete_if_present(target).await;
        return Err(DeviceIoError::from(error).during(WriteStep::CopyTarget));
    }
    if cancellable.is_cancelled() {
        delete_if_present(target).await;
        let error = gio::glib::Error::new(gio::IOErrorEnum::Cancelled, "Operation cancelled");
        return Err(DeviceIoError::from(error).during(WriteStep::CopyTarget));
    }
    if let Err(error) = verify_file(target, expected_size, WriteStep::VerifyTarget).await {
        delete_if_present(target).await;
        return Err(error);
    }
    Ok(())
}

/// Moves a finished `.part` file onto its final name and proves it landed.
///
/// Both steps exist because of how MTP answers an overwriting rename: gvfs reports success, drops the previous file, and never applies the new
/// name. Measured over a phone, a run whose targets already existed stranded 33 of 120 transfers that way, against 0 of 120 when the targets
/// were new — so the existing file is removed first, which keeps this a plain rename. The proof afterwards covers whatever else may
/// acknowledge work it did not do: without it the audio sits under a `.part` name no media scanner reads while the inventory records the
/// track as delivered, and the next run sees nothing to repair. On failure the partial is cleared so the run leaves no debris and the missing
/// track is simply copied again next time.
async fn publish(
    partial: &gio::File,
    target: &gio::File,
    expected_size: u64,
) -> Result<(), DeviceIoError> {
    delete_if_present(target).await;
    if let Err(error) = partial
        .move_future(
            target,
            gio::FileCopyFlags::OVERWRITE,
            gio::glib::Priority::DEFAULT,
        )
        .0
        .await
    {
        delete_if_present(partial).await;
        return Err(DeviceIoError::from(error).during(WriteStep::Publish));
    }
    if let Err(error) = verify_published(target, expected_size).await {
        delete_if_present(partial).await;
        return Err(error);
    }
    Ok(())
}

/// Confirms that a published file is on the device with the bytes we sent.
async fn verify_published(file: &gio::File, expected_size: u64) -> Result<(), DeviceIoError> {
    verify_file(file, expected_size, WriteStep::Publish).await
}

async fn verify_file(
    file: &gio::File,
    expected_size: u64,
    step: WriteStep,
) -> Result<(), DeviceIoError> {
    match target_size(file).await {
        Ok(Some(actual)) if actual == expected_size => Ok(()),
        Ok(Some(actual)) => Err(DeviceIoError::SizeMismatch {
            expected: expected_size,
            actual,
        }),
        Ok(None) => Err(DeviceIoError::PublishNotApplied {
            name: file.basename().map_or_else(
                || "the device file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            ),
        }),
        Err(error) => Err(error),
    }
    .map_err(|error| error.during(step))
}

async fn delete_if_present(file: &gio::File) {
    if let Err(error) = file.delete_future(gio::glib::Priority::DEFAULT).await {
        if !error.matches(gio::IOErrorEnum::NotFound) {
            tracing::warn!(%error, "failed to remove partial device sync file");
        }
    }
}

#[cfg(test)]
#[test]
fn a_mid_copy_error_discards_the_incomplete_final_file() {
    let (temp, _storage) = tests::fixture();
    let target_path = temp.path().join("incomplete.opus");
    std::fs::write(&target_path, b"truncated").unwrap();
    let target = gio::File::for_path(&target_path);
    let copy_error = gio::glib::Error::new(gio::IOErrorEnum::Failed, "injected copy failure");

    let result = tests::run(finish_managed_copy(
        &target,
        &gio::Cancellable::new(),
        Err(copy_error),
        99,
    ));

    assert!(matches!(
        result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::CopyTarget,
            ..
        })
    ));
    assert!(!target_path.exists());
}

#[cfg(test)]
#[test]
fn cancellation_after_a_successful_copy_discards_the_final_file() {
    let (temp, _storage) = tests::fixture();
    let target_path = temp.path().join("cancelled.opus");
    std::fs::write(&target_path, b"complete").unwrap();
    let target = gio::File::for_path(&target_path);
    let cancellable = gio::Cancellable::new();
    cancellable.cancel();

    let result = tests::run(finish_managed_copy(&target, &cancellable, Ok(()), 8));

    assert!(matches!(
        result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::CopyTarget,
            ..
        })
    ));
    assert!(!target_path.exists());
}

#[cfg(test)]
#[path = "device_sync_browser_tests.rs"]
mod browser_tests;
#[cfg(test)]
#[path = "device_sync_identity_tests.rs"]
mod identity_tests;
#[cfg(test)]
#[path = "device_sync_projection_tests.rs"]
mod projection_tests;
#[cfg(test)]
#[path = "device_sync_tests.rs"]
mod tests;
