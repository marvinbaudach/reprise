//! Pure presentation copy for the device page — split out of
//! `device_sync_page.rs` to keep that file under the project's 800-line
//! limit as the preparation surface (`MTP-43`) grew its own progress and
//! button-label projections. Every function here is a plain projection over
//! already-computed state; none of it touches a widget.

use chrono::TimeZone;
use reprise_core::device_sync::{
    MirrorBlocker, Mp3Quality, PrimaryAction, SyncChangeSummary, SyncPageControls, SyncPageWarning,
    SyncPlaylistRow, TransferProfile,
};

use super::device_sync_runtime::{DeviceView, PlannedSyncPhase, PreparationRunState, SyncStep};
use super::device_sync_strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PageActionCopy {
    pub(super) label: &'static str,
    pub(super) sensitive: bool,
    pub(super) destructive: bool,
}

pub(super) fn profile_label(profile: TransferProfile) -> &'static str {
    match profile {
        TransferProfile::Opus160 => "Opus · 160 kbit/s (Recommended)",
        TransferProfile::Mp3(Mp3Quality::Kbps256) => "MP3 · 256 kbit/s (Compatibility)",
        TransferProfile::Original => "Original files (no conversion)",
    }
}

pub(super) fn playlist_subtitle(row: &SyncPlaylistRow) -> String {
    if !row.available {
        return "Playlist no longer exists — deselect it to continue".into();
    }
    let mut parts = Vec::new();
    if row.smart {
        parts.push("Smart snapshot".into());
    }
    parts.push(counted(row.entry_count, "entry", "entries"));
    parts.push(counted(
        row.unique_track_count,
        "unique track",
        "unique tracks",
    ));
    if row.unavailable_count > 0 {
        parts.push(counted(
            row.unavailable_count,
            "unavailable track",
            "unavailable tracks",
        ));
    }
    parts.push(device_sync_strings::file_size(row.target_bytes));
    parts.push(playlist_last_sync_copy(row.last_synced_at));
    parts.join(" · ")
}

fn playlist_last_sync_copy(last_synced_at: Option<i64>) -> String {
    let Some(last_synced_at) = last_synced_at else {
        return "No verified sync time".into();
    };
    chrono::Local
        .timestamp_opt(last_synced_at, 0)
        .single()
        .map_or_else(
            || "Verified sync time unavailable".into(),
            |timestamp| format!("Last synced {}", timestamp.format("%b %-d, %Y at %H:%M")),
        )
}

pub(super) fn device_last_sync_copy(device: &DeviceView) -> String {
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return verification_summary(device);
    }
    let history = device.last_sync.as_ref().map_or_else(
        || "Never synchronized".into(),
        |timestamp| {
            format!(
                "Last synced {}",
                timestamp
                    .with_timezone(&chrono::Local)
                    .format("%b %-d, %Y at %H:%M")
            )
        },
    );
    device
        .verified_managed_track_count
        .map_or(history.clone(), |_| {
            format!("{history} · {}", verification_summary(device))
        })
}

pub(super) fn change_summary(changes: &SyncChangeSummary) -> String {
    [
        counted(changes.additions, "new", "new"),
        counted(changes.replacements, "updated", "updated"),
        counted(changes.removals, "removed", "removed"),
        counted(
            changes.retained_unavailable,
            "unavailable kept",
            "unavailable kept",
        ),
        counted(
            changes.playlist_writes,
            "playlist written",
            "playlists written",
        ),
        counted(
            changes.playlist_removals,
            "playlist removed",
            "playlists removed",
        ),
        format!(
            "{} transferred",
            device_sync_strings::file_size(changes.transfer_bytes)
        ),
    ]
    .join(" · ")
}

pub(super) fn verification_summary(device: &DeviceView) -> String {
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return "Verifying device contents…".into();
    }
    match (device.last_sync, device.verified_managed_track_count) {
        (Some(_), Some(count)) => format!(
            "Verified · {} on device",
            counted(count, "Reprise track", "Reprise tracks")
        ),
        (Some(_), None) => "Verified after synchronization".into(),
        (None, _) => "Not verified in this session".into(),
    }
}

pub(super) fn blocker_summary(blockers: &[MirrorBlocker]) -> Option<String> {
    if blockers.is_empty() {
        return None;
    }
    if blockers
        .iter()
        .any(|blocker| blocker == &MirrorBlocker::NoPlaylistsSelected)
    {
        return Some("Select at least one playlist to synchronize.".into());
    }
    let missing = blockers
        .iter()
        .filter(|blocker| matches!(blocker, MirrorBlocker::MissingPlaylist(_)))
        .count();
    let duplicate = blockers
        .iter()
        .filter(|blocker| matches!(blocker, MirrorBlocker::DuplicatePlaylist(_)))
        .count();
    let mut parts = Vec::new();
    if missing > 0 {
        parts.push(counted(
            missing,
            "selected playlist no longer exists",
            "selected playlists no longer exist",
        ));
    }
    if duplicate > 0 {
        parts.push(counted(
            duplicate,
            "playlist is selected twice",
            "playlists are selected twice",
        ));
    }
    Some(format!("Cannot synchronize: {}.", parts.join(" · ")))
}

pub(super) fn warning_summary(warnings: &[SyncPageWarning]) -> Vec<String> {
    let unavailable = warnings
        .iter()
        .filter(|warning| matches!(warning, SyncPageWarning::UnavailableNotOnDevice { .. }))
        .count();
    let mut summary = Vec::new();
    if unavailable == 1 {
        summary.push(
            "1 track will be skipped because it is unavailable and not already on the device."
                .into(),
        );
    } else if unavailable > 1 {
        summary.push(format!(
            "{unavailable} tracks will be skipped because they are unavailable and not already on the device."
        ));
    }
    if warnings.contains(&SyncPageWarning::UnsafeManagedItem) {
        summary.push("An unsafe managed path will be left untouched.".into());
    }
    summary
}

/// `MTP-43`: the primary button reads "Download & sync" exactly when
/// `primary_action` answers `DownloadAndSync` (`MTP-42`'s only phase that
/// starts a download alongside the sync) — every other phase, `Absent`
/// included, keeps the plain "Sync now" it always read.
pub(super) fn action_copy(controls: SyncPageControls, action: PrimaryAction) -> PageActionCopy {
    if controls.can_cancel {
        PageActionCopy {
            label: "_Cancel",
            sensitive: true,
            destructive: true,
        }
    } else {
        PageActionCopy {
            label: match action {
                PrimaryAction::DownloadAndSync => "_Download & sync",
                PrimaryAction::SyncNow => "_Sync now",
            },
            sensitive: controls.can_start,
            destructive: false,
        }
    }
}

pub(super) fn eject_sensitive(device: &DeviceView) -> bool {
    device.page.controls.can_eject
        && device.connected
        && !matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        )
}

pub(super) fn counted(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

/// `MTP-43`'s two-phase progress reading: a `Downloading` preparation run
/// always wins and is reported as step 1 of 2; otherwise the existing
/// transfer progress is used as-is, prefixed with "Step 2 of 2" only when
/// `prepared_this_run` says this run's transfer was actually preceded by a
/// preparation download — a plain sync with no preparation phase must keep
/// reading single-phase, exactly as it always has.
pub(super) fn progress_copy(device: &DeviceView) -> Option<(String, String, String, f64)> {
    if let PreparationRunState::Downloading {
        done,
        total,
        title,
        received_bytes,
        total_bytes,
    } = &device.preparation_run
    {
        let fraction = total_bytes
            .filter(|total_bytes| *total_bytes > 0)
            .map_or(0.0, |total_bytes| {
                (*received_bytes as f64 / total_bytes as f64).clamp(0.0, 1.0)
            });
        let percent = (fraction * 100.0).round() as u64;
        return Some((
            device_sync_strings::preparation_step_progress(*done, *total, percent),
            title.clone(),
            "—".into(),
            fraction,
        ));
    }
    let transfer = transfer_progress_copy(&device.sync_phase, device.bytes_per_second)?;
    if device.prepared_this_run {
        let (title, subtitle, speed, fraction) = transfer;
        return Some((
            device_sync_strings::two_phase_title(&title),
            subtitle,
            speed,
            fraction,
        ));
    }
    Some(transfer)
}

pub(super) fn transfer_progress_copy(
    phase: &PlannedSyncPhase,
    bytes_per_second: u64,
) -> Option<(String, String, String, f64)> {
    match phase {
        PlannedSyncPhase::Idle => None,
        PlannedSyncPhase::ComputingDelta => Some((
            "Checking device…".into(),
            "Reading storage and preparing the mirror plan".into(),
            "—".into(),
            0.0,
        )),
        PlannedSyncPhase::Finishing => Some((
            "Finishing synchronization…".into(),
            "Refreshing the device inventory".into(),
            "—".into(),
            1.0,
        )),
        PlannedSyncPhase::Syncing {
            step,
            done,
            total,
            current_track,
            bytes_done,
            bytes_total,
        } => {
            let is_copying = *step == SyncStep::Copying;
            let step = match step {
                SyncStep::Removing => "Removing",
                SyncStep::Transcoding => "Converting",
                SyncStep::Copying => "Copying",
                SyncStep::WritingPlaylists => "Writing playlists",
            };
            let fraction = if *bytes_total > 0 {
                *bytes_done as f64 / *bytes_total as f64
            } else if *total > 0 {
                f64::from(*done) / f64::from(*total)
            } else {
                0.0
            };
            let speed = if is_copying && bytes_per_second > 0 {
                format!("{}/s", device_sync_strings::file_size(bytes_per_second))
            } else {
                "—".into()
            };
            Some((
                format!("{step} · {done} of {total}"),
                current_track.clone(),
                speed,
                fraction.clamp(0.0, 1.0),
            ))
        }
    }
}
