//! Pure presentation copy for the device page — split out of
//! `device_sync_page.rs` to keep that file under the project's 800-line
//! limit as the preparation surface (`MTP-43`) grew its own progress and
//! button-label projections.
//!
//! The `projection` module is the toolkit-free seam. The surrounding functions
//! are the GNOME adapter: they narrow the GTK-owned `DeviceView`, format local
//! time, and render [`reprise_view::strings::Message`] values through gettext.

use chrono::TimeZone;
use reprise_core::device_sync::{
    MirrorBlocker, PrimaryAction, SyncChangeSummary, SyncPageControls, SyncPageWarning,
    SyncPlaylistRow, TransferProfile,
};
use reprise_view::strings::Message;

use super::device_sync_runtime::{DeviceView, PlannedSyncPhase, PreparationRunState};
use super::device_sync_strings;

mod projection {
    use reprise_core::device_sync::{
        DeviceSessionState, MirrorBlocker, Mp3Quality, PlannedSyncPhase, PrimaryAction,
        SyncChangeSummary, SyncPageControls, SyncPageWarning, SyncPlaylistRow, SyncStep,
        TransferProfile,
    };
    use reprise_view::strings::{Message, Plural};

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
    const NEW: (&str, &str) = plural("{count} new", "{count} new");
    const UPDATED: (&str, &str) = plural("{count} updated", "{count} updated");
    const REMOVED: (&str, &str) = plural("{count} removed", "{count} removed");
    const UNAVAILABLE_KEPT: (&str, &str) =
        plural("{count} unavailable kept", "{count} unavailable kept");
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
    const DOWNLOAD_AND_SYNC: &str = N_!("_Download & sync");
    const SYNC_NOW: &str = N_!("_Sync now");
    const PREPARING_TITLE: &str =
        N_!("Step 1 of 2 · Downloading {current} of {total} · {percent}%");
    const CHECKING_TITLE: &str = N_!("Checking device…");
    const CHECKING_SUBTITLE: &str = N_!("Reading storage and preparing the mirror plan");
    const FINISHING_TITLE: &str = N_!("Finishing synchronization…");
    const FINISHING_SUBTITLE: &str = N_!("Refreshing the device inventory");
    const REMOVING_TITLE: &str = N_!("Removing · {done} of {total}");
    const CONVERTING_TITLE: &str = N_!("Converting · {done} of {total}");
    const COPYING_TITLE: &str = N_!("Copying · {done} of {total}");
    const WRITING_PLAYLISTS_TITLE: &str = N_!("Writing playlists · {done} of {total}");
    const REMOVING_TITLE_STEP_TWO: &str = N_!("Step 2 of 2 · Removing · {done} of {total}");
    const CONVERTING_TITLE_STEP_TWO: &str = N_!("Step 2 of 2 · Converting · {done} of {total}");
    const COPYING_TITLE_STEP_TWO: &str = N_!("Step 2 of 2 · Copying · {done} of {total}");
    const WRITING_PLAYLISTS_TITLE_STEP_TWO: &str =
        N_!("Step 2 of 2 · Writing playlists · {done} of {total}");

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ui::device_sync) struct PageActionCopy {
        pub(in crate::ui::device_sync) label: &'static str,
        pub(in crate::ui::device_sync) sensitive: bool,
        pub(in crate::ui::device_sync) destructive: bool,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) enum VerifiedSyncTime {
        Never,
        Unavailable,
        Formatted(String),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) enum BlockerCopy {
        Standalone(Message),
        Reasons(Vec<Message>),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct PreparationProgress {
        pub(super) done: usize,
        pub(super) total: usize,
        pub(super) title: String,
        pub(super) received_bytes: u64,
        pub(super) total_bytes: Option<u64>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) enum ProgressSubtitle {
        Message(Message),
        CurrentTrack(String),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum ProgressSpeed {
        Unavailable,
        BytesPerSecond(u64),
    }

    /// Named boundary record replacing the four anonymous progress values.
    #[derive(Clone, Debug, PartialEq)]
    pub(super) struct ProgressCopy {
        pub(super) title: Message,
        pub(super) subtitle: ProgressSubtitle,
        pub(super) speed: ProgressSpeed,
        pub(super) fraction: f64,
    }

    pub(super) fn profile_label(profile: TransferProfile) -> &'static str {
        match profile {
            TransferProfile::Opus160 => PROFILE_OPUS,
            TransferProfile::Mp3(Mp3Quality::Kbps256) => PROFILE_MP3,
            TransferProfile::Original => PROFILE_ORIGINAL,
        }
    }

    pub(super) fn playlist_subtitle(
        row: &SyncPlaylistRow,
        last_sync: VerifiedSyncTime,
    ) -> Vec<Message> {
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

    pub(super) fn device_last_sync_copy(
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

    pub(super) fn change_summary(changes: &SyncChangeSummary) -> Vec<Message> {
        vec![
            counted(changes.additions, NEW.0, NEW.1),
            counted(changes.replacements, UPDATED.0, UPDATED.1),
            counted(changes.removals, REMOVED.0, REMOVED.1),
            counted(
                changes.retained_unavailable,
                UNAVAILABLE_KEPT.0,
                UNAVAILABLE_KEPT.1,
            ),
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

    pub(super) fn verification_summary(
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

    pub(super) fn blocker_summary(blockers: &[MirrorBlocker]) -> Option<BlockerCopy> {
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

    pub(super) fn warning_summary(warnings: &[SyncPageWarning]) -> Vec<Message> {
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

    pub(super) fn action_copy(controls: SyncPageControls, action: PrimaryAction) -> PageActionCopy {
        if controls.can_cancel {
            PageActionCopy {
                label: CANCEL,
                sensitive: true,
                destructive: true,
            }
        } else {
            PageActionCopy {
                label: match action {
                    PrimaryAction::DownloadAndSync => DOWNLOAD_AND_SYNC,
                    PrimaryAction::SyncNow => SYNC_NOW,
                },
                sensitive: controls.can_start,
                destructive: false,
            }
        }
    }

    pub(super) fn eject_sensitive(
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

    pub(super) fn counted(count: usize, singular: &'static str, plural: &'static str) -> Message {
        Message {
            id: singular,
            plural: Some(Plural {
                id: plural,
                count: u64::try_from(count).unwrap_or(u64::MAX),
            }),
            args: vec![("count", count.to_string())],
        }
    }

    pub(super) fn progress_copy(
        preparation: Option<&PreparationProgress>,
        phase: &PlannedSyncPhase,
        bytes_per_second: u64,
        prepared_this_run: bool,
    ) -> Option<ProgressCopy> {
        if let Some(preparation) = preparation {
            let fraction = preparation
                .total_bytes
                .filter(|total_bytes| *total_bytes > 0)
                .map_or(0.0, |total_bytes| {
                    (preparation.received_bytes as f64 / total_bytes as f64).clamp(0.0, 1.0)
                });
            let percent = (fraction * 100.0).round() as u64;
            return Some(ProgressCopy {
                title: message_with_args(
                    PREPARING_TITLE,
                    vec![
                        ("current", preparation.done.saturating_add(1).to_string()),
                        ("total", preparation.total.to_string()),
                        ("percent", percent.to_string()),
                    ],
                ),
                subtitle: ProgressSubtitle::CurrentTrack(preparation.title.clone()),
                speed: ProgressSpeed::Unavailable,
                fraction,
            });
        }
        transfer_progress(phase, bytes_per_second, prepared_this_run)
    }

    pub(super) fn transfer_progress_copy(
        phase: &PlannedSyncPhase,
        bytes_per_second: u64,
    ) -> Option<ProgressCopy> {
        transfer_progress(phase, bytes_per_second, false)
    }

    fn transfer_progress(
        phase: &PlannedSyncPhase,
        bytes_per_second: u64,
        step_two: bool,
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
                bytes_done,
                bytes_total,
            } => {
                let title = match (step_two, step) {
                    (false, SyncStep::Removing) => REMOVING_TITLE,
                    (false, SyncStep::Transcoding) => CONVERTING_TITLE,
                    (false, SyncStep::Copying) => COPYING_TITLE,
                    (false, SyncStep::WritingPlaylists) => WRITING_PLAYLISTS_TITLE,
                    (true, SyncStep::Removing) => REMOVING_TITLE_STEP_TWO,
                    (true, SyncStep::Transcoding) => CONVERTING_TITLE_STEP_TWO,
                    (true, SyncStep::Copying) => COPYING_TITLE_STEP_TWO,
                    (true, SyncStep::WritingPlaylists) => WRITING_PLAYLISTS_TITLE_STEP_TWO,
                };
                let fraction = if *bytes_total > 0 {
                    *bytes_done as f64 / *bytes_total as f64
                } else if *total > 0 {
                    f64::from(*done) / f64::from(*total)
                } else {
                    0.0
                };
                let speed = if step == &SyncStep::Copying && bytes_per_second > 0 {
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
}

pub(super) use projection::PageActionCopy;

fn borrowed<'a>(args: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    args.iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect()
}

fn render(message: &Message) -> String {
    let template = match &message.plural {
        Some(plural) => crate::i18n::ngettext(
            message.id,
            plural.id,
            u32::try_from(plural.count).unwrap_or(u32::MAX),
        ),
        None => crate::i18n::gettext(message.id),
    };
    crate::i18n::format_message(&template, &borrowed(&message.args))
}

fn render_joined(messages: &[Message]) -> String {
    messages.iter().map(render).collect::<Vec<_>>().join(" · ")
}

pub(super) fn profile_label(profile: TransferProfile) -> &'static str {
    projection::profile_label(profile)
}

pub(super) fn playlist_subtitle(row: &SyncPlaylistRow) -> String {
    let last_sync = match row.last_synced_at {
        None => projection::VerifiedSyncTime::Never,
        Some(last_synced_at) => chrono::Local
            .timestamp_opt(last_synced_at, 0)
            .single()
            .map_or(projection::VerifiedSyncTime::Unavailable, |timestamp| {
                projection::VerifiedSyncTime::Formatted(
                    timestamp.format("%b %-d, %Y at %H:%M").to_string(),
                )
            }),
    };
    render_joined(&projection::playlist_subtitle(row, last_sync))
}

pub(super) fn device_last_sync_copy(device: &DeviceView) -> String {
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return verification_summary(device);
    }
    let last_sync = device.last_sync.map(|timestamp| {
        timestamp
            .with_timezone(&chrono::Local)
            .format("%b %-d, %Y at %H:%M")
            .to_string()
    });
    render_joined(&projection::device_last_sync_copy(
        &device.sync_phase,
        last_sync,
        &device.session_state,
        device.size_on_device_bytes,
        device.verified_managed_track_count,
    ))
}

pub(super) fn change_summary(changes: &SyncChangeSummary) -> String {
    render_joined(&projection::change_summary(changes))
}

pub(super) fn verification_summary(device: &DeviceView) -> String {
    render(&projection::verification_summary(
        &device.sync_phase,
        device.last_sync.is_some(),
        device.verified_managed_track_count,
    ))
}

pub(super) fn blocker_summary(blockers: &[MirrorBlocker]) -> Option<String> {
    match projection::blocker_summary(blockers)? {
        projection::BlockerCopy::Standalone(message) => Some(render(&message)),
        projection::BlockerCopy::Reasons(reasons) => {
            Some(format!("Cannot synchronize: {}.", render_joined(&reasons)))
        }
    }
}

pub(super) fn warning_summary(warnings: &[SyncPageWarning]) -> Vec<String> {
    projection::warning_summary(warnings)
        .iter()
        .map(render)
        .collect()
}

pub(super) fn action_copy(controls: SyncPageControls, action: PrimaryAction) -> PageActionCopy {
    projection::action_copy(controls, action)
}

pub(super) fn eject_sensitive(device: &DeviceView) -> bool {
    projection::eject_sensitive(device.page.controls, device.connected, &device.sync_phase)
}

pub(super) fn counted(count: usize, singular: &'static str, plural: &'static str) -> String {
    render(&projection::counted(count, singular, plural))
}

pub(super) fn progress_copy(device: &DeviceView) -> Option<(String, String, String, f64)> {
    let preparation = match &device.preparation_run {
        PreparationRunState::Idle => None,
        PreparationRunState::Downloading {
            done,
            total,
            title,
            received_bytes,
            total_bytes,
        } => Some(projection::PreparationProgress {
            done: *done,
            total: *total,
            title: title.clone(),
            received_bytes: *received_bytes,
            total_bytes: *total_bytes,
        }),
    };
    if preparation.is_none() && !device.prepared_this_run {
        return transfer_progress_copy(&device.sync_phase, device.bytes_per_second);
    }
    projection::progress_copy(
        preparation.as_ref(),
        &device.sync_phase,
        device.bytes_per_second,
        device.prepared_this_run,
    )
    .map(render_progress)
}

pub(super) fn transfer_progress_copy(
    phase: &PlannedSyncPhase,
    bytes_per_second: u64,
) -> Option<(String, String, String, f64)> {
    projection::transfer_progress_copy(phase, bytes_per_second).map(render_progress)
}

fn render_progress(copy: projection::ProgressCopy) -> (String, String, String, f64) {
    let subtitle = match copy.subtitle {
        projection::ProgressSubtitle::Message(message) => render(&message),
        projection::ProgressSubtitle::CurrentTrack(title) => title,
    };
    let speed = match copy.speed {
        projection::ProgressSpeed::Unavailable => "—".into(),
        projection::ProgressSpeed::BytesPerSecond(bytes) => {
            format!("{}/s", device_sync_strings::file_size(bytes))
        }
    };
    (render(&copy.title), subtitle, speed, copy.fraction)
}

#[cfg(test)]
mod projection_tests {
    use reprise_view::strings::{Message, Plural};

    use super::projection;

    #[test]
    fn counted_copy_crosses_the_projection_seam_as_a_plural_message() {
        assert_eq!(
            projection::counted(2, "{count} entry", "{count} entries"),
            Message {
                id: "{count} entry",
                plural: Some(Plural {
                    id: "{count} entries",
                    count: 2,
                }),
                args: vec![("count", "2".into())],
            }
        );
    }
}
