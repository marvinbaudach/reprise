//! Toolkit-free device synchronization page projections.
//!
//! Inputs are core state or narrow plain values. Translatable output crosses
//! the surface boundary as [`Message`]; the GNOME adapter owns gettext, local
//! time formatting, and conversion from its GTK-owned preparation state.

use crate::strings::{Message, Plural};
use reprise_core::device_sync::{
    DeviceSessionState, MirrorBlocker, Mp3Quality, PlannedSyncPhase, SyncChangeSummary,
    SyncPageControls, SyncPageWarning, SyncPlaylistRow, SyncStep, TransferProfile,
};

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

const fn plural(singular: &'static str, plural: &'static str) -> (&'static str, &'static str) {
    (singular, plural)
}

const PROFILE_OPUS: &str = N_!("Opus · 160 kbit/s (Recommended)");
const PROFILE_MP3: &str = N_!("MP3 · 256 kbit/s (Compatibility)");
const PROFILE_ORIGINAL: &str = N_!("Original files (no conversion)");
const PLAYLIST_UNAVAILABLE: &str = N_!("Playlist no longer exists — deselect it to continue");
const SMART_SNAPSHOT: &str = N_!("Smart snapshot");
const ENTRIES: (&str, &str) = plural("{count} entry", "{count} entries");
const UNIQUE_TRACKS: (&str, &str) = plural("{count} unique track", "{count} unique tracks");
const UNAVAILABLE_TRACKS: (&str, &str) =
    plural("{count} unavailable track", "{count} unavailable tracks");
const NO_VERIFIED_PLAYLIST_SYNC: &str = N_!("{size} · No verified sync time");
const UNAVAILABLE_VERIFIED_PLAYLIST_SYNC: &str = N_!("{size} · Verified sync time unavailable");
const LAST_VERIFIED_PLAYLIST_SYNC: &str = N_!("{size} · Last synced {timestamp}");
const NEVER_SYNCHRONIZED: &str = N_!("Never synchronized");
const LAST_SYNCED: &str = N_!("Last synced {timestamp}");
const SIZE_ON_DEVICE: &str = N_!("{size} on device when last verified");
const NEW: &str = N_!("{count} new");
const UPDATED: &str = N_!("{count} updated");
const REMOVED: &str = N_!("{count} removed");
const UNAVAILABLE_KEPT: &str = N_!("{count} unavailable kept");
const PLAYLISTS_WRITTEN: (&str, &str) =
    plural("{count} playlist written", "{count} playlists written");
const PLAYLISTS_REMOVED: (&str, &str) =
    plural("{count} playlist removed", "{count} playlists removed");
const TRANSFERRED: &str = N_!("{size} transferred");
const VERIFYING_CONTENTS: &str = N_!("Verifying device contents…");
const VERIFIED_TRACKS: (&str, &str) = plural(
    "Verified · {count} Reprise track on device",
    "Verified · {count} Reprise tracks on device",
);
const VERIFIED_AFTER_SYNC: &str = N_!("Verified after synchronization");
const NOT_VERIFIED: &str = N_!("Not verified in this session");
const SELECT_PLAYLIST: &str = N_!("Select at least one playlist to synchronize.");
const MISSING_PLAYLISTS: (&str, &str) = plural(
    "{count} selected playlist no longer exists",
    "{count} selected playlists no longer exist",
);
const DUPLICATE_PLAYLISTS: (&str, &str) = plural(
    "{count} playlist is selected twice",
    "{count} playlists are selected twice",
);
const UNAVAILABLE_WARNING: (&str, &str) = plural(
    "{count} track will be skipped because it is unavailable and not already on the device.",
    "{count} tracks will be skipped because they are unavailable and not already on the device.",
);
const UNSAFE_WARNING: &str = N_!("An unsafe managed path will be left untouched.");
const CANCEL: &str = N_!("_Cancel");
const SYNC_NOW: &str = N_!("_Sync now");
const CHECKING_TITLE: &str = N_!("Checking device…");
const CHECKING_SUBTITLE: &str = N_!("Reading storage and preparing the mirror plan");
const FINISHING_TITLE: &str = N_!("Finishing synchronization…");
const FINISHING_SUBTITLE: &str = N_!("Refreshing the device inventory");
const REMOVING_TITLE: &str = N_!("Removing · {done} of {total}");
const CONVERTING_TITLE: &str = N_!("Converting · {done} of {total}");
const COPYING_TITLE: &str = N_!("Copying · {done} of {total}");
const WRITING_ANALYSIS_TITLE: &str = N_!("Writing analysis · {done} of {total}");
const WRITING_LYRICS_TITLE: &str = N_!("Writing lyrics · {done} of {total}");
const WRITING_PLAYLISTS_TITLE: &str = N_!("Writing playlists · {done} of {total}");
const WRITING_TRACK_METADATA_TITLE: &str = N_!("Writing track metadata · {done} of {total}");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageActionCopy {
    pub label: &'static str,
    pub sensitive: bool,
    pub destructive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifiedSyncTime {
    Never,
    Unavailable,
    Formatted(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockerCopy {
    Standalone(Message),
    Reasons(Vec<Message>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressSubtitle {
    Message(Message),
    CurrentTrack(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressSpeed {
    Unavailable,
    BytesPerSecond(u64),
}

/// Named boundary record replacing the four anonymous progress values.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressCopy {
    pub title: Message,
    pub subtitle: ProgressSubtitle,
    pub speed: ProgressSpeed,
    pub fraction: f64,
}

pub fn profile_label(profile: TransferProfile) -> &'static str {
    match profile {
        TransferProfile::Opus160 => PROFILE_OPUS,
        TransferProfile::Mp3(Mp3Quality::Kbps256) => PROFILE_MP3,
        TransferProfile::Original => PROFILE_ORIGINAL,
    }
}

pub fn playlist_subtitle(row: &SyncPlaylistRow, last_sync: VerifiedSyncTime) -> Vec<Message> {
    if !row.available {
        return vec![message(PLAYLIST_UNAVAILABLE)];
    }
    let mut parts = Vec::new();
    if row.smart {
        parts.push(message(SMART_SNAPSHOT));
    }
    parts.push(counted(row.entry_count, ENTRIES.0, ENTRIES.1));
    parts.push(counted(
        row.unique_track_count,
        UNIQUE_TRACKS.0,
        UNIQUE_TRACKS.1,
    ));
    if row.unavailable_count > 0 {
        parts.push(counted(
            row.unavailable_count,
            UNAVAILABLE_TRACKS.0,
            UNAVAILABLE_TRACKS.1,
        ));
    }
    let size = format_bytes(row.target_bytes);
    parts.push(match last_sync {
        VerifiedSyncTime::Never => {
            message_with_args(NO_VERIFIED_PLAYLIST_SYNC, vec![("size", size)])
        }
        VerifiedSyncTime::Unavailable => {
            message_with_args(UNAVAILABLE_VERIFIED_PLAYLIST_SYNC, vec![("size", size)])
        }
        VerifiedSyncTime::Formatted(timestamp) => message_with_args(
            LAST_VERIFIED_PLAYLIST_SYNC,
            vec![("size", size), ("timestamp", timestamp)],
        ),
    });
    parts
}

pub fn unique_tracks(count: usize) -> Message {
    counted(count, UNIQUE_TRACKS.0, UNIQUE_TRACKS.1)
}

pub fn device_last_sync_copy(
    phase: &PlannedSyncPhase,
    last_sync: Option<String>,
    session_state: &DeviceSessionState,
    size_on_device_bytes: Option<u64>,
    verified_managed_track_count: Option<usize>,
) -> Vec<Message> {
    if phase == &PlannedSyncPhase::Finishing {
        return vec![verification_summary(
            phase,
            last_sync.is_some(),
            verified_managed_track_count,
        )];
    }
    let has_last_sync = last_sync.is_some();
    let history = last_sync.map_or_else(
        || message(NEVER_SYNCHRONIZED),
        |timestamp| message_with_args(LAST_SYNCED, vec![("timestamp", timestamp)]),
    );
    let mut copy = vec![history];
    if session_state == &DeviceSessionState::Remembered {
        if let Some(bytes) = size_on_device_bytes {
            copy.push(message_with_args(
                SIZE_ON_DEVICE,
                vec![("size", format_bytes(bytes))],
            ));
        }
    } else if verified_managed_track_count.is_some() {
        copy.push(verification_summary(
            phase,
            has_last_sync,
            verified_managed_track_count,
        ));
    }
    copy
}

pub fn change_summary(changes: &SyncChangeSummary) -> Vec<Message> {
    vec![
        count_message(NEW, changes.additions),
        count_message(UPDATED, changes.replacements),
        count_message(REMOVED, changes.removals),
        count_message(UNAVAILABLE_KEPT, changes.retained_unavailable),
        counted(
            changes.playlist_writes,
            PLAYLISTS_WRITTEN.0,
            PLAYLISTS_WRITTEN.1,
        ),
        counted(
            changes.playlist_removals,
            PLAYLISTS_REMOVED.0,
            PLAYLISTS_REMOVED.1,
        ),
        message_with_args(
            TRANSFERRED,
            vec![("size", format_bytes(changes.transfer_bytes))],
        ),
    ]
}

pub fn verification_summary(
    phase: &PlannedSyncPhase,
    has_last_sync: bool,
    verified_managed_track_count: Option<usize>,
) -> Message {
    if phase == &PlannedSyncPhase::Finishing {
        return message(VERIFYING_CONTENTS);
    }
    match (has_last_sync, verified_managed_track_count) {
        (true, Some(count)) => counted(count, VERIFIED_TRACKS.0, VERIFIED_TRACKS.1),
        (true, None) => message(VERIFIED_AFTER_SYNC),
        (false, _) => message(NOT_VERIFIED),
    }
}

pub fn blocker_summary(blockers: &[MirrorBlocker]) -> Option<BlockerCopy> {
    if blockers.is_empty() {
        return None;
    }
    if blockers.contains(&MirrorBlocker::NoPlaylistsSelected) {
        return Some(BlockerCopy::Standalone(message(SELECT_PLAYLIST)));
    }
    let missing = blockers
        .iter()
        .filter(|blocker| matches!(blocker, MirrorBlocker::MissingPlaylist(_)))
        .count();
    let duplicate = blockers
        .iter()
        .filter(|blocker| matches!(blocker, MirrorBlocker::DuplicatePlaylist(_)))
        .count();
    let mut reasons = Vec::new();
    if missing > 0 {
        reasons.push(counted(missing, MISSING_PLAYLISTS.0, MISSING_PLAYLISTS.1));
    }
    if duplicate > 0 {
        reasons.push(counted(
            duplicate,
            DUPLICATE_PLAYLISTS.0,
            DUPLICATE_PLAYLISTS.1,
        ));
    }
    Some(BlockerCopy::Reasons(reasons))
}

pub fn warning_summary(warnings: &[SyncPageWarning]) -> Vec<Message> {
    let unavailable = warnings
        .iter()
        .filter(|warning| matches!(warning, SyncPageWarning::UnavailableNotOnDevice { .. }))
        .count();
    let mut summary = Vec::new();
    if unavailable > 0 {
        summary.push(counted(
            unavailable,
            UNAVAILABLE_WARNING.0,
            UNAVAILABLE_WARNING.1,
        ));
    }
    if warnings.contains(&SyncPageWarning::UnsafeManagedItem) {
        summary.push(message(UNSAFE_WARNING));
    }
    summary
}

pub fn action_copy(controls: SyncPageControls) -> PageActionCopy {
    if controls.can_cancel {
        PageActionCopy {
            label: CANCEL,
            sensitive: true,
            destructive: true,
        }
    } else {
        PageActionCopy {
            label: SYNC_NOW,
            sensitive: controls.can_start,
            destructive: false,
        }
    }
}

pub fn eject_sensitive(
    controls: SyncPageControls,
    connected: bool,
    phase: &PlannedSyncPhase,
) -> bool {
    controls.can_eject
        && connected
        && !matches!(
            phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        )
}

pub fn counted(count: usize, singular: &'static str, plural: &'static str) -> Message {
    Message {
        id: singular,
        plural: Some(Plural {
            id: plural,
            count: u64::try_from(count).unwrap_or(u64::MAX),
        }),
        args: vec![("count", count.to_string())],
    }
}

pub fn transfer_progress_copy(
    phase: &PlannedSyncPhase,
    bytes_per_second: u64,
) -> Option<ProgressCopy> {
    match phase {
        PlannedSyncPhase::Idle => None,
        PlannedSyncPhase::ComputingDelta => Some(ProgressCopy {
            title: message(CHECKING_TITLE),
            subtitle: ProgressSubtitle::Message(message(CHECKING_SUBTITLE)),
            speed: ProgressSpeed::Unavailable,
            fraction: 0.0,
        }),
        PlannedSyncPhase::Finishing => Some(ProgressCopy {
            title: message(FINISHING_TITLE),
            subtitle: ProgressSubtitle::Message(message(FINISHING_SUBTITLE)),
            speed: ProgressSpeed::Unavailable,
            fraction: 1.0,
        }),
        PlannedSyncPhase::Syncing {
            step,
            done,
            total,
            current_track,
            unit_bytes_done,
            unit_bytes_total,
        } => {
            let title = match step {
                SyncStep::Removing => REMOVING_TITLE,
                SyncStep::Transcoding => CONVERTING_TITLE,
                SyncStep::Copying => COPYING_TITLE,
                SyncStep::WritingAnalysis => WRITING_ANALYSIS_TITLE,
                SyncStep::WritingLyrics => WRITING_LYRICS_TITLE,
                SyncStep::WritingPlaylists => WRITING_PLAYLISTS_TITLE,
                SyncStep::WritingTrackMetadata => WRITING_TRACK_METADATA_TITLE,
            };
            let unit_fraction = if *unit_bytes_total > 0 {
                *unit_bytes_done as f64 / *unit_bytes_total as f64
            } else {
                0.0
            };
            let fraction = if *total > 0 {
                (f64::from(*done) + unit_fraction) / f64::from(*total)
            } else {
                0.0
            };
            let speed = if step.reports_transfer_rate() && bytes_per_second > 0 {
                ProgressSpeed::BytesPerSecond(bytes_per_second)
            } else {
                ProgressSpeed::Unavailable
            };
            Some(ProgressCopy {
                title: message_with_args(
                    title,
                    vec![("done", done.to_string()), ("total", total.to_string())],
                ),
                subtitle: ProgressSubtitle::CurrentTrack(current_track.clone()),
                speed,
                fraction: fraction.clamp(0.0, 1.0),
            })
        }
    }
}

fn message(id: &'static str) -> Message {
    Message {
        id,
        plural: None,
        args: Vec::new(),
    }
}

fn message_with_args(id: &'static str, args: Vec<(&'static str, String)>) -> Message {
    Message {
        id,
        plural: None,
        args,
    }
}

fn count_message(id: &'static str, count: usize) -> Message {
    message_with_args(id, vec![("count", count.to_string())])
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1_024.0;
    const MIB: f64 = KIB * 1_024.0;
    const GIB: f64 = MIB * 1_024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::device_sync::{PlannedSyncPhase, SyncStep};

    use super::*;

    #[test]
    fn transfer_progress_exposes_named_fields_and_an_unrendered_message() {
        let progress = transfer_progress_copy(
            &PlannedSyncPhase::Syncing {
                step: SyncStep::Copying,
                done: 1,
                total: 2,
                current_track: "Immortal — Lorna Shore".into(),
                unit_bytes_done: 50,
                unit_bytes_total: 100,
            },
            2 * 1_024 * 1_024,
        )
        .unwrap();

        assert_eq!(
            progress,
            ProgressCopy {
                title: Message {
                    id: "Copying · {done} of {total}",
                    plural: None,
                    args: vec![("done", "1".into()), ("total", "2".into())],
                },
                subtitle: ProgressSubtitle::CurrentTrack("Immortal — Lorna Shore".into()),
                speed: ProgressSpeed::BytesPerSecond(2 * 1_024 * 1_024),
                fraction: 0.75,
            }
        );
    }
}
