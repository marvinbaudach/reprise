//! Human-readable storage capability and capacity copy.

use reprise_core::device_sync::{
    DeviceStorageAccess, DeviceStorageProjection, StorageProjectionState,
};

use super::device_sync_strings;

pub(super) fn storage_summary(storage: &DeviceStorageProjection) -> String {
    format!(
        "{} · {}",
        storage_access_label(storage.access),
        storage_capacity_summary(storage)
    )
}

pub(super) fn storage_access_notice(access: DeviceStorageAccess) -> Option<String> {
    (access == DeviceStorageAccess::ReadOnly)
        .then(|| "The selected device storage is read-only.".into())
}

pub(in crate::ui) fn storage_access_label(access: DeviceStorageAccess) -> &'static str {
    match access {
        DeviceStorageAccess::Writable => "Writable",
        DeviceStorageAccess::ReadOnly => "Read-only",
        DeviceStorageAccess::Unknown => "Write access unknown",
    }
}

fn storage_capacity_summary(storage: &DeviceStorageProjection) -> String {
    match storage.state {
        StorageProjectionState::Blocked => {
            "Storage projection is unavailable until the selection is valid.".into()
        }
        StorageProjectionState::Inconsistent => {
            "The device reported inconsistent storage information.".into()
        }
        StorageProjectionState::Insufficient { shortfall_bytes } => format!(
            "Not enough space · {} more needed",
            device_sync_strings::file_size(shortfall_bytes)
        ),
        StorageProjectionState::CapacityUnknown => {
            let Some(after) = &storage.after_sync else {
                return "Storage capacity and after-sync composition are unknown.".into();
            };
            format!(
                "Music {} · after sync {} · Other unknown · Free {}",
                device_sync_strings::file_size(
                    storage
                        .current
                        .reprise_music_bytes
                        .saturating_add(storage.current.other_music_bytes)
                ),
                storage_delta(
                    storage.current.reprise_music_bytes,
                    after.reprise_music_bytes
                ),
                after
                    .free_bytes
                    .map_or_else(|| "unknown".into(), device_sync_strings::file_size)
            )
        }
        StorageProjectionState::Fits => {
            let Some(after) = &storage.after_sync else {
                return "After-sync storage is unavailable.".into();
            };
            let free = after
                .free_bytes
                .map_or_else(|| "unknown".into(), device_sync_strings::file_size);
            format!(
                "Music {} · after sync {} · Other {} · Free {free}",
                device_sync_strings::file_size(
                    storage
                        .current
                        .reprise_music_bytes
                        .saturating_add(storage.current.other_music_bytes)
                ),
                storage_delta(
                    storage.current.reprise_music_bytes,
                    after.reprise_music_bytes
                ),
                after
                    .other_used_bytes
                    .map_or_else(|| "unknown".into(), device_sync_strings::file_size),
            )
        }
    }
}

fn storage_delta(current_reprise: u64, after_reprise: u64) -> String {
    match after_reprise.cmp(&current_reprise) {
        std::cmp::Ordering::Greater => format!(
            "+{}",
            device_sync_strings::file_size(after_reprise - current_reprise)
        ),
        std::cmp::Ordering::Less => format!(
            "−{}",
            device_sync_strings::file_size(current_reprise - after_reprise)
        ),
        std::cmp::Ordering::Equal => "no change".into(),
    }
}
