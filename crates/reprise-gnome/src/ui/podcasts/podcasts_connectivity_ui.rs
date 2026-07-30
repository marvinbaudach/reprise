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

impl PodcastsView {
    /// Sets the one explicit connectivity seam. Reconnect drains transient
    /// user actions in click order, then invokes the persistent
    /// `wanted_on_device` runner.
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
        if value == Connectivity::Offline && !self.groups.borrow().is_empty() {
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
            self.request_run_queued();
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
