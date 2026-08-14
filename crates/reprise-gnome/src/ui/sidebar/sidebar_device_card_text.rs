//! Pure sidebar device-card text projections (design 7c, `MTP-29`).
//!
//! The card used to blend every kind of pending work into one "N changes"
//! count next to a single bytes figure that only ever counted bytes moving
//! *onto* the device (`page.changes.transfer_bytes`). A deletions-only sync
//! — three files leaving, nothing arriving — read "3 changes · 0 B", which
//! design 7c calls out by name: "0 B sounds like nothing to do even though
//! three files are being deleted". `MTP-22`'s [`SyncBalance`] already fixed
//! the underlying data (copy and remove keep separate file *and* byte
//! counts); this module is the last mile — turning that balance into the
//! card's four exact leading sentences, kept as plain functions so the
//! wording is unit-tested independently of any widget callback.

use chrono::{DateTime, Utc};
use reprise_core::device_sync::device_view::DeviceContentsState;
use reprise_core::device_sync::{DeviceSessionState, SyncBalance};

use crate::ui::device_sync_runtime::{DeviceView, PlannedSyncPhase, SyncStep};
use crate::ui::device_sync_strings;

/// `MTP-63`: which contrast step a device card carries. The step decides
/// ground, edge, and how far the status line falls off against the name —
/// without ever applying blanket opacity to the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CardEmphasis {
    /// A sync of this device is running: accent edge, tinted ground.
    Active,
    /// Connected and idle: solid ground, neutral edge.
    Connected,
    /// Remembered but not connected: no ground, contrast-bearing hairline edge.
    Remembered,
}

#[must_use]
pub(super) fn card_emphasis(device: &DeviceView) -> CardEmphasis {
    if device.session_state == DeviceSessionState::Remembered {
        CardEmphasis::Remembered
    } else if is_syncing(device) {
        CardEmphasis::Active
    } else {
        CardEmphasis::Connected
    }
}

#[must_use]
pub(super) fn is_syncing(device: &DeviceView) -> bool {
    device.connected
        && matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        )
}

/// `MTP-63`: the syncing card's status line — one run-wide file ordinal, not
/// the current machine step's zero-based index. The percentage keeps its own
/// fixed-width slot so the count cannot shove it.
#[must_use]
pub(super) fn syncing_file_count(device: &DeviceView) -> Option<String> {
    let changes = &device.page.changes;
    let transfers = changes.additions.saturating_add(changes.replacements);
    let playlist_files = changes
        .playlist_writes
        .saturating_add(changes.playlist_removals);
    let planned_total = transfers
        .saturating_add(playlist_files)
        .saturating_add(changes.removals);
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return (planned_total > 0)
            .then(|| format_syncing_file_count(planned_total, planned_total));
    }
    let PlannedSyncPhase::Syncing {
        step, done, total, ..
    } = &device.sync_phase
    else {
        return None;
    };
    let local_total = *total as usize;
    if local_total == 0 {
        return None;
    }

    let offset = match step {
        SyncStep::Transcoding | SyncStep::Copying => 0,
        SyncStep::WritingPlaylists => transfers,
        SyncStep::Removing => transfers.saturating_add(playlist_files),
    };
    let run_total = planned_total.max(offset.saturating_add(local_total));
    let current = offset
        .saturating_add((*done as usize).min(local_total.saturating_sub(1)))
        .saturating_add(1)
        .min(run_total);
    Some(format_syncing_file_count(current, run_total))
}

fn format_syncing_file_count(current: usize, total: usize) -> String {
    let current = current.to_string();
    let total = total.to_string();
    device_sync_strings::formatted(
        device_sync_strings::SYNCING_FILE_COUNT,
        &[("completed", &current), ("total", &total)],
    )
}

pub(super) fn css() -> String {
    ".device-card { min-height: 0; padding: 0; border-radius: 14px; border: 1px solid transparent; background-color: transparent; }\n\
     .device-card-remembered { background-color: transparent; border-color: alpha(@window_fg_color, 0.55); }\n\
     .device-card-connected { background-color: alpha(@window_fg_color, 0.07); border-color: alpha(@window_fg_color, 0.65); }\n\
     .device-card-active { background-color: alpha(@accent_color, 0.10); border-color: @reprise_accent_text_color; }\n\
     .device-card-current.device-card-remembered { background-color: alpha(@window_fg_color, 0.13); }\n\
     .device-card-current.device-card-connected { background-color: alpha(@window_fg_color, 0.16); }\n\
     .device-card-current.device-card-active { background-color: alpha(@accent_color, 0.20); }\n\
     .device-card-remembered:hover { background-color: alpha(@window_fg_color, 0.04); border-color: alpha(@window_fg_color, 0.62); }\n\
     .device-card-connected:hover { background-color: alpha(@window_fg_color, 0.10); border-color: alpha(@window_fg_color, 0.72); }\n\
     .device-card-active:hover { background-color: alpha(@accent_color, 0.16); border-color: @reprise_accent_text_color; }\n\
     .device-card-current.device-card-remembered:hover { background-color: alpha(@window_fg_color, 0.17); }\n\
     .device-card-current.device-card-connected:hover { background-color: alpha(@window_fg_color, 0.20); }\n\
     .device-card-current.device-card-active:hover { background-color: alpha(@accent_color, 0.26); }\n\
     .device-card:focus-visible { box-shadow: inset 0 0 0 2px alpha(@window_fg_color, 0.32); }\n\
     .device-card-icon { border-radius: 13px; background-color: transparent; }\n\
     .device-card-remembered .device-card-icon { background-color: alpha(@window_fg_color, 0.035); }\n\
     .device-card-connected .device-card-icon { background-color: alpha(@window_fg_color, 0.075); }\n\
     .device-card-active .device-card-icon { background-color: alpha(@accent_color, 0.02); }\n\
     .device-card-glyph { color: alpha(@window_fg_color, 0.82); }\n\
     .device-card-title { font-size: 13.5px; }\n\
     .device-card-remembered .device-card-title { color: alpha(@window_fg_color, 0.62); }\n\
     .device-card-detail { font-size: 11.5px; color: @reprise_secondary_fg_color; }\n\
     .device-card-active .device-card-detail { color: @reprise_accent_text_color; font-feature-settings: \"tnum\"; }\n\
     .device-card-active .device-card-glyph { color: @reprise_accent_text_color; }\n\
     .device-card-percent { font-size: 11.5px; font-feature-settings: \"tnum\"; color: alpha(@window_fg_color, 0.45); }\n\
     .device-card-cancel { padding: 0; color: @reprise_accent_text_color; background-color: alpha(@accent_color, 0.04); }\n\
     .device-card-cancel:hover { background-color: alpha(@accent_color, 0.08); }\n\
     .device-card-progress { min-height: 3px; }\n\
     .device-card-progress trough { min-height: 3px; border-radius: 2px; background-color: alpha(@window_fg_color, 0.12); }\n\
     .device-card-progress progress { min-height: 3px; border-radius: 2px; background-color: @accent_color; }\n\
     .device-section-heading { min-height: 0; padding: 0 8px; color: alpha(@window_fg_color, 0.72); background: none; box-shadow: none; border: none; }\n\
     .device-section-heading:hover { background-color: alpha(@window_fg_color, 0.05); }\n\
     .device-section-heading:disabled { background: none; filter: none; }"
        .to_string()
}

/// `MTP-29`: the card's leading sentence — exactly one of design 7c's four
/// states:
/// - "14 to copy · 2.6 GiB · 3 to remove"
/// - "3 to remove · frees 148 MiB" (0 B moved is correct here, and must
///   read as work, not as idle)
/// - "Up to date · synced 12 min ago"
/// - "Tap to scan device contents"
#[must_use]
pub(super) fn leading_sentence(
    contents: &DeviceContentsState,
    balance: &SyncBalance,
    last_sync: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if !matches!(contents, DeviceContentsState::Verified) {
        return "Tap to scan device contents".to_string();
    }
    if balance.has_work() {
        return balance_sentence(balance);
    }
    last_sync.map_or_else(
        || "Up to date".to_string(),
        |at| {
            format!(
                "Up to date · synced {}",
                device_sync_strings::relative_time(now, at)
            )
        },
    )
}

/// `MTP-29`: the card's tooltip — the full balance, in the same vocabulary
/// as the Next synchronization panel (`MTP-22`), never abbreviated the way
/// the leading sentence is.
#[must_use]
pub(super) fn tooltip_text(balance: &SyncBalance) -> String {
    device_sync_strings::detailed_balance_text(balance)
}

#[must_use]
pub(super) fn remembered_sentence(
    last_verified: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    reprise_core::device_sync::remembered_device_status(last_verified, now)
}

/// The narrow card omits unknown storage instead of spending its leading,
/// non-ellipsized words on a placeholder. Once known, the figure remains the
/// prefix so the activity that follows stays consistent across phases.
#[must_use]
pub(super) fn card_subtitle(device: &DeviceView, now: DateTime<Utc>) -> String {
    if device.session_state == DeviceSessionState::Remembered {
        return remembered_sentence(device.last_sync, now);
    }
    if let DeviceSessionState::Inert { active_device_name } = &device.session_state {
        return device_sync_strings::inert_device_status(active_device_name);
    }
    if let Some(status) = &device.memory_status {
        return status.clone();
    }
    match &device.sync_phase {
        PlannedSyncPhase::ComputingDelta => with_storage_prefix(
            known_free_space(device.storage.free_bytes),
            device_sync_strings::text(device_sync_strings::CARD_CHECKING_CHANGES),
        ),
        PlannedSyncPhase::Syncing {
            step,
            current_track,
            ..
        } => {
            let mut activity = device_sync_strings::sync_activity(step_glyph(step), current_track);
            if matches!(step, SyncStep::Copying) && device.bytes_per_second > 0 {
                activity.push_str(&format!(
                    " · {}/s",
                    device_sync_strings::file_size(device.bytes_per_second)
                ));
            }
            with_storage_prefix(known_free_space(device.storage.free_bytes), activity)
        }
        PlannedSyncPhase::Finishing => with_storage_prefix(
            known_free_space(device.storage.free_bytes),
            "Finishing…".to_string(),
        ),
        PlannedSyncPhase::Idle => {
            if mirror_needs_attention(device) {
                return with_storage_prefix(
                    known_available_space(device.storage.free_bytes),
                    "Needs attention".to_string(),
                );
            }
            let balance = reprise_core::device_sync::aggregate_balance(&[device.target_reading]);
            leading_sentence(&device.contents_state, &balance, device.last_sync, now)
        }
    }
}

#[must_use]
pub(super) fn mirror_needs_attention(device: &DeviceView) -> bool {
    device
        .page
        .blockers
        .iter()
        .any(|blocker| blocker != &reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected)
        || !device.page.warnings.is_empty()
        || device.scan_error.is_some()
        || device.sync_error.is_some()
}

fn known_free_space(bytes: Option<u64>) -> Option<String> {
    bytes.map(|bytes| device_sync_strings::free_space(Some(bytes)))
}

fn known_available_space(bytes: Option<u64>) -> Option<String> {
    bytes.map(|bytes| device_sync_strings::available_space(Some(bytes)))
}

fn with_storage_prefix(prefix: Option<String>, activity: String) -> String {
    match prefix {
        Some(prefix) => format!("{prefix} · {activity}"),
        None => activity,
    }
}

pub(super) fn step_glyph(step: &SyncStep) -> &'static str {
    match step {
        SyncStep::Transcoding => "⟳ transcoding ·",
        SyncStep::Copying => "↑",
        SyncStep::Removing => "−",
        SyncStep::WritingPlaylists => "≡",
    }
}

/// Design 7c's two "has work" states, kept distinct from the aggregate
/// balance formatter used for the tooltip (`MTP-22`'s "To copy N files ·
/// X" wording) because the card's leading sentence is deliberately terser:
/// no "To copy"/"To remove" verbs, and copy-and-remove-together omits the
/// remove byte figure entirely (the design's own example, "14 to copy ·
/// 2.6 GiB · 3 to remove", never states how many bytes 3 files free).
fn balance_sentence(balance: &SyncBalance) -> String {
    match (balance.files_to_copy, balance.files_to_remove) {
        (0, 0) => waiting_or_playlists_sentence(balance),
        (copy, 0) => format!(
            "{copy} to copy · {}",
            device_sync_strings::file_size(balance.bytes_to_copy)
        ),
        (0, remove) => format!(
            "{remove} to remove · frees {}",
            device_sync_strings::file_size(balance.bytes_freed)
        ),
        (copy, remove) => format!(
            "{copy} to copy · {} · {remove} to remove",
            device_sync_strings::file_size(balance.bytes_to_copy)
        ),
    }
}

/// A balance can carry work without a file to copy or remove when playlist
/// manifests will be rewritten.
fn waiting_or_playlists_sentence(balance: &SyncBalance) -> String {
    let _ = balance;
    "Playlists updating".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::device_sync_runtime::PlannedSyncPhase;
    use crate::ui::sidebar::sidebar_device_card::tests::view;
    use chrono::TimeZone;

    fn balance(
        files_to_copy: usize,
        bytes_to_copy: u64,
        files_to_remove: usize,
        bytes_freed: u64,
    ) -> SyncBalance {
        SyncBalance {
            files_to_copy,
            bytes_to_copy,
            files_to_remove,
            bytes_freed,
            playlists_rewritten: 0,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 28, 12, 0, 0).unwrap()
    }

    #[test]
    fn mtp_29_copy_and_remove_together_matches_the_designs_first_example() {
        let balance = balance(14, 2_600_000_000, 3, 0);
        assert_eq!(
            leading_sentence(&DeviceContentsState::Verified, &balance, None, now()),
            "14 to copy · 2.4 GiB · 3 to remove"
        );
    }

    #[test]
    fn mtp_29_deletions_only_reads_frees_not_zero_bytes_and_is_not_idle() {
        let balance = balance(0, 0, 3, 148 * 1024 * 1024);

        let sentence = leading_sentence(&DeviceContentsState::Verified, &balance, None, now());

        assert_eq!(sentence, "3 to remove · frees 148.0 MiB");
        assert!(
            !sentence.contains("0 B"),
            "zero bytes copied must never read as nothing to do while files are removed"
        );
    }

    #[test]
    fn mtp_29_up_to_date_names_how_long_ago_it_last_synced() {
        let idle = SyncBalance::default();
        let last_sync = now() - chrono::Duration::minutes(12);

        assert_eq!(
            leading_sentence(
                &DeviceContentsState::Verified,
                &idle,
                Some(last_sync),
                now()
            ),
            "Up to date · synced 12 min ago"
        );
        assert_eq!(
            leading_sentence(&DeviceContentsState::Verified, &idle, None, now()),
            "Up to date",
            "a device that has never synced still reads as up to date once verified and idle"
        );
    }

    #[test]
    fn mtp_29_unverified_contents_always_prompt_a_scan_regardless_of_balance() {
        let would_have_work = balance(14, 1, 3, 1);

        assert_eq!(
            leading_sentence(
                &DeviceContentsState::NeverVerified,
                &would_have_work,
                None,
                now()
            ),
            "Tap to scan device contents"
        );
        assert_eq!(
            leading_sentence(
                &DeviceContentsState::Failed("timeout".into()),
                &SyncBalance::default(),
                None,
                now()
            ),
            "Tap to scan device contents"
        );
    }

    #[test]
    fn mtp_29_tooltip_carries_the_full_balance_the_leading_sentence_omits() {
        let balance = balance(14, 2 * 1024 * 1024 * 1024, 3, 100 * 1024 * 1024);
        assert_eq!(
            tooltip_text(&balance),
            "To copy 14 files · 2.0 GiB · To remove 3 files · 100.0 MiB"
        );
    }

    #[test]
    fn mtp_50_remembered_devices_name_only_connection_history_and_never_a_diff() {
        let last_verified = now() - chrono::Duration::days(3);
        assert_eq!(
            remembered_sentence(Some(last_verified), now()),
            "Not connected · synced 3 days ago"
        );
        assert_eq!(
            remembered_sentence(None, now()),
            "Not connected · never verified"
        );
        let copy_balance = balance(14, 2_600_000_000, 3, 148 * 1024 * 1024);
        assert!(
            !remembered_sentence(Some(last_verified), now()).contains(&tooltip_text(&copy_balance)),
            "an absent card must never present a stale copy/remove balance"
        );
    }

    #[test]
    fn mtp_63_the_card_emphasis_separates_active_connected_and_remembered() {
        let active = view(PlannedSyncPhase::Finishing);
        let connected = view(PlannedSyncPhase::Idle);
        let mut remembered = view(PlannedSyncPhase::Idle);
        remembered.connected = false;
        remembered.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;

        assert_eq!(card_emphasis(&active), CardEmphasis::Active);
        assert_eq!(card_emphasis(&connected), CardEmphasis::Connected);
        assert_eq!(card_emphasis(&remembered), CardEmphasis::Remembered);
    }

    #[test]
    fn mtp_63_the_card_states_monotonic_run_progress_across_sync_phases() {
        let mut device = view(PlannedSyncPhase::Syncing {
            step: crate::ui::device_sync_runtime::SyncStep::Copying,
            done: 1_046,
            total: 1_047,
            current_track: "Last transfer".into(),
            bytes_done: 1,
            bytes_total: 1,
        });
        device.page.changes.additions = 1_047;
        device.page.changes.playlist_writes = 3;
        device.page.changes.removals = 12;

        assert_eq!(
            syncing_file_count(&device).as_deref(),
            Some("Syncing · 1047 / 1062")
        );

        device.sync_phase = PlannedSyncPhase::Syncing {
            step: crate::ui::device_sync_runtime::SyncStep::WritingPlaylists,
            done: 0,
            total: 3,
            current_track: "Road".into(),
            bytes_done: 1,
            bytes_total: 1,
        };
        assert_eq!(
            syncing_file_count(&device).as_deref(),
            Some("Syncing · 1048 / 1062")
        );

        device.sync_phase = PlannedSyncPhase::Syncing {
            step: crate::ui::device_sync_runtime::SyncStep::Removing,
            done: 11,
            total: 12,
            current_track: "old.mp3".into(),
            bytes_done: 1,
            bytes_total: 1,
        };
        assert_eq!(
            syncing_file_count(&device).as_deref(),
            Some("Syncing · 1062 / 1062")
        );

        device.sync_phase = PlannedSyncPhase::Finishing;
        assert_eq!(
            syncing_file_count(&device).as_deref(),
            Some("Syncing · 1062 / 1062")
        );
    }

    #[test]
    fn syncing_file_count_clamps_inconsistent_progress_and_never_states_zero_of_zero() {
        let mut device = view(PlannedSyncPhase::Syncing {
            step: crate::ui::device_sync_runtime::SyncStep::Removing,
            done: 9,
            total: 3,
            current_track: "old.mp3".into(),
            bytes_done: 0,
            bytes_total: 0,
        });

        assert_eq!(
            syncing_file_count(&device).as_deref(),
            Some("Syncing · 3 / 3")
        );

        device.sync_phase = PlannedSyncPhase::Syncing {
            step: crate::ui::device_sync_runtime::SyncStep::Removing,
            done: 0,
            total: 0,
            current_track: String::new(),
            bytes_done: 0,
            bytes_total: 0,
        };
        assert_eq!(syncing_file_count(&device), None);
    }

    #[test]
    fn disconnected_syncing_state_keeps_remembered_presentation() {
        let mut remembered = view(PlannedSyncPhase::Syncing {
            step: crate::ui::device_sync_runtime::SyncStep::Copying,
            done: 0,
            total: 1,
            current_track: "Track".into(),
            bytes_done: 0,
            bytes_total: 1,
        });
        remembered.connected = false;
        remembered.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;

        assert!(!is_syncing(&remembered));
        assert_eq!(card_emphasis(&remembered), CardEmphasis::Remembered);
    }
}
