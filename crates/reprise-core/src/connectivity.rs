//! The connectivity seam and the offline presentation contract (`NET-3`).
//!
//! Turn 6's design puts it plainly: **offline is a state, not an error.**
//! Everything already downloaded stays fully playable; an online action a
//! user starts while offline is accepted, marked, and (once `NET-3c` builds
//! the runner) carried out automatically once the connection returns. This
//! module is the pure, display-free projection that decides what a row
//! reads and what an action does, given connectivity and whether the item
//! already has a local file. Nothing here renders anything — see
//! `NET-3a`/`NET-3b` in `docs/ux-rules.md` for who consults it and how.
//!
//! ## What `Connectivity` can and cannot know
//!
//! There is no reliable way to ask "are we online" without either an
//! explicit signal or a guess after a failed request. Guessing is the
//! anti-pattern this seam avoids: DNS can resolve while the actual provider
//! host is down, a captive portal answers HTTP 200 for everything, and a
//! single failed request says nothing about the next one. Issues #104 and
//! #106 record that no deterministic provider/network loop has been run in
//! this codebase — so this module makes **no** claim about detecting real
//! connectivity. `Connectivity` is deliberately just an explicit, injectable
//! value: something outside this module sets it (a future OS network-monitor
//! binding, or a test), and every offline-aware projection reads it here
//! instead of inferring it locally. What it can tell you: whether the app
//! currently believes a network path exists. What it cannot tell you:
//! whether a *specific* provider is reachable, authenticated, or
//! rate-limited — those are per-request outcomes (`NET-3`'s authentication /
//! rate-limit / provider-failure states), not a connectivity state, and only
//! show up once a request is actually attempted.
//!
//! ## The critical distinction from `NET-1a`
//!
//! "Switched off" (the global `online-sources-enabled` gate or a module's
//! own switch, `NET-1a`) and "offline" (this module) are different states
//! and must never be conflated. Switched off is a privacy promise: it
//! refuses **both** search and the URL path, no exceptions — see
//! `reprise_core::online_sources::network_allowed` and its callers
//! (`podcasts::add_dialog_input::submit_refusal`,
//! `radio::add_dialog::submit`). Offline refuses nothing; it defers. This
//! module never inspects a module's enabled flag — only connectivity.

/// Whether the app currently believes a network path exists. Explicit and
/// injectable only — see the module docs for what this can and cannot know.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Connectivity {
    #[default]
    Online,
    Offline,
}

impl Connectivity {
    #[must_use]
    pub const fn is_offline(self) -> bool {
        matches!(self, Self::Offline)
    }
}

/// Whether an item already has a locally playable/transferable file. This
/// is the only "local state" the projection needs — download history,
/// retries, and provider identity live elsewhere (e.g.
/// `podcasts::download_state::DownloadState`, which has its own
/// [`crate::podcasts::download_state::DownloadState::local_availability`]
/// bridge into this type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalAvailability {
    Available,
    Missing,
}

/// What a row reads, given connectivity and local availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowPresentation {
    /// Fully playable from the local file — nothing about local playback
    /// touches the network, online or offline.
    Playable,
    /// Nothing local yet; stays listed from cache but reads "Needs
    /// network" and is dimmed, never hidden. Only shown while offline —
    /// while online, an undownloaded item is a normal streaming candidate.
    NeedsNetwork,
}

/// What an action does, given connectivity and (for a deferrable action)
/// local availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    /// Runs immediately: either online, or (for a deferrable action) the
    /// file is already local so no network is needed to act on it.
    RunsNow,
    /// Accepted and marked "Queued offline". `NET-3c` (not built yet) is
    /// the runner that replays queued actions automatically once
    /// connectivity returns; this module only models the state.
    QueuedOffline,
    /// Radio's exception (`NET-3b`): a live stream cannot be deferred, so
    /// there is nothing to queue — only a retry once online.
    NoConnectionRetry,
}

/// `NET-3a`: what a row reads.
#[must_use]
pub fn row_presentation(connectivity: Connectivity, local: LocalAvailability) -> RowPresentation {
    match (connectivity, local) {
        (_, LocalAvailability::Available) => RowPresentation::Playable,
        (Connectivity::Online, LocalAvailability::Missing) => RowPresentation::Playable,
        (Connectivity::Offline, LocalAvailability::Missing) => RowPresentation::NeedsNetwork,
    }
}

/// `NET-3a`: what a deferrable action (download, or a device sync of an
/// item) does. A deferrable action can wait for the network, unlike a live
/// stream — see [`live_stream_action_outcome`].
///
/// Phone sync's rule falls straight out of this: MTP transfer itself is
/// local, so an already-downloaded file's sync `RunsNow` even offline; only
/// an item still missing its file waits (`QueuedOffline`).
#[must_use]
pub fn deferrable_action_outcome(
    connectivity: Connectivity,
    local: LocalAvailability,
) -> ActionOutcome {
    if connectivity == Connectivity::Online {
        return ActionOutcome::RunsNow;
    }
    match local {
        LocalAvailability::Available => ActionOutcome::RunsNow,
        LocalAvailability::Missing => ActionOutcome::QueuedOffline,
    }
}

/// `NET-3b`: what Radio's Play action does. A live stream cannot be
/// deferred, so offline never produces `QueuedOffline` here — only
/// `NoConnectionRetry`.
#[must_use]
pub const fn live_stream_action_outcome(connectivity: Connectivity) -> ActionOutcome {
    match connectivity {
        Connectivity::Online => ActionOutcome::RunsNow,
        Connectivity::Offline => ActionOutcome::NoConnectionRetry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3a_downloaded_content_stays_playable_regardless_of_connectivity() {
        assert_eq!(
            row_presentation(Connectivity::Online, LocalAvailability::Available),
            RowPresentation::Playable
        );
        assert_eq!(
            row_presentation(Connectivity::Offline, LocalAvailability::Available),
            RowPresentation::Playable
        );
    }

    #[test]
    fn net_3a_not_downloaded_rows_read_needs_network_only_while_offline() {
        assert_eq!(
            row_presentation(Connectivity::Online, LocalAvailability::Missing),
            RowPresentation::Playable,
            "online and undownloaded is a normal streaming candidate, not a network warning"
        );
        assert_eq!(
            row_presentation(Connectivity::Offline, LocalAvailability::Missing),
            RowPresentation::NeedsNetwork
        );
    }

    #[test]
    fn net_3a_deferrable_actions_run_now_online_and_queue_when_offline_and_not_yet_local() {
        assert_eq!(
            deferrable_action_outcome(Connectivity::Online, LocalAvailability::Missing),
            ActionOutcome::RunsNow
        );
        assert_eq!(
            deferrable_action_outcome(Connectivity::Offline, LocalAvailability::Missing),
            ActionOutcome::QueuedOffline
        );
    }

    #[test]
    fn net_3a_phone_sync_of_an_already_downloaded_file_runs_even_offline() {
        // MTP is local: syncing a file that is already on disk does not
        // need the network, so it must not wait behind the queue.
        assert_eq!(
            deferrable_action_outcome(Connectivity::Offline, LocalAvailability::Available),
            ActionOutcome::RunsNow
        );
    }

    #[test]
    fn net_3a_radio_live_stream_never_queues_it_only_offers_retry() {
        assert_eq!(
            live_stream_action_outcome(Connectivity::Online),
            ActionOutcome::RunsNow
        );
        assert_eq!(
            live_stream_action_outcome(Connectivity::Offline),
            ActionOutcome::NoConnectionRetry
        );
    }
}
