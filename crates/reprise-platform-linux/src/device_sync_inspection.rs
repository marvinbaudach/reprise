use std::collections::VecDeque;

use gio::prelude::*;
use reprise_core::device_sync::{DeviceStorageAccess, ManagedDeviceFile};

use super::{
    is_audio_file, join_relative, DeviceIoError, DeviceStorage, DeviceStorageInspection,
    DeviceStorageSnapshot, ENUMERATE_ATTRIBUTES, ENUMERATE_BATCH_SIZE,
};

impl DeviceStorage {
    /// Aggregates music usage on the exact storage volume that receives
    /// `Music/Reprise`. Only Reprise-owned relative paths cross the platform
    /// boundary; foreign phone tracks are reduced to one byte count.
    pub async fn inspect(&self) -> Result<DeviceStorageInspection, DeviceIoError> {
        let storage = self.storage_root().await?;
        let mut inspection = DeviceStorageInspection {
            snapshot: DeviceStorageSnapshot {
                target_name: storage
                    .basename()
                    .map(|name| name.to_string_lossy().into_owned()),
                access: storage_access(&storage).await,
                free_bytes: optional_filesystem_bytes(
                    &storage,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_FREE,
                )
                .await,
                total_bytes: optional_filesystem_bytes(
                    &storage,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE,
                )
                .await,
                ..DeviceStorageSnapshot::default()
            },
            managed_files: Vec::new(),
            podcast_files: Vec::new(),
        };
        let music = storage.child("Music");
        let mut pending = VecDeque::from([(music, String::new())]);
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
                    return Ok(inspection);
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
                        continue;
                    }
                    if info.file_type() != gio::FileType::Regular {
                        continue;
                    }
                    let size_bytes = info.size().max(0) as u64;
                    if let Some(managed_path) = relative_path.strip_prefix("Reprise/") {
                        if !is_managed_item_file(&name) {
                            continue;
                        }
                        inspection.snapshot.reprise_music_bytes = inspection
                            .snapshot
                            .reprise_music_bytes
                            .saturating_add(size_bytes);
                        inspection.managed_files.push(ManagedDeviceFile {
                            relative_path: managed_path.to_string(),
                            size_bytes,
                        });
                    } else if is_audio_file(&name) {
                        inspection.snapshot.other_music_bytes = inspection
                            .snapshot
                            .other_music_bytes
                            .saturating_add(size_bytes);
                    }
                }
            }
        }
        inspection
            .managed_files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        inspection.podcast_files = inspect_podcasts(&storage).await?;
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

async fn inspect_podcasts(storage: &gio::File) -> Result<Vec<ManagedDeviceFile>, DeviceIoError> {
    let root = storage.child("Podcasts").child("Reprise");
    let mut pending = VecDeque::from([(root, String::new())]);
    let mut files = Vec::new();
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
                return Ok(files);
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
                } else if info.file_type() == gio::FileType::Regular
                    && is_audio_file(&name)
                    && !name.to_ascii_lowercase().ends_with(".part")
                {
                    files.push(ManagedDeviceFile {
                        relative_path,
                        size_bytes: info.size().max(0) as u64,
                    });
                }
            }
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn is_managed_item_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.ends_with(".part")
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
