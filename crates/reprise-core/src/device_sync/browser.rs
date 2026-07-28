//! The device folder browser (design 7d, `MTP-31`/`MTP-32`).
//!
//! `MTP-18`'s module docs already state the hard MTP fact this browser has
//! to live inside: folders are object handles under a `StorageID`, and
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

/// Design 7d's warning: does `candidate` sit at or inside the Playlists
/// target's folder? `MTP-17` makes the Playlists target fully
/// authoritative — it deletes anything under it that Reprise does not
/// want — so nesting another target's folder inside it would expose that
/// target's own files to the Playlists cleanup pass. Only compares the
/// same storage: a path string alone means nothing across two different
/// storages, and an unresolved Playlists target has nothing to conflict
/// with yet.
#[must_use]
pub fn folder_conflicts_with_playlist_target(
    candidate_storage: Option<StorageId>,
    candidate_path: &str,
    playlist_target: &SyncTarget,
) -> bool {
    let Some(playlist_storage) = playlist_target.storage_id else {
        return false;
    };
    if candidate_storage != Some(playlist_storage) {
        return false;
    }
    path_is_within(candidate_path, &playlist_target.path)
}

/// True when `candidate` is `ancestor` itself or a subfolder of it,
/// compared component-wise (not by string prefix, so `/Music/Reprise2`
/// does not falsely match ancestor `/Music/Reprise`).
fn path_is_within(candidate: &str, ancestor: &str) -> bool {
    let candidate = normalized_components(candidate);
    let ancestor = normalized_components(ancestor);
    ancestor.len() <= candidate.len() && candidate[..ancestor.len()] == ancestor[..]
}

fn normalized_components(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|component| !component.is_empty())
        .collect()
}

/// Design 7d's "Reset to default": restores only the folder — storage and
/// path — to `kind`'s design default. `enabled` and `cap_bytes` are
/// untouched; they are not part of the browser's scope (`MTP-28`'s
/// addendum keeps the cap and selection summary as read-only Preferences
/// mirrors), so resetting the folder must never silently flip either.
#[must_use]
pub fn reset_target_folder(target: &SyncTarget) -> SyncTarget {
    SyncTarget {
        storage_id: None,
        path: target.kind.default_path().to_string(),
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
/// of the question to [`target_storage_transition`] (`MTP-18`) rather than
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
    use crate::device_sync::SyncTargetKind;

    fn playlists_target(storage_id: Option<StorageId>, path: &str) -> SyncTarget {
        SyncTarget {
            kind: SyncTargetKind::Playlists,
            storage_id,
            path: path.to_string(),
            enabled: true,
            cap_bytes: None,
        }
    }

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
            kind: SyncTargetKind::YoutubeAudio,
            storage_id: None,
            path: "/Music/Reprise-YouTube".to_string(),
            enabled: true,
            cap_bytes: None,
        };
        assert_eq!(
            preview_target_folder(&target, &[]),
            TargetPreview::Unresolved {
                path: "/Music/Reprise-YouTube".to_string()
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
                path: "/Music/Reprise-YouTube".to_string(),
            }
        );

        let missing = SyncTarget {
            storage_id: Some(StorageId(2)),
            ..target
        };
        assert_eq!(
            preview_target_folder(&missing, &storages),
            TargetPreview::StorageMissing {
                path: "/Music/Reprise-YouTube".to_string()
            }
        );
    }

    #[test]
    fn mtp_31_conflict_warns_when_candidate_is_at_or_inside_the_playlist_target() {
        let playlists = playlists_target(Some(StorageId(1)), "/Music/Reprise");
        assert!(folder_conflicts_with_playlist_target(
            Some(StorageId(1)),
            "/Music/Reprise",
            &playlists
        ));
        assert!(folder_conflicts_with_playlist_target(
            Some(StorageId(1)),
            "/Music/Reprise/Sub",
            &playlists
        ));
        assert!(!folder_conflicts_with_playlist_target(
            Some(StorageId(1)),
            "/Music/Reprise2",
            &playlists
        ));
        assert!(!folder_conflicts_with_playlist_target(
            Some(StorageId(1)),
            "/Music",
            &playlists
        ));
    }

    #[test]
    fn mtp_31_conflict_is_false_when_playlist_target_storage_is_unresolved_or_different() {
        let unresolved = playlists_target(None, "/Music/Reprise");
        assert!(!folder_conflicts_with_playlist_target(
            Some(StorageId(1)),
            "/Music/Reprise",
            &unresolved
        ));

        let resolved = playlists_target(Some(StorageId(1)), "/Music/Reprise");
        assert!(!folder_conflicts_with_playlist_target(
            Some(StorageId(2)),
            "/Music/Reprise",
            &resolved
        ));
    }

    #[test]
    fn mtp_31_reset_restores_default_path_and_clears_storage_without_touching_enabled_or_cap() {
        let target = SyncTarget {
            kind: SyncTargetKind::PodcastEpisodes,
            storage_id: Some(StorageId(9)),
            path: "/Weird/Custom/Path".to_string(),
            enabled: false,
            cap_bytes: Some(123),
        };
        let reset = reset_target_folder(&target);
        assert_eq!(reset.storage_id, None);
        assert_eq!(reset.path, "/Podcasts/Reprise");
        assert!(!reset.enabled, "reset must not silently re-enable a target");
        assert_eq!(
            reset.cap_bytes,
            Some(123),
            "reset must not touch the cap — not part of the browser's scope"
        );
    }

    #[test]
    fn mtp_32_relocation_moves_within_the_same_storage_and_copies_across_a_storage_change() {
        let resolved = SyncTarget {
            kind: SyncTargetKind::YoutubeAudio,
            storage_id: Some(StorageId(1)),
            path: "/Music/Reprise-YouTube".to_string(),
            enabled: true,
            cap_bytes: None,
        };
        let renamed = SyncTarget {
            path: "/Music/YT".to_string(),
            ..resolved.clone()
        };
        assert_eq!(
            target_relocation_action(&resolved, &renamed),
            TargetRelocation::MoveFolder {
                from_path: "/Music/Reprise-YouTube".to_string()
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
        let unresolved = SyncTarget::default_for(SyncTargetKind::PodcastEpisodes);
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
