use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::path::{Component, Path};
use std::rc::Rc;

use gio::prelude::*;
use reprise_core::device_sync::safe_component;
use reprise_core::library::m3u::{parse_m3u, M3uEntry};

pub use reprise_core::device_sync::{DeviceStorageInspection, DeviceStorageSnapshot};

const ENUMERATE_ATTRIBUTES: &str = "standard::name,standard::type,standard::size";
const ENUMERATE_BATCH_SIZE: i32 = 64;
const MANAGED_ROOT: [&str; 2] = ["Music", "Reprise"];
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Copied,
    Skipped,
}

#[derive(Debug)]
pub enum DeviceIoError {
    InvalidRelativePath,
    SizeMismatch { expected: u64, actual: u64 },
    Io(gio::glib::Error),
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

    /// Removes transfer remnants left by a disconnect or process exit. Only
    /// files below `Music/Reprise` with the dedicated `.part` suffix are
    /// touched; unrelated device content remains outside our ownership.
    pub async fn cleanup_partials(&self) -> Result<u32, DeviceIoError> {
        let storage = self.storage_root().await?;
        let managed_root = Self::managed_child(&storage, &[]);
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

    /// Deletes one Reprise-managed device track. A missing target is already
    /// in the desired state and is reported as `false`.
    pub async fn delete_track(&self, relative_path: &str) -> Result<bool, DeviceIoError> {
        let components = safe_relative_components(relative_path)?;
        let storage = self.storage_root().await?;
        let target = Self::managed_child(&storage, &components);
        match target.delete_future(gio::glib::Priority::DEFAULT).await {
            Ok(()) => Ok(true),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn copy_track<P>(
        &self,
        source: &gio::File,
        relative_path: &str,
        expected_size: u64,
        cancellable: &gio::Cancellable,
        progress: P,
    ) -> Result<CopyOutcome, DeviceIoError>
    where
        P: FnMut(u64, u64) + 'static,
    {
        self.transfer_track(
            source,
            relative_path,
            expected_size,
            cancellable,
            progress,
            true,
        )
        .await
    }

    /// Copies a track selected by a fresh DB delta, replacing any existing
    /// target even when the byte count happens to be unchanged.
    pub async fn replace_track<P>(
        &self,
        source: &gio::File,
        relative_path: &str,
        expected_size: u64,
        cancellable: &gio::Cancellable,
        progress: P,
    ) -> Result<CopyOutcome, DeviceIoError>
    where
        P: FnMut(u64, u64) + 'static,
    {
        self.transfer_track(
            source,
            relative_path,
            expected_size,
            cancellable,
            progress,
            false,
        )
        .await
    }

    async fn transfer_track<P>(
        &self,
        source: &gio::File,
        relative_path: &str,
        expected_size: u64,
        cancellable: &gio::Cancellable,
        progress: P,
        skip_matching_size: bool,
    ) -> Result<CopyOutcome, DeviceIoError>
    where
        P: FnMut(u64, u64) + 'static,
    {
        let components = safe_relative_components(relative_path)?;
        let storage = self.storage_root().await?;
        self.ensure_managed_directories(&storage, &components[..components.len() - 1])
            .await?;
        let target = Self::managed_child(&storage, &components);
        if skip_matching_size && target_size(&target).await? == Some(expected_size) {
            return Ok(CopyOutcome::Skipped);
        }
        let target_name = components.last().expect("validated nonempty path");
        let partial_components = components[..components.len() - 1]
            .iter()
            .cloned()
            .chain([format!("{target_name}{PARTIAL_SUFFIX}")])
            .collect::<Vec<_>>();
        let partial = Self::managed_child(&storage, &partial_components);
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
        playlist: &str,
        contents: Vec<u8>,
    ) -> Result<(), DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        let storage = self.storage_root().await?;
        self.ensure_managed_directories(&storage, &[]).await?;
        let final_file = Self::managed_child(&storage, &[format!("{playlist}.m3u8")]);
        let partial = Self::managed_child(&storage, &[format!("{playlist}.m3u8{PARTIAL_SUFFIX}")]);
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

    pub async fn read_playlist(&self, playlist: &str) -> Result<Vec<M3uEntry>, DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        let storage = self.storage_root().await?;
        let file = Self::managed_child(&storage, &[format!("{playlist}.m3u8")]);
        match file.load_contents_future().await {
            Ok((bytes, _)) => Ok(parse_m3u(&String::from_utf8_lossy(&bytes))),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    async fn ensure_managed_directories(
        &self,
        storage: &gio::File,
        relative_directories: &[String],
    ) -> Result<(), DeviceIoError> {
        let mut current = storage.clone();
        for component in MANAGED_ROOT
            .iter()
            .map(|value| (*value).to_string())
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

    /// `<storage>/Music/Reprise/<relative…>`. Takes the storage root resolved
    /// by [`Self::storage_root`] rather than reaching for `self.root`, which
    /// on MTP is the (unwritable) volume list.
    fn managed_child(storage: &gio::File, relative_components: &[String]) -> gio::File {
        MANAGED_ROOT
            .iter()
            .map(|component| (*component).to_string())
            .chain(relative_components.iter().cloned())
            .fold(storage.clone(), |parent, component| parent.child(component))
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
