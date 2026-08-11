use std::rc::Rc;

use reprise_core::connectivity::Connectivity;
use reprise_core::source_error::SourceErrorKind;

use super::{replay_until_refused, DeferredAction, PodcastsView};

fn should_clear_failure(
    previous: Connectivity,
    current: Connectivity,
    failure_kind: Option<&SourceErrorKind>,
) -> bool {
    previous == Connectivity::Offline
        && current == Connectivity::Online
        && matches!(failure_kind, Some(SourceErrorKind::Offline))
}

/// Whether going offline may post its notice.
///
/// Only when nothing more specific is already on screen. A provider failure —
/// a feed that returned 404, a source being rate-limited, a helper needing an
/// update — outlives the connection dropping and stays true while offline, and
/// it is the one thing the user could act on. Overwriting it with "You're
/// offline" loses that: the notice would then read as merely transient, and
/// `should_clear_failure` would remove it on reconnect because the kind it now
/// carries *is* `Offline`. The source would still be gone and the app would
/// never have said so again.
fn should_show_offline_notice(
    current: Connectivity,
    has_cached_items: bool,
    failure_kind: Option<&SourceErrorKind>,
) -> bool {
    current == Connectivity::Offline
        && has_cached_items
        && matches!(failure_kind, None | Some(SourceErrorKind::Offline))
}

impl PodcastsView {
    /// Sets the one explicit connectivity seam. Reconnect drains transient
    /// user actions in click order.
    pub(in crate::ui) fn set_connectivity(self: &Rc<Self>, value: Connectivity) {
        let previous = self.connectivity.replace(value);
        let failure_kind = self
            .fetch_failure
            .borrow()
            .as_ref()
            .map(|error| error.kind().clone());
        if should_clear_failure(previous, value, failure_kind.as_ref()) {
            self.clear_fetch_failure();
        } else {
            self.render();
        }
        if should_show_offline_notice(
            value,
            !self.groups.borrow().is_empty(),
            failure_kind.as_ref(),
        ) {
            self.show_unclassified_refresh_failure(
                "NetworkMonitor reports no available connection",
            );
        }
        if previous == Connectivity::Offline && value == Connectivity::Online {
            let actions = self.deferred_actions.borrow_mut().drain();
            let remaining = replay_until_refused(&actions, |action| match action {
                DeferredAction::Download(episode_id) => self.dispatch_download(episode_id),
                DeferredAction::LoadMore {
                    subscription_id,
                    end,
                } => self.request_load_more(subscription_id, end),
            });
            if !remaining.is_empty() {
                self.deferred_actions.borrow_mut().prepend(remaining);
            }
        }
    }

    pub(in crate::ui) fn connectivity(&self) -> Connectivity {
        self.connectivity.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3_going_offline_never_replaces_a_provider_failure() {
        // The flap that used to lose a notice: a feed returns 404, the network
        // drops, the network returns. Before the guard, step two rewrote the
        // notice as `Offline` and step three cleared it as merely transient —
        // the podcast was still gone and the app never said so again.
        for kind in [
            SourceErrorKind::SourceGone,
            SourceErrorKind::RateLimited { retry_after: None },
            SourceErrorKind::HelperOutdated,
            SourceErrorKind::Unreachable,
        ] {
            assert!(
                !should_show_offline_notice(Connectivity::Offline, true, Some(&kind)),
                "{kind:?} must survive the connection dropping"
            );
        }

        assert!(should_show_offline_notice(
            Connectivity::Offline,
            true,
            None
        ));
        assert!(should_show_offline_notice(
            Connectivity::Offline,
            true,
            Some(&SourceErrorKind::Offline)
        ));
        assert!(
            !should_show_offline_notice(Connectivity::Offline, false, None),
            "with nothing cached the full-area state speaks, not a banner"
        );
        assert!(!should_show_offline_notice(
            Connectivity::Online,
            true,
            None
        ));
    }

    #[test]
    fn net_3_reconnect_clears_only_the_offline_notice() {
        assert!(should_clear_failure(
            Connectivity::Offline,
            Connectivity::Online,
            Some(&SourceErrorKind::Offline),
        ));
        assert!(!should_clear_failure(
            Connectivity::Offline,
            Connectivity::Online,
            Some(&SourceErrorKind::SourceGone),
        ));
        assert!(!should_clear_failure(
            Connectivity::Online,
            Connectivity::Online,
            Some(&SourceErrorKind::Offline),
        ));
    }
}
