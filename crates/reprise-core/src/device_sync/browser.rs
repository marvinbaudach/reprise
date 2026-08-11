//! The device folder browser (design 7d, `MTP-31`/`MTP-32`).
//!
//! The hard MTP fact this browser lives inside is that folders are object
//! handles under a `StorageID`, and
//! handles are not stable across reconnects. This module never receives or
//! produces a handle — only facts the frontend has already gathered this
//! session ([`StorageOption`], a folder's already-listed child names) and
//! the two persisted fields a [`SyncTarget`] actually stores
//! (`storage_id`, `path`). Fetching those facts over MTP (`GetObjectPropList`
//! for the folder tree, `SendObjectInfo` for "New folder") is GVfs/gio I/O
//! and lives in `reprise-platform-linux`; this module is the display-free
//! projection over the results, so the browser dialog gathers facts and
//! obeys rather than deciding anything itself.

use super::targets::{target_storage_transition, StorageId, StorageTransition};
use super::SyncTarget;

/// Design 7d's "storage selection (internal / SD card)": a browsable MTP
/// storage volume, classified from its GVfs-reported name since GVfs's MTP
/// backend does not expose the raw PTP `StorageID` value through any
/// standard file attribute — see
/// `reprise_platform_linux::device_sync::browser` for where the name is
/// read and [`StorageId`] is derived from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageOption {
    pub id: StorageId,
    pub name: String,
    pub kind: StorageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageKind {
    Internal,
    Removable,
}

/// Classifies one storage volume's GVfs-reported name. Mirrors the
/// "internal, unless it names itself card/SD" heuristic
/// `reprise_platform_linux::device_sync::choose_storage_volume` already
/// uses to pick a default; this is the same judgement made explicit and
/// reusable for every volume, not just the chosen one.
#[must_use]
pub fn classify_storage_kind(name: &str) -> StorageKind {
    let lower = name.to_lowercase();
    if lower.contains("sd") || lower.contains("card") {
        StorageKind::Removable
    } else {
        StorageKind::Internal
    }
}

/// Design 7d's target preview: where files chosen for one [`SyncTarget`]
/// will actually land, given the storages the browser has listed this
/// session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetPreview {
    /// No storage has been resolved for this target yet — only the path is
    /// known (a fresh device's design default, or a target never opened in
    /// the browser).
    Unresolved { path: String },
    /// The target's `storage_id` no longer matches any storage the browser
    /// just listed — e.g. an SD card was removed. The path alone cannot
    /// resolve to a real on-device location.
    StorageMissing { path: String },
    /// Resolves to a concrete on-device location.
    Resolved { storage_name: String, path: String },
}

/// Pure projection backing the target preview. Takes the already-listed
/// `storages` rather than fetching them, matching this module's "gather
/// facts, then obey" split.
#[must_use]
pub fn preview_target_folder(target: &SyncTarget, storages: &[StorageOption]) -> TargetPreview {
    let Some(storage_id) = target.storage_id else {
        return TargetPreview::Unresolved {
            path: target.path.clone(),
        };
    };
    match storages.iter().find(|option| option.id == storage_id) {
        Some(option) => TargetPreview::Resolved {
            storage_name: option.name.clone(),
            path: target.path.clone(),
        },
        None => TargetPreview::StorageMissing {
            path: target.path.clone(),
        },
    }
}

/// Design 7d's "Reset to default": restores the single playlists target's
/// folder while preserving whether synchronization is enabled.
#[must_use]
pub fn reset_target_folder(target: &SyncTarget) -> SyncTarget {
    SyncTarget {
        storage_id: None,
        path: super::targets::DEFAULT_TARGET_PATH.to_string(),
        ..target.clone()
    }
}

/// `MTP-32`: what should happen to already-synced files when a target's
/// folder changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetRelocation {
    /// Nothing changed, or the target had never been resolved to a
    /// storage before — there is nothing on the device to relocate.
    Unchanged,
    /// The folder moved on the *same* storage: an MTP move (rename)
    /// relocates whatever is already there in one step, so the next sync
    /// only has to copy what is genuinely new instead of re-uploading
    /// everything that was already correctly on the device.
    MoveFolder { from_path: String },
    /// The storage itself changed (`StorageTransition::Changed`). A folder
    /// cannot move across MTP storage boundaries, so the sync layer must
    /// copy fresh into the new location; the previous storage's copy
    /// becomes orphaned and is cleaned up once that copy has completed —
    /// unchanged from `target_storage_transition`'s existing contract.
    CopyAndOrphanPrevious,
}

/// The pure decision behind `MTP-32`. Delegates the storage-boundary half
/// of the question to [`target_storage_transition`] rather than
/// re-deriving it, and only adds the same-storage "is this actually a
/// folder rename" half that target does not need to answer.
#[must_use]
pub fn target_relocation_action(previous: &SyncTarget, next: &SyncTarget) -> TargetRelocation {
    match target_storage_transition(previous, next) {
        StorageTransition::Changed { .. } => TargetRelocation::CopyAndOrphanPrevious,
        StorageTransition::SameOrFirstResolution => {
            if previous.storage_id.is_some() && previous.path != next.path {
                TargetRelocation::MoveFolder {
                    from_path: previous.path.clone(),
                }
            } else {
                TargetRelocation::Unchanged
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_31_classifies_storage_by_name() {
        assert_eq!(
            classify_storage_kind("Internal shared storage"),
            StorageKind::Internal
        );
        assert_eq!(classify_storage_kind("SD card"), StorageKind::Removable);
        assert_eq!(
            classify_storage_kind("Samsung SD Card 128GB"),
            StorageKind::Removable
        );
        assert_eq!(classify_storage_kind("Phone"), StorageKind::Internal);
    }

    #[test]
    fn mtp_31_preview_reports_unresolved_missing_and_resolved_storage() {
        let target = SyncTarget {
            storage_id: None,
            path: "/Music/Reprise".to_string(),
            enabled: true,
        };
        assert_eq!(
            preview_target_folder(&target, &[]),
            TargetPreview::Unresolved {
                path: "/Music/Reprise".to_string()
            }
        );

        let resolved = SyncTarget {
            storage_id: Some(StorageId(1)),
            ..target.clone()
        };
        let storages = [StorageOption {
            id: StorageId(1),
            name: "Internal shared storage".to_string(),
            kind: StorageKind::Internal,
        }];
        assert_eq!(
            preview_target_folder(&resolved, &storages),
            TargetPreview::Resolved {
                storage_name: "Internal shared storage".to_string(),
                path: "/Music/Reprise".to_string(),
            }
        );

        let missing = SyncTarget {
            storage_id: Some(StorageId(2)),
            ..target
        };
        assert_eq!(
            preview_target_folder(&missing, &storages),
            TargetPreview::StorageMissing {
                path: "/Music/Reprise".to_string()
            }
        );
    }

    #[test]
    fn mtp_31_reset_restores_default_path_and_clears_storage_without_enabling_sync() {
        let target = SyncTarget {
            storage_id: Some(StorageId(9)),
            path: "/Weird/Custom/Path".to_string(),
            enabled: false,
        };
        let reset = reset_target_folder(&target);
        assert_eq!(reset.storage_id, None);
        assert_eq!(reset.path, "/Music/Reprise");
        assert!(!reset.enabled, "reset must not silently re-enable a target");
    }

    #[test]
    fn mtp_32_relocation_moves_within_the_same_storage_and_copies_across_a_storage_change() {
        let resolved = SyncTarget {
            storage_id: Some(StorageId(1)),
            path: "/Music/Reprise".to_string(),
            enabled: true,
        };
        let renamed = SyncTarget {
            path: "/Music/Playlists".to_string(),
            ..resolved.clone()
        };
        assert_eq!(
            target_relocation_action(&resolved, &renamed),
            TargetRelocation::MoveFolder {
                from_path: "/Music/Reprise".to_string()
            }
        );

        let moved_storage = SyncTarget {
            storage_id: Some(StorageId(2)),
            ..resolved.clone()
        };
        assert_eq!(
            target_relocation_action(&resolved, &moved_storage),
            TargetRelocation::CopyAndOrphanPrevious
        );
    }

    #[test]
    fn mtp_32_relocation_is_unchanged_for_first_resolution_or_identical_target() {
        let unresolved = SyncTarget::default();
        let first_resolution = SyncTarget {
            storage_id: Some(StorageId(1)),
            ..unresolved.clone()
        };
        assert_eq!(
            target_relocation_action(&unresolved, &first_resolution),
            TargetRelocation::Unchanged,
            "first resolution copies fresh through the ordinary sync path, not a move"
        );

        let identical = first_resolution.clone();
        assert_eq!(
            target_relocation_action(&first_resolution, &identical),
            TargetRelocation::Unchanged
        );
    }
}
