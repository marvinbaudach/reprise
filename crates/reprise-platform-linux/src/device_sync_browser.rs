//! GVfs/MTP I/O behind the device folder browser (design 7d, `MTP-31`).
//!
//! Every operation here is the literal PTP call the design doc names —
//! `GetObjectPropList` for listing a folder's children, `SendObjectInfo` for
//! creating one — but GVfs's `mtp://` backend already performs those calls
//! underneath ordinary [`gio::File`] enumeration and
//! [`gio::File::make_directory_future`], the same primitives
//! [`DeviceStorage`]'s existing managed-file operations use. No raw PTP
//! object handle is ever read or stored: every method below is given (or
//! re-derives) a [`StorageId`] plus a path string and re-resolves the
//! actual `gio::File` fresh, matching `reprise_core::device_sync::browser`'s
//! module docs on why handles are never persisted.
//!
//! GVfs's MTP backend does not expose the raw PTP `StorageID` numeric value
//! through any standard file attribute, so [`derive_storage_id`] derives a
//! stable [`StorageId`] from the storage's GVfs-reported name instead. This
//! is stable for as long as the name is (Android does not rename "Internal
//! shared storage" or an installed SD card between sessions), which is the
//! property a persisted [`StorageId`] actually needs — bit-identical to the
//! wire-protocol value is not.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path};

use gio::prelude::*;
use reprise_core::device_sync::browser::{classify_storage_kind, StorageOption};
use reprise_core::device_sync::StorageId;

use super::{DeviceIoError, DeviceStorage, ENUMERATE_ATTRIBUTES, ENUMERATE_BATCH_SIZE};

impl DeviceStorage {
    /// Design 7d's storage selection: every browsable storage volume at
    /// this device's root (e.g. "Internal shared storage", "SD card").
    pub async fn list_storage_volumes(&self) -> Result<Vec<StorageOption>, DeviceIoError> {
        let mut options = Vec::new();
        let enumerator = self
            .root
            .enumerate_children_future(
                ENUMERATE_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await?;
        loop {
            let batch = enumerator
                .next_files_future(ENUMERATE_BATCH_SIZE, gio::glib::Priority::DEFAULT)
                .await?;
            if batch.is_empty() {
                break;
            }
            for info in batch {
                if info.file_type() != gio::FileType::Directory {
                    continue;
                }
                let name = info.name().to_string_lossy().into_owned();
                options.push(StorageOption {
                    id: derive_storage_id(&name),
                    kind: classify_storage_kind(&name),
                    name,
                });
            }
        }
        options.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(options)
    }

    /// Design 7d's folder tree: the immediate child folders of `path` on
    /// `storage_id`. `path` uses the same absolute-looking device-path
    /// syntax as [`crate::device_sync::browser`]'s other operations; the
    /// empty path (or a bare `/`) means the storage's own root.
    pub async fn list_child_folders(
        &self,
        storage_id: StorageId,
        path: &str,
    ) -> Result<Vec<String>, DeviceIoError> {
        let storage = self.resolve_storage_root(storage_id).await?;
        let directory = browse_components(path)?
            .into_iter()
            .fold(storage, |parent, component| parent.child(component));
        let mut folders = Vec::new();
        let enumerator = directory
            .enumerate_children_future(
                ENUMERATE_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await?;
        loop {
            let batch = enumerator
                .next_files_future(ENUMERATE_BATCH_SIZE, gio::glib::Priority::DEFAULT)
                .await?;
            if batch.is_empty() {
                break;
            }
            for info in batch {
                if info.file_type() == gio::FileType::Directory {
                    folders.push(info.name().to_string_lossy().into_owned());
                }
            }
        }
        folders.sort();
        Ok(folders)
    }

    /// Design 7d's "New folder". A device that refuses folder creation
    /// directly at a storage's own top level (the same class of refusal
    /// `DeviceStorage::storage_root`'s doc comment already records for the
    /// raw multi-volume device root, just one level down) surfaces as
    /// [`DeviceIoError::CannotCreateAtStorageRoot`] instead of the generic
    /// I/O error, so the browser can explain it rather than just failing.
    pub async fn create_child_folder(
        &self,
        storage_id: StorageId,
        path: &str,
        name: &str,
    ) -> Result<(), DeviceIoError> {
        let sanitized = reprise_core::device_sync::safe_component(name, "New folder");
        let storage = self.resolve_storage_root(storage_id).await?;
        let components = browse_components(path)?;
        let at_storage_root = components.is_empty();
        let parent = components
            .into_iter()
            .fold(storage, |parent, component| parent.child(component));
        let target = parent.child(&sanitized);
        match target
            .make_directory_future(gio::glib::Priority::DEFAULT)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) if error.matches(gio::IOErrorEnum::Exists) => {
                Err(DeviceIoError::FolderAlreadyExists)
            }
            Err(error) if at_storage_root => Err(DeviceIoError::CannotCreateAtStorageRoot(error)),
            Err(error) => Err(error.into()),
        }
    }

    /// `MTP-32`: relocates an already-synced target folder from
    /// `from_path` to `to_path` on the *same* `storage_id` in one MTP move
    /// instead of the sync layer re-copying every file under it. Only
    /// called when `reprise_core::device_sync::browser::target_relocation_action`
    /// resolves to `MoveFolder` — a storage change is never routed here,
    /// it goes through the ordinary copy-and-orphan path instead.
    pub async fn move_child_folder(
        &self,
        storage_id: StorageId,
        from_path: &str,
        to_path: &str,
    ) -> Result<(), DeviceIoError> {
        let storage = self.resolve_storage_root(storage_id).await?;
        let from_components = browse_components(from_path)?;
        let to_components = browse_components(to_path)?;
        if from_components.is_empty() || to_components.is_empty() {
            return Err(DeviceIoError::InvalidRelativePath);
        }
        let mut destination_parent = storage.clone();
        for component in &to_components[..to_components.len() - 1] {
            destination_parent = destination_parent.child(component);
            match destination_parent
                .make_directory_future(gio::glib::Priority::DEFAULT)
                .await
            {
                Ok(()) => {}
                Err(error) if error.matches(gio::IOErrorEnum::Exists) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let from = from_components
            .into_iter()
            .fold(storage.clone(), |parent, component| parent.child(component));
        let to = to_components
            .into_iter()
            .fold(storage, |parent, component| parent.child(component));
        from.move_future(&to, gio::FileCopyFlags::NONE, gio::glib::Priority::DEFAULT)
            .0
            .await?;
        Ok(())
    }

    /// Re-lists the device root's storage volumes and returns the one
    /// matching `storage_id`. Deliberately re-derived every call rather
    /// than cached — a storage can disappear (SD card removed) between
    /// browser sessions, and the module's core rule is that nothing
    /// MTP-derived is trusted to still be valid across a reconnect.
    ///
    /// `pub(super)`: also the resolution primitive
    /// [`DeviceStorage::resolve_target_storage`] (parent module) and
    /// [`inspection`](super::inspection) build on, so a target's persisted
    /// `StorageId` (`MTP-38`) is what transfers and inspection actually use,
    /// not just what the folder browser previews (`MTP-31`).
    pub(super) async fn resolve_storage_root(
        &self,
        storage_id: StorageId,
    ) -> Result<gio::File, DeviceIoError> {
        let enumerator = self
            .root
            .enumerate_children_future(
                ENUMERATE_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await?;
        loop {
            let batch = enumerator
                .next_files_future(ENUMERATE_BATCH_SIZE, gio::glib::Priority::DEFAULT)
                .await?;
            if batch.is_empty() {
                return Err(DeviceIoError::StorageNotFound);
            }
            for info in batch {
                if info.file_type() != gio::FileType::Directory {
                    continue;
                }
                let name = info.name().to_string_lossy().into_owned();
                if derive_storage_id(&name) == storage_id {
                    return Ok(self.root.child(name));
                }
            }
        }
    }
}

/// See the module docs: GVfs exposes no raw PTP `StorageID`, so this
/// derives a [`StorageId`] deterministically from the volume's GVfs-
/// reported name.
pub(super) fn derive_storage_id(name: &str) -> StorageId {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    #[allow(clippy::cast_possible_truncation)]
    StorageId(hasher.finish() as u32)
}

/// Splits a device-path string (e.g. `/Music/Reprise-YouTube`, or `/` /
/// `""` for "the storage's own root") into path components. Unlike
/// `safe_target_components` in the parent module, an empty result is
/// valid here — it names the storage root itself, a real, browsable
/// location — rather than an error.
fn browse_components(path: &str) -> Result<Vec<String>, DeviceIoError> {
    if path.chars().any(char::is_control) {
        return Err(DeviceIoError::InvalidRelativePath);
    }
    Ok(Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string),
            _ => None,
        })
        .collect())
}
