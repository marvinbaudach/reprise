use std::collections::VecDeque;

use gio::prelude::*;
use reprise_core::device_sync::{
    DeviceStorageAccess, ManagedDeviceFile, SyncTarget, SyncTargetKind,
};

use super::{
    is_audio_file, join_relative, safe_target_components, DeviceIoError, DeviceStorage,
    DeviceStorageInspection, DeviceStorageSnapshot, ENUMERATE_ATTRIBUTES, ENUMERATE_BATCH_SIZE,
};

impl DeviceStorage {
    /// Aggregates music usage on the storage volume that receives the
    /// Playlists target, and walks each of the three named targets
    /// (`MTP-18`) at its own persisted `storage_id` + path — never a
    /// hard-coded `Music/Reprise`-shaped guess, so a folder the browser
    /// (`MTP-31`) repointed at a different storage or path is recognized
    /// as that target's inventory both here and by the transfer that wrote
    /// it (`DeviceStorage::resolve_target_storage`).
    pub async fn inspect(
        &self,
        targets: &[SyncTarget; 3],
    ) -> Result<DeviceStorageInspection, DeviceIoError> {
        let playlists_target = find_target(targets, SyncTargetKind::Playlists);
        let youtube_target = find_target(targets, SyncTargetKind::YoutubeAudio);
        let podcasts_target = find_target(targets, SyncTargetKind::PodcastEpisodes);

        let playlists_storage = self
            .resolve_target_storage(playlists_target.storage_id)
            .await?;
        let youtube_storage = self
            .resolve_target_storage(youtube_target.storage_id)
            .await?;
        let podcasts_storage = self
            .resolve_target_storage(podcasts_target.storage_id)
            .await?;

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
            podcast_files: Vec::new(),
            youtube_files: Vec::new(),
        };

        let playlists_components = safe_target_components(&playlists_target.path)?;
        let youtube_components = safe_target_components(&youtube_target.path)?;
        let podcasts_components = safe_target_components(&podcasts_target.path)?;

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
        if same_storage(&youtube_storage, &playlists_storage) {
            if let Some(relative) = under_music(&youtube_components) {
                excluded_from_music.push(relative);
            }
        }
        if same_storage(&podcasts_storage, &playlists_storage) {
            if let Some(relative) = under_music(&podcasts_components) {
                excluded_from_music.push(relative);
            }
        }
        inspection.snapshot.other_music_bytes =
            other_music_bytes(&playlists_storage, &excluded_from_music).await?;

        inspection.managed_files = inspect_target_folder(
            &playlists_storage,
            &playlists_components,
            is_managed_item_file,
        )
        .await?;
        inspection.snapshot.reprise_music_bytes = inspection
            .managed_files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.size_bytes));

        inspection.youtube_files =
            inspect_target_folder(&youtube_storage, &youtube_components, managed_audio_file)
                .await?;
        inspection.podcast_files =
            inspect_target_folder(&podcasts_storage, &podcasts_components, managed_audio_file)
                .await?;

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

/// Walks one target's own folder — its resolved storage plus its literal
/// path components (`components`, e.g. `["Podcasts", "Reprise"]`) — and
/// returns every file `accept` keeps, keyed by its path relative to that
/// folder. This is the one inventory primitive all three named targets
/// (`MTP-18`) use, so each is recognized only by the storage + path it was
/// actually written to, never by a hard-coded folder name.
async fn inspect_target_folder(
    storage: &gio::File,
    components: &[String],
    accept: impl Fn(&str) -> bool,
) -> Result<Vec<ManagedDeviceFile>, DeviceIoError> {
    let root = components
        .iter()
        .fold(storage.clone(), |parent, component| parent.child(component));
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
                } else if info.file_type() == gio::FileType::Regular && accept(&name) {
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

/// Sums every audio file under `Music/` on `storage` that does not fall
/// inside `excluded` — the storage-relative-to-`Music/` subtrees already
/// owned by one of the three named targets (`MTP-18`). A missing `Music/`
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

fn is_managed_item_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.ends_with(".part")
}

/// The accept predicate for the YouTube-audio and podcast-episode targets:
/// unlike the Playlists target, these never contain non-audio managed files
/// (no `.m3u8`), so a stray non-audio file under them is left alone rather
/// than swept into the inventory.
fn managed_audio_file(name: &str) -> bool {
    is_audio_file(name) && is_managed_item_file(name)
}

/// The `SyncTarget` for `kind` out of the freshly loaded three — falls back
/// to the kind's design default if somehow absent, matching
/// `device_sync_planned.rs::target_path`'s "defense in depth, never the
/// normal path" reasoning: `load_or_create_targets` always returns all
/// three.
fn find_target(targets: &[SyncTarget; 3], kind: SyncTargetKind) -> SyncTarget {
    targets
        .iter()
        .find(|target| target.kind == kind)
        .cloned()
        .unwrap_or_else(|| SyncTarget::default_for(kind))
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

fn same_storage(left: &gio::File, right: &gio::File) -> bool {
    left.equal(right)
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
