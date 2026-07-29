//! Pure projections for the device view (design 7a) and the sidebar device
//! card (design 7c) — turn E5/E7. See
//! `docs/plans/podcasts-youtube-radio-turn6.md` §3b/§8a (`E-5`, `E-6`) for
//! the exact contract these projections implement.
//!
//! ## `E-5`/`E-6`: sync rules live on the device page, not in Preferences
//!
//! Reprise supports exactly one connected MTP device (`E-5`); the
//! 2026-07-28 addendum's global-rules-in-Preferences split existed only to
//! answer "which device do these rules apply to" once several devices were
//! in play. With one device that question does not arise, so `E-6`
//! withdrew the addendum: the cap and the transfer profile are editable
//! per device (`MTP-37`), and the selection summary is a live, honest read
//! of the same per-device selection state edited elsewhere (the podcast
//! and channel pages, `POD-12`) — never a second selection surface layered
//! on top of it. The target folder (`SyncTarget::path`) and whether this
//! device's slot for a category is active at all (`SyncTarget::enabled`)
//! remain per-device, as they always were (`MTP-38`).
//!
//! ## What is reused, not recomputed
//!
//! Every byte and file count here comes from the existing engines:
//! [`super::category_diff::CategoryDiff`]/[`super::category_diff::SyncBalance`]
//! (`MTP-22`) for the diff, and the existing per-target inventory lists
//! (`super::storage::DeviceStorageInspection`'s `managed_files`/
//! `youtube_files`/`podcast_files`) for what is already on the device. This
//! module only adds the two things that were genuinely missing: turning an
//! inventory list into a category's on-device byte total
//! ([`category_bytes`]), and a real, checkable "has this device's content
//! ever been inspected" state ([`DeviceContentsState`]) — today that fact is
//! silently implied by internal scan bookkeeping and never shown to the
//! user at all.

use super::category_diff::{
    project_category_reading, CandidateSource, CategoryDiff, CategoryReading,
};
use super::mirror::ManagedDeviceFile;
use super::storage::DeviceStorageSnapshot;
use super::targets::SyncTarget;

/// `MTP-26`: "Device contents never verified" as a real, checkable state
/// (design 7a), not an implicit fact buried in scan bookkeeping. `Verifying`
/// and `Failed` are their own states rather than folded into
/// `NeverVerified` — a `Failed` scan already tried and has something
/// specific to say (`Scan device` should retry, not pretend nothing
/// happened), and `Verifying` should not offer the action at all
/// ([`DeviceContentsState::can_scan`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceContentsState {
    /// No successful inspection has happened yet this session.
    NeverVerified,
    /// An inspection is in flight right now.
    Verifying,
    /// The most recent inspection succeeded.
    Verified,
    /// The most recent inspection failed; the reason is worth surfacing
    /// next to the "Scan device" retry action.
    Failed(String),
}

impl DeviceContentsState {
    /// Whether the "Scan device" action should be enabled. Only false while
    /// a scan is already running — starting a second one concurrently would
    /// race the first for the same MTP handles.
    #[must_use]
    pub const fn can_scan(&self) -> bool {
        !matches!(self, Self::Verifying)
    }
}

/// `MTP-26`: projects [`DeviceContentsState`] from the runtime's existing
/// scan bookkeeping (`scanning`, `scan_error`) plus whether an inspection
/// has ever completed successfully this session (`ever_inspected`) — no new
/// scan mechanism, this only makes the existing one's outcome legible.
#[must_use]
pub fn project_contents_state(
    scanning: bool,
    scan_error: Option<&str>,
    ever_inspected: bool,
) -> DeviceContentsState {
    if scanning {
        return DeviceContentsState::Verifying;
    }
    if let Some(error) = scan_error {
        return DeviceContentsState::Failed(error.to_string());
    }
    if ever_inspected {
        DeviceContentsState::Verified
    } else {
        DeviceContentsState::NeverVerified
    }
}

/// `MTP-27`: sums one category's on-device inventory into a single byte
/// total — the input to the storage bar's per-category segments and the
/// Content section's "size on device" column. Saturating: a device
/// reporting an impossible sum should read as very large, not wrap.
#[must_use]
pub fn category_bytes(files: &[ManagedDeviceFile]) -> u64 {
    files
        .iter()
        .map(|file| file.size_bytes)
        .fold(0_u64, u64::saturating_add)
}

/// `MTP-27`: the storage bar's segments (design 7a: "a storage bar
/// segmented by category ... with a distinctly hatched 'Incoming this
/// sync' segment"). `music_bytes` covers both Reprise-managed and other
/// music under `/Music` — YouTube audio and podcast episodes get their own
/// segments because they live in their own target folders (`MTP-38`), not
/// because they stop being "music" in any other sense.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CategorySegments {
    pub music_bytes: u64,
    pub youtube_bytes: u64,
    pub podcast_bytes: u64,
    pub other_bytes: u64,
    /// The hatched "Incoming this sync" segment — bytes this sync will
    /// write, before any freed space is subtracted back out.
    pub incoming_bytes: u64,
    pub free_before_bytes: u64,
    pub free_after_bytes: u64,
    pub total_bytes: u64,
}

/// `MTP-27`: projects [`CategorySegments`] from the device's current
/// storage snapshot, this session's per-category on-device byte totals
/// ([`category_bytes`]) and the aggregate sync balance (`MTP-22`'s
/// `SyncBalance`). Returns `None` when capacity is not fully known or the
/// numbers are inconsistent (music + youtube + podcasts + free exceeding
/// the reported total) — the bar disappears rather than inventing a
/// segment, matching how the existing per-target storage bar
/// (`storage::project_storage`) already refuses to guess.
#[must_use]
pub fn project_category_segments(
    snapshot: &DeviceStorageSnapshot,
    youtube_bytes: u64,
    podcast_bytes: u64,
    incoming_bytes: u64,
    freed_bytes: u64,
) -> Option<CategorySegments> {
    let total = snapshot.total_bytes.filter(|total| *total > 0)?;
    let free_before = snapshot.free_bytes?;
    let music = snapshot
        .reprise_music_bytes
        .checked_add(snapshot.other_music_bytes)?;
    let known = music
        .checked_add(youtube_bytes)?
        .checked_add(podcast_bytes)?
        .checked_add(free_before)?;
    let other = total.checked_sub(known)?;
    let free_after = free_before
        .saturating_sub(incoming_bytes)
        .saturating_add(freed_bytes)
        .min(total);
    Some(CategorySegments {
        music_bytes: music,
        youtube_bytes,
        podcast_bytes,
        other_bytes: other,
        incoming_bytes,
        free_before_bytes: free_before,
        free_after_bytes: free_after,
        total_bytes: total,
    })
}

/// `MTP-37`: one category's row in the device view's "Content" section
/// (design 7a) — target folder, per-device activation, size already on the
/// device, and the cap (`E-6`: editable here, not a read-only mirror of a
/// Preferences page that no longer exists). Deliberately excludes anything
/// about *what* content is wanted: that summary is read from the existing
/// selection projections (`selection::summarize_playlist_selection`,
/// `selection::summarize_youtube_selection`,
/// `podcasts::phone_sync::selection_summary`) by the caller — this type
/// only ever carries the per-device half.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryContentRow {
    pub kind: super::targets::SyncTargetKind,
    pub target_path: String,
    /// `SyncTarget::enabled` — the one per-device toggle this row owns.
    /// Independent of any global "sync this content type" rule (`MTP-38`'s
    /// doc comment); there is no second, competing toggle here.
    pub target_enabled: bool,
    pub size_on_device_bytes: u64,
    pub cap_bytes: Option<u64>,
}

/// `MTP-37`: thin, pure translation from a [`SyncTarget`] plus its already-
/// summed on-device size into the row the Content section renders.
#[must_use]
pub fn project_category_content_row(
    target: &SyncTarget,
    size_on_device_bytes: u64,
) -> CategoryContentRow {
    CategoryContentRow {
        kind: target.kind,
        target_path: target.path.clone(),
        target_enabled: target.enabled,
        size_on_device_bytes,
        cap_bytes: target.cap_bytes,
    }
}

/// `MTP-37`: a category's `MTP-22` reading, gated by this device's target
/// activation only. `E-6` withdrew the planned global "sync this content
/// type" rule (7b's Phone sync block) outright — it will not be built — so
/// `SyncTarget::enabled` is, and stays, the only switch this reading
/// depends on. Connectivity does not gate this
/// either: every category's candidates are read from the local database and
/// MTP inventory, never from the network, so
/// [`CategoryReading::UnavailableKeptOnPhone`] cannot occur through this
/// path — that state stays reachable only through
/// [`super::category_diff::candidate_source`] for callers that do cross a
/// network boundary.
#[must_use]
pub fn project_device_category_reading(target: &SyncTarget, diff: CategoryDiff) -> CategoryReading {
    project_category_reading(target.enabled, true, CandidateSource::Computed(diff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_sync::targets::SyncTargetKind;

    fn file(size_bytes: u64) -> ManagedDeviceFile {
        ManagedDeviceFile {
            relative_path: "a.opus".into(),
            size_bytes,
        }
    }

    fn snapshot(
        total: u64,
        free: u64,
        reprise_music: u64,
        other_music: u64,
    ) -> DeviceStorageSnapshot {
        DeviceStorageSnapshot {
            target_name: Some("Pixel 8".into()),
            access: super::super::storage::DeviceStorageAccess::Writable,
            total_bytes: Some(total),
            free_bytes: Some(free),
            reprise_music_bytes: reprise_music,
            other_music_bytes: other_music,
        }
    }

    #[test]
    fn mtp_26_never_verified_before_any_successful_inspection() {
        assert_eq!(
            project_contents_state(false, None, false),
            DeviceContentsState::NeverVerified
        );
    }

    #[test]
    fn mtp_26_verifying_takes_priority_over_a_stale_error_or_prior_success() {
        assert_eq!(
            project_contents_state(true, Some("stale error"), true),
            DeviceContentsState::Verifying
        );
        assert!(!DeviceContentsState::Verifying.can_scan());
    }

    #[test]
    fn mtp_26_a_failed_scan_is_distinct_from_never_having_scanned() {
        let failed = project_contents_state(false, Some("MTP timeout"), false);
        assert_eq!(failed, DeviceContentsState::Failed("MTP timeout".into()));
        assert!(failed.can_scan(), "a failed scan must still offer retry");
        assert_ne!(failed, DeviceContentsState::NeverVerified);
    }

    #[test]
    fn mtp_26_verified_only_after_a_real_successful_inspection() {
        assert_eq!(
            project_contents_state(false, None, true),
            DeviceContentsState::Verified
        );
    }

    #[test]
    fn mtp_27_category_bytes_sums_the_inventory_and_saturates() {
        assert_eq!(category_bytes(&[]), 0);
        assert_eq!(category_bytes(&[file(100), file(250)]), 350);
        assert_eq!(
            category_bytes(&[file(u64::MAX), file(1)]),
            u64::MAX,
            "an impossible sum reads as very large, not wrapped"
        );
    }

    #[test]
    fn mtp_27_segments_separate_music_youtube_podcasts_other_incoming_and_free() {
        // 100 GiB total: 20 music, 8 youtube, 4 podcasts, 60 free, 8 other.
        const GIB: u64 = 1024 * 1024 * 1024;
        let snapshot = snapshot(100 * GIB, 60 * GIB, 12 * GIB, 8 * GIB);

        let segments =
            project_category_segments(&snapshot, 8 * GIB, 4 * GIB, 3 * GIB, GIB).unwrap();

        assert_eq!(segments.music_bytes, 20 * GIB);
        assert_eq!(segments.youtube_bytes, 8 * GIB);
        assert_eq!(segments.podcast_bytes, 4 * GIB);
        assert_eq!(segments.other_bytes, 8 * GIB);
        assert_eq!(segments.incoming_bytes, 3 * GIB);
        assert_eq!(segments.free_before_bytes, 60 * GIB);
        assert_eq!(
            segments.free_after_bytes,
            60 * GIB - 3 * GIB + GIB,
            "175.0 GiB free -> 172.4 GiB after this sync, in miniature"
        );
        assert_eq!(segments.total_bytes, 100 * GIB);
        assert_eq!(
            segments.music_bytes
                + segments.youtube_bytes
                + segments.podcast_bytes
                + segments.other_bytes
                + segments.free_before_bytes,
            segments.total_bytes,
            "every byte on the bar accounts for the whole disk exactly once"
        );
    }

    #[test]
    fn mtp_27_segments_disappear_rather_than_invent_a_share_when_inconsistent() {
        const GIB: u64 = 1024 * 1024 * 1024;
        // Music + free already exceeds total before youtube/podcasts even land.
        let snapshot = snapshot(10 * GIB, 8 * GIB, 5 * GIB, 0);

        assert_eq!(
            project_category_segments(&snapshot, 0, 0, 0, 0),
            None,
            "an impossible breakdown must not silently render a bar"
        );
    }

    #[test]
    fn mtp_27_segments_are_none_without_full_capacity_knowledge() {
        let mut snapshot = snapshot(10, 5, 1, 1);
        snapshot.total_bytes = None;
        assert_eq!(project_category_segments(&snapshot, 0, 0, 0, 0), None);
    }

    #[test]
    fn mtp_37_content_row_reads_the_targets_per_device_folder_activation_and_cap() {
        let target = SyncTarget {
            kind: SyncTargetKind::YoutubeAudio,
            storage_id: None,
            path: "/Music/Reprise-YouTube".into(),
            enabled: false,
            cap_bytes: Some(8 * 1024 * 1024 * 1024),
        };

        let row = project_category_content_row(&target, 42);

        assert_eq!(row.kind, SyncTargetKind::YoutubeAudio);
        assert_eq!(row.target_path, "/Music/Reprise-YouTube");
        assert!(!row.target_enabled);
        assert_eq!(row.size_on_device_bytes, 42);
        assert_eq!(row.cap_bytes, Some(8 * 1024 * 1024 * 1024));
    }

    #[test]
    fn mtp_37_a_disabled_target_reads_source_off_even_with_a_nonzero_diff() {
        let target = SyncTarget {
            kind: SyncTargetKind::PodcastEpisodes,
            storage_id: None,
            path: "/Podcasts/Reprise".into(),
            enabled: false,
            cap_bytes: None,
        };
        let would_have_had_changes = CategoryDiff {
            files_to_copy: 5,
            bytes_to_copy: 1_000,
            files_to_remove: 0,
            bytes_freed: 0,
            files_waiting_for_download: 0,
            playlists_rewritten: 0,
        };

        assert_eq!(
            project_device_category_reading(&target, would_have_had_changes),
            CategoryReading::SourceOff
        );
    }

    #[test]
    fn mtp_37_an_enabled_target_reads_its_computed_diff() {
        let target = SyncTarget {
            kind: SyncTargetKind::Playlists,
            storage_id: None,
            path: "/Music/Reprise".into(),
            enabled: true,
            cap_bytes: None,
        };
        let diff = CategoryDiff {
            files_to_copy: 14,
            bytes_to_copy: 2,
            files_to_remove: 3,
            bytes_freed: 1,
            files_waiting_for_download: 0,
            playlists_rewritten: 2,
        };

        assert_eq!(
            project_device_category_reading(&target, diff),
            CategoryReading::Diff(diff)
        );
    }
}
