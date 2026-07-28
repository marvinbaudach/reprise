use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::path::{Component, Path};
use std::rc::Rc;

use gio::prelude::*;
use reprise_core::device_sync::safe_component;
use reprise_core::device_sync::StorageId;
use reprise_core::library::m3u::{parse_m3u, M3uEntry};

pub use reprise_core::device_sync::{DeviceStorageInspection, DeviceStorageSnapshot};

const ENUMERATE_ATTRIBUTES: &str = "standard::name,standard::type,standard::size";
const ENUMERATE_BATCH_SIZE: i32 = 64;
const PARTIAL_SUFFIX: &str = ".part";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub id: String,
    pub name: String,
    pub root_uri: String,
    pub reconnectable: bool,
    pub icon: gio::Icon,
}

pub fn project_descriptor(
    root_uri: &str,
    uuid: Option<&str>,
    name: &str,
) -> Option<DeviceDescriptor> {
    if !root_uri.starts_with("mtp://") {
        return None;
    }
    let stable_uuid = uuid.filter(|value| !value.trim().is_empty());
    Some(DeviceDescriptor {
        id: stable_uuid.unwrap_or(root_uri).to_string(),
        name: name.to_string(),
        root_uri: root_uri.to_string(),
        reconnectable: stable_uuid.is_some(),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
    })
}

pub fn descriptor_from_mount(mount: &gio::Mount) -> Option<DeviceDescriptor> {
    let root_uri = mount.root().uri();
    let uuid = mount.uuid();
    let mut descriptor = project_descriptor(&root_uri, uuid.as_deref(), &mount.name())?;
    descriptor.icon = mount.icon();
    Some(descriptor)
}

type DeviceCallback = Rc<dyn Fn(Vec<DeviceDescriptor>)>;

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

    /// Ejects or unmounts the matching MTP device. Returns `false` when the
    /// device disappeared between the UI action and this lookup.
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

/// Enumerates MTP devices **volume-first**, the way GNOME apps are expected
/// to: GVfs models a phone as a `GProxyVolume` ("Pixel 10 Pro XL", themed
/// `[phone]` icon, `activation_root=mtp://…`) whose mount is a
/// `GProxyShadowMount` attached to the volume, while the underlying
/// `GDaemonMount` is a top-level mount named just "mtp" with a
/// multimedia-player icon and the SHADOWED flag set. Enumerating raw mounts
/// is therefore order-dependent: depending on when the proxy monitor
/// registers, `monitor.mounts()` can contain both entries (the shadowed one
/// used to win and label a Pixel "mtp"), or only the shadowed daemon mount
/// (filtering it left zero devices). The volume is the stable entity, so it
/// is the source of identity, name, and icon; unshadowed `mtp://` mounts
/// that no volume claims are kept as a fallback for exotic backends.
fn projected_devices(monitor: &gio::VolumeMonitor) -> Vec<DeviceDescriptor> {
    let mut devices = Vec::new();
    let mut seen_roots = std::collections::HashSet::new();
    for volume in monitor.volumes() {
        let Some(root) = volume.activation_root() else {
            continue;
        };
        let root_uri = root.uri();
        if !root_uri.starts_with("mtp://") {
            continue;
        }
        let mounted = volume.get_mount().is_some();
        tracing::debug!(
            name = %volume.name(),
            root = %root_uri,
            mounted,
            "device sync: MTP volume observed"
        );
        // v1 shows only mounted devices (mount-on-demand is a follow-up);
        // an unmounted volume simply stays hidden, as before.
        if !mounted {
            continue;
        }
        let uuid = volume.uuid();
        let Some(mut descriptor) = project_descriptor(&root_uri, uuid.as_deref(), &volume.name())
        else {
            continue;
        };
        descriptor.icon = volume.icon();
        seen_roots.insert(root_uri.to_string());
        devices.push(descriptor);
    }
    for mount in monitor.mounts() {
        // Shadowed mounts are the volume-owned devices' plumbing (see above);
        // per g_mount_is_shadowed they must not be displayed.
        if mount.is_shadowed() {
            continue;
        }
        let Some(descriptor) = descriptor_from_mount(&mount) else {
            continue;
        };
        if seen_roots.contains(&descriptor.root_uri) {
            continue;
        }
        tracing::debug!(
            name = %descriptor.name,
            root = %descriptor.root_uri,
            "device sync: unshadowed MTP mount without a volume"
        );
        devices.push(descriptor);
    }
    devices.sort_by(|left, right| left.name.cmp(&right.name));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Copied,
}

#[derive(Debug)]
pub enum DeviceIoError {
    InvalidRelativePath,
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    Io(gio::glib::Error),
    /// Design 7d: the chosen `StorageId` no longer matches any storage
    /// volume at the device root — e.g. an SD card was removed since the
    /// browser last listed storages.
    StorageNotFound,
    /// Design 7d's "New folder": a folder with that name already exists at
    /// the chosen location.
    FolderAlreadyExists,
    /// Design 7d's root-creation error path: the device refused to create
    /// a folder directly at a storage volume's own top level.
    CannotCreateAtStorageRoot(gio::glib::Error),
}

impl fmt::Display for DeviceIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRelativePath => formatter.write_str("invalid managed device path"),
            Self::SizeMismatch { expected, actual } => write!(
                formatter,
                "partial device file has {actual} bytes, expected {expected}"
            ),
            Self::Io(error) => write!(formatter, "device I/O failed: {error}"),
            Self::StorageNotFound => {
                formatter.write_str("the selected storage is no longer available on this device")
            }
            Self::FolderAlreadyExists => {
                formatter.write_str("a folder with that name already exists here")
            }
            Self::CannotCreateAtStorageRoot(error) => write!(
                formatter,
                "this device does not allow creating folders directly in the storage root: {error}"
            ),
        }
    }
}

impl std::error::Error for DeviceIoError {}

impl From<gio::glib::Error> for DeviceIoError {
    fn from(error: gio::glib::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone)]
pub struct DeviceStorage {
    root: gio::File,
    /// Cached result of [`Self::storage_root`] — resolving it enumerates the
    /// device root, which is a round-trip worth doing once per instance.
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
    /// Android MTP does not expose a filesystem at the device root: the root
    /// lists *storage volumes* ("Internal shared storage", plus an SD card on
    /// some devices) and is itself read-only, so creating `Music` there fails
    /// with "Cannot make directory in this location" — which made every copy
    /// fail. The managed folder must live inside a storage volume, which is
    /// also where every other app (and the phone's own media scanner) expects
    /// `Music/` to be.
    ///
    /// Only `mtp://` roots are resolved this way; other roots (the local
    /// directories the tests use) already are the storage and are returned
    /// unchanged.
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

    /// The storage volume one sync target's I/O actually runs against
    /// (`MTP-18`): the explicit `storage_id` the folder browser resolved and
    /// persisted for it (`MTP-31`/`MTP-32`), re-resolved fresh — MTP handles
    /// are not stable across reconnects, see the module docs — or, for a
    /// target that has never been repointed (`storage_id` still `None`),
    /// the same "prefer internal, else the only volume" default
    /// [`Self::storage_root`] always used before the folder browser
    /// existed. Every transfer and inspection call routes through this so a
    /// target's persisted choice is what receives the bytes, not whatever
    /// the default would guess.
    async fn resolve_target_storage(
        &self,
        storage_id: Option<StorageId>,
    ) -> Result<gio::File, DeviceIoError> {
        match storage_id {
            Some(storage_id) => self.resolve_storage_root(storage_id).await,
            None => self.storage_root().await,
        }
    }

    /// Removes transfer remnants left by a disconnect or process exit under
    /// one sync target's folder (`target_path`, `MTP-18`). Only files below
    /// that folder with the dedicated `.part` suffix are touched; unrelated
    /// device content — including the other two named targets — remains
    /// outside our ownership.
    pub async fn cleanup_partials_in(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
    ) -> Result<u32, DeviceIoError> {
        let storage = self.resolve_target_storage(storage_id).await?;
        let managed_root = Self::managed_child(&storage, target_path, &[])?;
        let mut pending = VecDeque::from([managed_root]);
        let mut removed = 0_u32;
        while let Some(directory) = pending.pop_front() {
            let enumerator = match directory
                .enumerate_children_future(
                    "standard::name,standard::type",
                    gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                    gio::glib::Priority::DEFAULT,
                )
                .await
            {
                Ok(enumerator) => enumerator,
                Err(error) if error.matches(gio::IOErrorEnum::NotFound) => continue,
                Err(error) => return Err(error.into()),
            };
            loop {
                let batch = enumerator
                    .next_files_future(ENUMERATE_BATCH_SIZE, gio::glib::Priority::DEFAULT)
                    .await?;
                if batch.is_empty() {
                    break;
                }
                for info in batch {
                    let child = directory.child(info.name());
                    if info.file_type() == gio::FileType::Directory {
                        pending.push_back(child);
                    } else if info.name().to_string_lossy().ends_with(PARTIAL_SUFFIX) {
                        child.delete_future(gio::glib::Priority::DEFAULT).await?;
                        removed = removed.saturating_add(1);
                    }
                }
            }
        }
        Ok(removed)
    }

    /// Deletes one file under a sync target's folder (`target_path`,
    /// `MTP-18`). A missing target is already in the desired state and is
    /// reported as `false`.
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

    /// Copies (or overwrites) one file under a sync target's folder
    /// (`target_path`, `MTP-18`), always replacing any existing file at the
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
        let storage = self.resolve_target_storage(storage_id).await?;
        self.ensure_managed_directories(&storage, target_path, &components[..components.len() - 1])
            .await?;
        let target = Self::managed_child(&storage, target_path, &components)?;
        let target_name = components.last().expect("validated nonempty path");
        let partial_components = components[..components.len() - 1]
            .iter()
            .cloned()
            .chain([format!("{target_name}{PARTIAL_SUFFIX}")])
            .collect::<Vec<_>>();
        let partial = Self::managed_child(&storage, target_path, &partial_components)?;
        let progress = Rc::new(RefCell::new(progress));
        let callback_progress = progress.clone();
        let (sender, receiver) = async_channel::bounded(1);
        source.copy_async(
            &partial,
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
        let copied = receiver
            .recv()
            .await
            .map_err(|_| DeviceIoError::InvalidRelativePath)?;
        if let Err(error) = copied {
            delete_if_present(&partial).await;
            return Err(error.into());
        }
        let actual_size = target_size(&partial).await?.unwrap_or(0);
        if actual_size != expected_size {
            delete_if_present(&partial).await;
            return Err(DeviceIoError::SizeMismatch {
                expected: expected_size,
                actual: actual_size,
            });
        }
        if let Err(error) = partial
            .move_future(
                &target,
                gio::FileCopyFlags::OVERWRITE,
                gio::glib::Priority::DEFAULT,
            )
            .0
            .await
        {
            delete_if_present(&partial).await;
            return Err(error.into());
        }
        (progress.borrow_mut())(expected_size, expected_size);
        Ok(CopyOutcome::Copied)
    }

    pub async fn replace_playlist(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        playlist: &str,
        contents: Vec<u8>,
    ) -> Result<(), DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        let storage = self.resolve_target_storage(storage_id).await?;
        self.ensure_managed_directories(&storage, target_path, &[])
            .await?;
        let final_file = Self::managed_child(&storage, target_path, &[format!("{playlist}.m3u8")])?;
        let partial = Self::managed_child(
            &storage,
            target_path,
            &[format!("{playlist}.m3u8{PARTIAL_SUFFIX}")],
        )?;
        partial
            .replace_contents_future(
                contents,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await
            .map_err(|(_, error)| DeviceIoError::Io(error))?;
        if let Err(error) = partial
            .move_future(
                &final_file,
                gio::FileCopyFlags::OVERWRITE,
                gio::glib::Priority::DEFAULT,
            )
            .0
            .await
        {
            delete_if_present(&partial).await;
            return Err(error.into());
        }
        Ok(())
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

    async fn ensure_managed_directories(
        &self,
        storage: &gio::File,
        target_path: &str,
        relative_directories: &[String],
    ) -> Result<(), DeviceIoError> {
        let mut current = storage.clone();
        for component in safe_target_components(target_path)?
            .into_iter()
            .chain(relative_directories.iter().cloned())
        {
            current = current.child(component);
            match current
                .make_directory_future(gio::glib::Priority::DEFAULT)
                .await
            {
                Ok(()) => {}
                Err(error) if error.matches(gio::IOErrorEnum::Exists) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// `<storage>/<target_path>/<relative…>`, e.g.
    /// `<storage>/Music/Reprise-YouTube/<relative…>`. Takes the storage root
    /// resolved by [`Self::storage_root`] rather than reaching for
    /// `self.root`, which on MTP is the (unwritable) volume list.
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

/// Splits a [`reprise_core::device_sync::SyncTarget`] path (e.g.
/// `/Music/Reprise-YouTube`, `MTP-18`) into path components for building a
/// `gio::File` under the resolved storage volume. Unlike
/// [`safe_relative_components`], a single leading `Component::RootDir` is
/// accepted and dropped — sync target paths are written as absolute-looking
/// device paths, but every one of them is still resolved relative to the
/// storage volume returned by [`DeviceStorage::storage_root`].
fn safe_target_components(path: &str) -> Result<Vec<String>, DeviceIoError> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(DeviceIoError::InvalidRelativePath);
    }
    let components = Path::new(path)
        .components()
        .filter(|component| !matches!(component, Component::RootDir))
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or(DeviceIoError::InvalidRelativePath),
            _ => Err(DeviceIoError::InvalidRelativePath),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        Err(DeviceIoError::InvalidRelativePath)
    } else {
        Ok(components)
    }
}

fn safe_relative_components(path: &str) -> Result<Vec<String>, DeviceIoError> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(DeviceIoError::InvalidRelativePath);
    }
    let components = Path::new(path)
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or(DeviceIoError::InvalidRelativePath),
            _ => Err(DeviceIoError::InvalidRelativePath),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        Err(DeviceIoError::InvalidRelativePath)
    } else {
        Ok(components)
    }
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

async fn delete_if_present(file: &gio::File) {
    if let Err(error) = file.delete_future(gio::glib::Priority::DEFAULT).await {
        if !error.matches(gio::IOErrorEnum::NotFound) {
            tracing::warn!(%error, "failed to remove partial device sync file");
        }
    }
}

fn join_relative(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn is_audio_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "wav"
            )
        })
}

#[cfg(test)]
#[path = "device_sync_tests.rs"]
mod tests;
