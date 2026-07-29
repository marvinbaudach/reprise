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
use reprise_core::device_sync::SyncBalance;

use crate::ui::device_sync_strings;

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
    device_sync_strings::balance_text(balance)
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

/// A balance can carry work without a single file to copy or remove yet —
/// only playlists rewritten, or episodes waiting on a download (`MTP-40`).
/// Neither is one of the design's four named states, so this stays honest
/// rather than inventing a fifth exact phrase for it.
fn waiting_or_playlists_sentence(balance: &SyncBalance) -> String {
    if balance.files_waiting_for_download > 0 {
        return format!(
            "{} waiting for download",
            balance.files_waiting_for_download
        );
    }
    "Playlists updating".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
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
            files_waiting_for_download: 0,
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
}
