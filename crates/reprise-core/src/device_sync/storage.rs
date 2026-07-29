//! Pure storage composition and after-sync projection.

use super::{ManagedDeviceFile, MirrorPlan};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceStorageAccess {
    Writable,
    ReadOnly,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceStorageSnapshot {
    pub target_name: Option<String>,
    pub access: DeviceStorageAccess,
    pub total_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub reprise_music_bytes: u64,
    pub other_music_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceStorageInspection {
    pub snapshot: DeviceStorageSnapshot,
    pub managed_files: Vec<ManagedDeviceFile>,
    pub podcast_files: Vec<ManagedDeviceFile>,
    /// Files found under the YouTube-audio target folder (`MTP-38`,
    /// default `/Music/Reprise-YouTube`). Kept apart from `managed_files`
    /// (the Playlists target) the same way `podcast_files` already is —
    /// each named target gets its own inventory list.
    pub youtube_files: Vec<ManagedDeviceFile>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StorageKnowledge {
    Complete,
    #[default]
    CapacityUnknown,
    Inconsistent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageComposition {
    pub total_bytes: Option<u64>,
    pub reprise_music_bytes: u64,
    pub other_music_bytes: u64,
    pub other_used_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub knowledge: StorageKnowledge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageProjectionState {
    Fits,
    Insufficient { shortfall_bytes: u64 },
    CapacityUnknown,
    Inconsistent,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceStorageProjection {
    pub target_name: Option<String>,
    pub access: DeviceStorageAccess,
    pub current: StorageComposition,
    pub after_sync: Option<StorageComposition>,
    pub transfer_bytes: u64,
    pub state: StorageProjectionState,
}

pub fn project_storage(
    snapshot: &DeviceStorageSnapshot,
    plan: &MirrorPlan,
) -> DeviceStorageProjection {
    let current = storage_composition(snapshot);
    let base = DeviceStorageProjection {
        target_name: snapshot.target_name.clone(),
        access: snapshot.access,
        current: current.clone(),
        after_sync: None,
        transfer_bytes: plan.transfer_bytes,
        state: StorageProjectionState::CapacityUnknown,
    };
    if !plan.blockers.is_empty() {
        return DeviceStorageProjection {
            state: StorageProjectionState::Blocked,
            ..base
        };
    }
    if current.knowledge == StorageKnowledge::Inconsistent {
        return DeviceStorageProjection {
            state: StorageProjectionState::Inconsistent,
            ..base
        };
    }
    let Some(free_bytes) = snapshot.free_bytes else {
        return DeviceStorageProjection {
            after_sync: Some(composition(
                snapshot.total_bytes,
                None,
                plan.target_bytes,
                snapshot.other_music_bytes,
            )),
            ..base
        };
    };
    let Some(reclaimable_bytes) = free_bytes.checked_add(snapshot.reprise_music_bytes) else {
        return DeviceStorageProjection {
            state: StorageProjectionState::Inconsistent,
            ..base
        };
    };
    let Some(projected_free_bytes) = reclaimable_bytes.checked_sub(plan.target_bytes) else {
        return DeviceStorageProjection {
            state: StorageProjectionState::Insufficient {
                shortfall_bytes: plan.target_bytes - reclaimable_bytes,
            },
            ..base
        };
    };
    let after_sync = composition(
        snapshot.total_bytes,
        Some(projected_free_bytes),
        plan.target_bytes,
        snapshot.other_music_bytes,
    );
    if after_sync.knowledge == StorageKnowledge::Inconsistent {
        return DeviceStorageProjection {
            state: StorageProjectionState::Inconsistent,
            ..base
        };
    }
    DeviceStorageProjection {
        state: StorageProjectionState::Fits,
        after_sync: Some(after_sync),
        ..base
    }
}

pub fn storage_composition(snapshot: &DeviceStorageSnapshot) -> StorageComposition {
    composition(
        snapshot.total_bytes,
        snapshot.free_bytes,
        snapshot.reprise_music_bytes,
        snapshot.other_music_bytes,
    )
}

fn composition(
    total_bytes: Option<u64>,
    free_bytes: Option<u64>,
    reprise_music_bytes: u64,
    other_music_bytes: u64,
) -> StorageComposition {
    let Some(known_music_bytes) = reprise_music_bytes.checked_add(other_music_bytes) else {
        return StorageComposition {
            total_bytes,
            reprise_music_bytes,
            other_music_bytes,
            other_used_bytes: None,
            free_bytes,
            knowledge: StorageKnowledge::Inconsistent,
        };
    };
    let knowledge = match (total_bytes, free_bytes) {
        (Some(total), _) if known_music_bytes > total => StorageKnowledge::Inconsistent,
        (Some(total), Some(free)) if free > total || known_music_bytes > total - free => {
            StorageKnowledge::Inconsistent
        }
        (Some(_), Some(_)) => StorageKnowledge::Complete,
        _ => StorageKnowledge::CapacityUnknown,
    };
    let other_used_bytes = match (knowledge, total_bytes, free_bytes) {
        (StorageKnowledge::Complete, Some(total), Some(free)) => {
            Some(total - free - known_music_bytes)
        }
        _ => None,
    };
    StorageComposition {
        total_bytes,
        reprise_music_bytes,
        other_music_bytes,
        other_used_bytes,
        free_bytes,
        knowledge,
    }
}
