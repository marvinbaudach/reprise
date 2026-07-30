use std::rc::Rc;

use reprise_core::connectivity::Connectivity;

use super::{DeferredAction, PodcastsView};

impl PodcastsView {
    /// Sets the one explicit connectivity seam. Reconnect drains transient
    /// user actions in click order, then invokes the persistent
    /// `wanted_on_device` runner.
    pub(in crate::ui) fn set_connectivity(self: &Rc<Self>, value: Connectivity) {
        let previous = self.connectivity.replace(value);
        self.render();
        if value == Connectivity::Offline && !self.groups.borrow().is_empty() {
            self.show_refresh_failure(1, "NetworkMonitor reports no available connection");
        }
        if previous == Connectivity::Offline && value == Connectivity::Online {
            for action in self.deferred_actions.borrow_mut().drain() {
                match action {
                    DeferredAction::Download(episode_id) => self.dispatch_download(episode_id),
                    DeferredAction::LoadMore {
                        subscription_id,
                        end,
                    } => {
                        self.request_load_more(subscription_id, end);
                    }
                }
            }
            self.request_run_queued();
        }
    }

    pub(in crate::ui) fn connectivity(&self) -> Connectivity {
        self.connectivity.get()
    }
}
