//! Design 7f's preparation phase: pure planning for the download step that
//! can run before a device sync's transfer step (E9, `MTP-42`).
//!
//! A user can mark an episode "wanted on device" (`MTP-40`) before it has a
//! local file. Nothing here downloads anything — this module only decides,
//! from already-known facts, whether that preparation phase exists at all
//! and, if so, what it is about. The actual download runner is a later
//! commit; this is its pure precondition and byte/file-count projection.

use crate::connectivity::Connectivity;

/// One episode that is `wanted_on_device` (`MTP-40`) but has no local file
/// yet, and would therefore need to be downloaded before it can be copied to
/// the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingFile {
    pub episode_id: i64,
    pub title: String,
    pub size_bytes: u64,
}

/// Everything `plan_preparation` needs, already resolved by its caller. This
/// module makes no I/O calls and reads no settings itself — it only decides
/// given these facts, so every input that changes the outcome must be named
/// here explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparationFacts {
    /// Episodes wanted on the device with no local file yet.
    pub missing: Vec<MissingFile>,
    /// Whether the app currently believes a network path exists (`NET-3a`).
    pub connectivity: Connectivity,
    /// Whether the current network connection is reported metered.
    pub metered: bool,
    /// The global `online-sources-enabled` gate (`NET-1a`).
    pub online_sources_enabled: bool,
    /// The device's own "prepare before sync" switch.
    pub prepare_switch_on: bool,
}

/// What the preparation phase is, given [`PreparationFacts`]. See
/// [`plan_preparation`] for the precedence between these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreparationPhase {
    /// `NET-1a`: online sources are switched off. The phase does not exist —
    /// not an empty phase, not a disabled switch, nothing shown.
    Absent,
    /// Nothing is missing; every wanted episode already has a local file.
    NothingMissing,
    /// Offline (`NET-3`/`MTP-40`): the sync still runs and skips these
    /// files, which stay marked `wanted_on_device` for the next attempt.
    SkippedOffline { files: usize },
    /// Offered to the user but not started, because the connection is
    /// metered or the device's own switch is off.
    Offered { files: usize, bytes: u64 },
    /// Will run as part of this sync.
    Planned { files: usize, bytes: u64 },
}

/// Decides the preparation phase from `facts`, in this precedence order:
///
/// 1. `online_sources_enabled == false` -> [`PreparationPhase::Absent`].
/// 2. no missing files -> [`PreparationPhase::NothingMissing`].
/// 3. offline -> [`PreparationPhase::SkippedOffline`].
/// 4. metered, or the device's prepare switch is off ->
///    [`PreparationPhase::Offered`].
/// 5. otherwise -> [`PreparationPhase::Planned`].
///
/// Offline is checked before metered/switch-off deliberately, and that order
/// is not incidental: offline is a fact about whether the download *can*
/// run at all, while metered and the prepare switch are policy about
/// whether it *should* run given that it could. Offering a download to the
/// user that cannot run either way — because there is no network path for
/// it to use — would be a lie dressed up as a choice. So a missing
/// connection always wins over any policy state, and the two are reported
/// as different phases even though both end up not downloading right now:
/// `SkippedOffline` means "try again once you have a connection",
/// `Offered` means "you decide, the connection is fine".
#[must_use]
pub fn plan_preparation(facts: &PreparationFacts) -> PreparationPhase {
    if !facts.online_sources_enabled {
        return PreparationPhase::Absent;
    }
    if facts.missing.is_empty() {
        return PreparationPhase::NothingMissing;
    }
    if facts.connectivity.is_offline() {
        return PreparationPhase::SkippedOffline {
            files: facts.missing.len(),
        };
    }
    let files = facts.missing.len();
    let bytes = facts.missing.iter().map(|file| file.size_bytes).sum();
    if facts.metered || !facts.prepare_switch_on {
        return PreparationPhase::Offered { files, bytes };
    }
    PreparationPhase::Planned { files, bytes }
}

/// The primary button's action, projected from the preparation phase. This
/// only names the action, never its wording — labels live in the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryAction {
    SyncNow,
    DownloadAndSync,
}

/// `Planned` is the only phase that starts a download alongside the sync;
/// every other phase — including `Offered`, which still requires an
/// explicit separate choice — leaves the primary button as a plain sync.
#[must_use]
pub fn primary_action(phase: &PreparationPhase) -> PrimaryAction {
    match phase {
        PreparationPhase::Planned { .. } => PrimaryAction::DownloadAndSync,
        _ => PrimaryAction::SyncNow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing(episode_id: i64, size_bytes: u64) -> MissingFile {
        MissingFile {
            episode_id,
            title: format!("Episode {episode_id}"),
            size_bytes,
        }
    }

    fn base_facts() -> PreparationFacts {
        PreparationFacts {
            missing: vec![missing(1, 1_000), missing(2, 2_000)],
            connectivity: Connectivity::Online,
            metered: false,
            online_sources_enabled: true,
            prepare_switch_on: true,
        }
    }

    #[test]
    fn mtp_42_online_sources_disabled_wins_over_every_other_condition() {
        // Missing files present, online, unmetered, switch on — every other
        // condition points at `Planned`, yet the gate must still win.
        let facts = PreparationFacts {
            online_sources_enabled: false,
            ..base_facts()
        };
        assert_eq!(plan_preparation(&facts), PreparationPhase::Absent);
    }

    #[test]
    fn mtp_42_disabled_gate_wins_even_while_offline_and_switch_off() {
        // Stack every other reason to skip on top of the gate too, so a
        // future refactor cannot make `Absent` fall out "by accident" of
        // some other rule instead of the gate itself.
        let facts = PreparationFacts {
            online_sources_enabled: false,
            connectivity: Connectivity::Offline,
            metered: true,
            prepare_switch_on: false,
            ..base_facts()
        };
        assert_eq!(plan_preparation(&facts), PreparationPhase::Absent);
    }

    #[test]
    fn mtp_42_no_missing_files_reports_nothing_missing() {
        let facts = PreparationFacts {
            missing: vec![],
            ..base_facts()
        };
        assert_eq!(plan_preparation(&facts), PreparationPhase::NothingMissing);
    }

    #[test]
    fn mtp_42_no_missing_files_wins_even_offline_and_switch_off() {
        let facts = PreparationFacts {
            missing: vec![],
            connectivity: Connectivity::Offline,
            metered: true,
            prepare_switch_on: false,
            ..base_facts()
        };
        assert_eq!(plan_preparation(&facts), PreparationPhase::NothingMissing);
    }

    #[test]
    fn mtp_42_offline_skips_and_counts_the_missing_files() {
        let facts = PreparationFacts {
            connectivity: Connectivity::Offline,
            ..base_facts()
        };
        assert_eq!(
            plan_preparation(&facts),
            PreparationPhase::SkippedOffline { files: 2 }
        );
    }

    #[test]
    fn mtp_42_offline_beats_metered_and_switch_off_instead_of_offering() {
        // The core precedence claim: offline is a fact about whether the
        // download can run; metered/switch-off is policy about whether it
        // should. A download that cannot run at all must never be dressed
        // up as an `Offered` choice — offline must still win here.
        let facts = PreparationFacts {
            connectivity: Connectivity::Offline,
            metered: true,
            prepare_switch_on: false,
            ..base_facts()
        };
        assert_eq!(
            plan_preparation(&facts),
            PreparationPhase::SkippedOffline { files: 2 }
        );
    }

    #[test]
    fn mtp_42_metered_offers_the_download_instead_of_starting_it() {
        let facts = PreparationFacts {
            metered: true,
            ..base_facts()
        };
        assert_eq!(
            plan_preparation(&facts),
            PreparationPhase::Offered {
                files: 2,
                bytes: 3_000
            }
        );
    }

    #[test]
    fn mtp_42_switch_off_offers_the_download_instead_of_starting_it() {
        let facts = PreparationFacts {
            prepare_switch_on: false,
            ..base_facts()
        };
        assert_eq!(
            plan_preparation(&facts),
            PreparationPhase::Offered {
                files: 2,
                bytes: 3_000
            }
        );
    }

    #[test]
    fn mtp_42_online_unmetered_switch_on_plans_the_download() {
        assert_eq!(
            plan_preparation(&base_facts()),
            PreparationPhase::Planned {
                files: 2,
                bytes: 3_000
            }
        );
    }

    #[test]
    fn mtp_42_planned_downloads_add_the_download_to_the_primary_button() {
        let planned = PreparationPhase::Planned {
            files: 2,
            bytes: 3_000,
        };
        assert_eq!(primary_action(&planned), PrimaryAction::DownloadAndSync);
    }

    #[test]
    fn mtp_42_every_other_phase_leaves_the_primary_button_as_plain_sync() {
        assert_eq!(
            primary_action(&PreparationPhase::Absent),
            PrimaryAction::SyncNow
        );
        assert_eq!(
            primary_action(&PreparationPhase::NothingMissing),
            PrimaryAction::SyncNow
        );
        assert_eq!(
            primary_action(&PreparationPhase::SkippedOffline { files: 2 }),
            PrimaryAction::SyncNow
        );
        assert_eq!(
            primary_action(&PreparationPhase::Offered {
                files: 2,
                bytes: 3_000
            }),
            PrimaryAction::SyncNow
        );
    }
}
