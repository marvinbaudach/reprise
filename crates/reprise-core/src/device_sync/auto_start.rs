//! `MTP-30` (design 7a, turn-6 plan E5): whether a device connect should
//! start a synchronization by itself, with no button pressed.
//!
//! The switch ("Sync automatically when this phone connects",
//! `DeviceSettings::sync_automatically`) has existed since schema v44, but
//! nothing ever read it to decide anything — it only round-tripped through
//! the database and rendered into a `gtk4::Switch`. This module is the
//! decision that closes that gap. It is deliberately a pure function over a
//! flat fact struct: the GTK runtime (`device_sync_runtime.rs`'s
//! `refresh_contents_with_delta`) gathers the facts from live device state
//! inside one `borrow_mut` block, drops the borrow, and only then acts on
//! the answer — the same discipline `resume_planned` already uses to avoid
//! re-entering GTK while holding a `RefCell` borrow.

use super::category_diff::SyncBalance;

/// Every fact [`should_auto_start`] needs. Flat and `Copy` on purpose: the
/// caller builds one of these from a single, short-lived borrow of device
/// state and can then drop that borrow before calling `sync_now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoStartFacts {
    /// This refresh is the first one after the device connected — a
    /// brand-new device or a reconnect — not every subsequent refresh, and
    /// never the user's manual "Refresh" action or the post-sync verify
    /// refresh.
    pub just_connected: bool,
    /// `DeviceSettings::sync_automatically` — the switch itself.
    pub sync_automatically: bool,
    /// No `scan_error`: the connect's on-device inventory scan actually
    /// succeeded, so the plan below is trustworthy.
    pub scan_ok: bool,
    /// The sync plan (`recompute_delta_silent`) computed without error.
    pub planning_ok: bool,
    /// The device is still connected by the time this decision runs — the
    /// scan/plan round trip is async and a disconnect can race it.
    pub device_connected: bool,
    /// The device already has a sync active or finishing.
    pub device_busy: bool,
    /// The projected balance across all three sync targets, reused as-is
    /// from `category_diff::aggregate_balance` — never re-derived here.
    pub balance: SyncBalance,
}

/// `MTP-30`: every condition is required. See `docs/ux-rules.md`'s `MTP-30`
/// for the user-facing contract this enforces.
#[must_use]
pub fn should_auto_start(facts: AutoStartFacts) -> bool {
    facts.just_connected
        && facts.sync_automatically
        && facts.scan_ok
        && facts.planning_ok
        && facts.device_connected
        && !facts.device_busy
        && facts.balance.has_work()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> SyncBalance {
        SyncBalance {
            files_to_copy: 2,
            bytes_to_copy: 1_000,
            files_to_remove: 0,
            bytes_freed: 0,
            files_waiting_for_download: 0,
            playlists_rewritten: 0,
        }
    }

    fn ready_facts() -> AutoStartFacts {
        AutoStartFacts {
            just_connected: true,
            sync_automatically: true,
            scan_ok: true,
            planning_ok: true,
            device_connected: true,
            device_busy: false,
            balance: work(),
        }
    }

    #[test]
    fn mtp_30_starts_when_every_condition_holds() {
        assert!(should_auto_start(ready_facts()));
    }

    #[test]
    fn mtp_30_never_starts_outside_the_connect_refresh() {
        assert!(!should_auto_start(AutoStartFacts {
            just_connected: false,
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_never_starts_when_the_switch_is_off() {
        assert!(!should_auto_start(AutoStartFacts {
            sync_automatically: false,
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_never_starts_on_an_unverified_scan() {
        assert!(!should_auto_start(AutoStartFacts {
            scan_ok: false,
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_never_starts_when_planning_failed() {
        assert!(!should_auto_start(AutoStartFacts {
            planning_ok: false,
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_never_starts_on_a_device_that_already_disconnected_again() {
        assert!(!should_auto_start(AutoStartFacts {
            device_connected: false,
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_never_starts_on_a_device_that_is_already_busy() {
        assert!(!should_auto_start(AutoStartFacts {
            device_busy: true,
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_never_starts_when_there_is_nothing_to_do() {
        assert!(!should_auto_start(AutoStartFacts {
            balance: SyncBalance::default(),
            ..ready_facts()
        }));
    }

    #[test]
    fn mtp_30_a_deletions_only_balance_still_counts_as_work() {
        let deletions_only = SyncBalance {
            files_to_copy: 0,
            bytes_to_copy: 0,
            files_to_remove: 3,
            bytes_freed: 0,
            files_waiting_for_download: 0,
            playlists_rewritten: 0,
        };
        assert!(should_auto_start(AutoStartFacts {
            balance: deletions_only,
            ..ready_facts()
        }));
    }
}
