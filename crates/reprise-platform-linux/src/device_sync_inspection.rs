use std::collections::VecDeque;

use gio::prelude::*;
use reprise_core::device_sync::{DeviceStorageAccess, ManagedDeviceFile, SyncTarget};

use super::{
    is_audio_file, join_relative, safe_target_components, DeviceIoError, DeviceStorage,
    DeviceStorageInspection, DeviceStorageSnapshot, ENUMERATE_ATTRIBUTES, ENUMERATE_BATCH_SIZE,
    PARTIAL_SUFFIX,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManagedWalk {
    pub managed_files: Vec<ManagedDeviceFile>,
    pub partial_paths: Vec<String>,
    pub lyrics_files: Vec<ManagedDeviceFile>,
}

impl DeviceStorage {
    /// Aggregates music usage and managed files on the storage volume that
    /// receives the single playlists target.
    pub async fn inspect(
        &self,
        target: &SyncTarget,
    ) -> Result<DeviceStorageInspection, DeviceIoError> {
        let playlists_storage = self.resolve_target_storage(target.storage_id).await?;

        let mut inspection = DeviceStorageInspection {
            snapshot: DeviceStorageSnapshot {
                target_name: playlists_storage
                    .basename()
                    .map(|name| name.to_string_lossy().into_owned()),
                access: storage_access(&playlists_storage).await,
                free_bytes: optional_filesystem_bytes(
                    &playlists_storage,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_FREE,
                )
                .await,
                total_bytes: optional_filesystem_bytes(
                    &playlists_storage,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE,
                )
                .await,
                ..DeviceStorageSnapshot::default()
            },
            managed_files: Vec::new(),
            partial_paths: Vec::new(),
            lyrics_files: Vec::new(),
        };

        let playlists_components = safe_target_components(&target.path)?;

        // Every target that shares the Playlists target's storage and sits
        // under `Music/` on it must be excluded from the generic `Music/`
        // walk below — it is that target's own tree, not foreign music, and
        // is already inventoried by its own `inspect_target_folder` call.
        // A target on a different storage never appears in this walk at
        // all, so it needs no exclusion.
        let mut excluded_from_music = Vec::new();
        if let Some(relative) = under_music(&playlists_components) {
            excluded_from_music.push(relative);
        }
        inspection.snapshot.other_music_bytes =
            other_music_bytes(&playlists_storage, &excluded_from_music).await?;

        let managed_walk = inspect_target_folder(
            &playlists_storage,
            &playlists_components,
            is_known_managed_item_file,
        )
        .await?;
        inspection.managed_files = managed_walk.managed_files;
        inspection.partial_paths = managed_walk.partial_paths;
        inspection.lyrics_files = managed_walk.lyrics_files;
        inspection.snapshot.reprise_music_bytes = inspection
            .managed_files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.size_bytes));

        Ok(inspection)
    }

    pub async fn available_bytes(&self) -> Result<Option<u64>, DeviceIoError> {
        let storage = self.storage_root().await?;
        filesystem_bytes(&storage, gio::FILE_ATTRIBUTE_FILESYSTEM_FREE).await
    }

    /// Returns capacity attributes from the resolved target storage, never
    /// from the read-only MTP device root that merely lists storage volumes.
    pub async fn capacity_bytes(&self) -> Result<(Option<u64>, Option<u64>), DeviceIoError> {
        let storage = self.storage_root().await?;
        let available = filesystem_bytes(&storage, gio::FILE_ATTRIBUTE_FILESYSTEM_FREE).await?;
        let total = match filesystem_bytes(&storage, gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE).await {
            Ok(total) => total,
            Err(error) => {
                tracing::debug!(%error, "device sync: total capacity is unavailable");
                None
            }
        };
        Ok((available, total))
    }
}

/// Walks the target's own folder — its resolved storage plus its literal
/// path components (`components`, e.g. `["Music", "Reprise"]`) — and
/// returns every file `accept` keeps, keyed by its path relative to that
/// folder. The playlists target (`MTP-23`) is recognized only by the storage
/// + path it was actually written to, never by a hard-coded folder name.
async fn inspect_target_folder(
    storage: &gio::File,
    components: &[String],
    accept: impl Fn(&str) -> bool,
) -> Result<ManagedWalk, DeviceIoError> {
    let root = components
        .iter()
        .fold(storage.clone(), |parent, component| parent.child(component));
    let mut pending = VecDeque::from([(root, String::new())]);
    let mut walk = ManagedWalk::default();
    while let Some((directory, prefix)) = pending.pop_front() {
        let enumerator = match directory
            .enumerate_children_future(
                ENUMERATE_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await
        {
            Ok(enumerator) => enumerator,
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) && prefix.is_empty() => {
                return Ok(walk);
            }
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
                let name = info.name().to_string_lossy().into_owned();
                let relative_path = join_relative(&prefix, &name);
                if info.file_type() == gio::FileType::Directory {
                    pending.push_back((directory.child(&name), relative_path));
                } else if info.file_type() == gio::FileType::Regular {
                    let file = ManagedDeviceFile {
                        relative_path: relative_path.clone(),
                        size_bytes: info.size().max(0) as u64,
                    };
                    if name.ends_with(PARTIAL_SUFFIX) {
                        walk.partial_paths.push(relative_path);
                    } else if reprise_core::device_sync::lyrics_sidecar::is_sidecar_path(
                        std::path::Path::new(&name),
                    ) {
                        walk.lyrics_files.push(file);
                    } else if accept(&name) {
                        walk.managed_files.push(file);
                    }
                }
            }
        }
    }
    walk.managed_files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    walk.partial_paths.sort();
    walk.lyrics_files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(walk)
}

/// Sums every audio file under `Music/` on `storage` that does not fall
/// inside `excluded` — the storage-relative-to-`Music/` subtrees already
/// owned by the playlists target. A missing `Music/`
/// folder is not an error, just an empty device library.
async fn other_music_bytes(storage: &gio::File, excluded: &[String]) -> Result<u64, DeviceIoError> {
    let music = storage.child("Music");
    let mut pending = VecDeque::from([(music, String::new())]);
    let mut total = 0_u64;
    while let Some((directory, prefix)) = pending.pop_front() {
        let enumerator = match directory
            .enumerate_children_future(
                ENUMERATE_ATTRIBUTES,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await
        {
            Ok(enumerator) => enumerator,
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) && prefix.is_empty() => {
                return Ok(total);
            }
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
                let name = info.name().to_string_lossy().into_owned();
                let relative_path = join_relative(&prefix, &name);
                if info.file_type() == gio::FileType::Directory {
                    if excluded.contains(&relative_path) {
                        continue;
                    }
                    pending.push_back((directory.child(&name), relative_path));
                    continue;
                }
                if info.file_type() == gio::FileType::Regular && is_audio_file(&name) {
                    total = total.saturating_add(info.size().max(0) as u64);
                }
            }
        }
    }
    Ok(total)
}

/// Whether a regular file belongs in the device inventory.
///
/// This answers only whether the planner needs to know the file exists.
/// Whether an unmatched file may be removed is a separate Core planning
/// decision: generated analysis sidecars and the metadata list are known here
/// but deliberately protected there.
fn is_known_managed_item_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.ends_with(".part")
        && !reprise_core::device_sync::lyrics_sidecar::is_sidecar_path(std::path::Path::new(&name))
}

/// `components` relative to `Music/`, when `components` names a path that
/// actually starts with a `Music` folder — `None` otherwise (the target
/// lives outside `Music/` entirely, so the generic `Music/` walk never
/// reaches it and needs no exclusion for it).
fn under_music(components: &[String]) -> Option<String> {
    match components.split_first() {
        Some((first, rest)) if first == "Music" && !rest.is_empty() => Some(rest.join("/")),
        _ => None,
    }
}

async fn storage_access(storage: &gio::File) -> DeviceStorageAccess {
    let filesystem_readonly =
        optional_filesystem_boolean(storage, gio::FILE_ATTRIBUTE_FILESYSTEM_READONLY).await;
    let can_write = optional_file_boolean(storage, gio::FILE_ATTRIBUTE_ACCESS_CAN_WRITE).await;
    storage_access_from_attributes(filesystem_readonly, can_write)
}

pub(super) fn storage_access_from_attributes(
    filesystem_readonly: Option<bool>,
    can_write: Option<bool>,
) -> DeviceStorageAccess {
    if filesystem_readonly == Some(true) || can_write == Some(false) {
        DeviceStorageAccess::ReadOnly
    } else if can_write == Some(true) {
        DeviceStorageAccess::Writable
    } else {
        DeviceStorageAccess::Unknown
    }
}

async fn optional_filesystem_boolean(storage: &gio::File, attribute: &str) -> Option<bool> {
    match storage
        .query_filesystem_info_future(attribute, gio::glib::Priority::DEFAULT)
        .await
    {
        Ok(info) => info
            .has_attribute(attribute)
            .then(|| info.boolean(attribute)),
        Err(error) => {
            tracing::debug!(%error, attribute, "device sync: storage capability attribute is unavailable");
            None
        }
    }
}

async fn optional_file_boolean(storage: &gio::File, attribute: &str) -> Option<bool> {
    match storage
        .query_info_future(
            attribute,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::glib::Priority::DEFAULT,
        )
        .await
    {
        Ok(info) => info
            .has_attribute(attribute)
            .then(|| info.boolean(attribute)),
        Err(error) => {
            tracing::debug!(%error, attribute, "device sync: target access attribute is unavailable");
            None
        }
    }
}

async fn optional_filesystem_bytes(storage: &gio::File, attribute: &str) -> Option<u64> {
    match filesystem_bytes(storage, attribute).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::debug!(%error, attribute, "device sync: storage capacity attribute is unavailable");
            None
        }
    }
}

async fn filesystem_bytes(
    storage: &gio::File,
    attribute: &str,
) -> Result<Option<u64>, DeviceIoError> {
    let info = storage
        .query_filesystem_info_future(attribute, gio::glib::Priority::DEFAULT)
        .await?;
    Ok(info
        .has_attribute(attribute)
        .then(|| info.attribute_uint64(attribute)))
}

#[cfg(test)]
mod tests {
    use super::is_known_managed_item_file;

    #[test]
    fn lyr_7_lrc_attachments_are_not_independent_managed_inventory_entries() {
        assert!(!is_known_managed_item_file("Artist/Album/Song.lrc"));
        assert!(is_known_managed_item_file("Artist/Album/Song.opus"));
    }

    #[test]
    fn analysis_sidecars_are_visible_to_the_managed_inventory() {
        assert!(is_known_managed_item_file(
            "Artist/Album/Song.reprise-analysis"
        ));
        assert!(is_known_managed_item_file("Artist/Album/Song.opus"));
    }

    #[test]
    fn track_metadata_list_is_visible_to_the_managed_inventory() {
        assert!(is_known_managed_item_file("reprise-track-metadata.rpl"));
    }
}
