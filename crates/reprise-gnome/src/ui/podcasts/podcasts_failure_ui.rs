use std::rc::Rc;

use reprise_core::source_error::{
    source_failure_presentation, FailureAction, FailureSurface, SourceError, SourceErrorKind,
    SourceSurface,
};

use super::{PodcastsView, FAILURE_PAGE};
use crate::ui::strings;

fn safe_action_error_message(_technical_error: &str) -> String {
    strings::text(strings::SOURCE_ACTION_FAILED)
}

impl PodcastsView {
    pub(in crate::ui::podcasts) fn show_error(&self, technical_error: &str) {
        tracing::warn!(%technical_error, "podcast action failed");
        let message = safe_action_error_message(technical_error);
        if let Some(overlay) = self.toast_overlay.upgrade() {
            let toast = libadwaita::Toast::new(&message);
            toast.set_priority(libadwaita::ToastPriority::High);
            overlay.add_toast(toast);
        }
    }

    pub(in crate::ui::podcasts) fn show_queued_offline(&self) {
        if let Some(overlay) = self.toast_overlay.upgrade() {
            overlay.add_toast(libadwaita::Toast::new(&strings::text(
                strings::PODCAST_QUEUED_OFFLINE,
            )));
        }
    }

    pub(super) fn clear_fetch_failure(&self) {
        self.fetch_failure.replace(None);
        self.error_banner.hide();
        self.render();
    }

    pub(super) fn show_refresh_failure(
        self: &Rc<Self>,
        failed_sources: usize,
        technical_cause: impl Into<String>,
    ) {
        let kind = if self.connectivity.get() == reprise_core::connectivity::Connectivity::Offline {
            SourceErrorKind::Offline
        } else {
            SourceErrorKind::Unreachable
        };
        let error = SourceError::new(kind, "Refresh source", technical_cause);
        let cached_items = self.rows.borrow().len();
        let surface = match self.kind {
            reprise_core::podcasts::PodcastKind::Rss => SourceSurface::Podcast,
            reprise_core::podcasts::PodcastKind::Youtube => SourceSurface::Youtube,
        };
        let presentation =
            source_failure_presentation(surface, error.kind(), cached_items, failed_sources);
        self.fetch_failure.replace(Some(error.clone()));
        let occurred_at = chrono::Utc::now().to_rfc3339();
        let last_checked = super::last_updated_text(&self.conn);
        let weak = Rc::downgrade(self);
        let on_action = move |action| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            match action {
                FailureAction::TryAgain => {
                    view.request_refresh(true);
                }
                FailureAction::OpenPreferences => {
                    if let Some(callback) = view.on_open_youtube_preferences.borrow().clone() {
                        callback();
                    }
                }
                FailureAction::CheckSubscription
                | FailureAction::Unsubscribe
                | FailureAction::FindNewUrl => view.open_add_dialog(),
            }
        };
        match presentation.surface {
            FailureSurface::Banner => {
                let support = if matches!(error.kind(), SourceErrorKind::Offline) {
                    strings::source_offline_description(&last_checked)
                } else {
                    strings::source_cached_episodes_still_work(cached_items, &last_checked)
                };
                self.error_banner
                    .show(&presentation, &support, &error, &occurred_at, on_action);
            }
            FailureSurface::FullArea => {
                self.error_banner.hide();
                let description = match self.kind {
                    reprise_core::podcasts::PodcastKind::Rss => {
                        strings::SOURCE_PODCAST_EMPTY_FAILURE_DESCRIPTION
                    }
                    reprise_core::podcasts::PodcastKind::Youtube => {
                        strings::SOURCE_YOUTUBE_EMPTY_FAILURE_DESCRIPTION
                    }
                };
                self.failure_state.show(
                    &presentation,
                    &strings::text(description),
                    &error,
                    &occurred_at,
                    on_action,
                );
                self.stack.set_visible_child_name(FAILURE_PAGE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3_d_technical_text_never_reaches_the_action_toast() {
        let message =
            safe_action_error_message("HTTP 599 private.example exception /home/user/token");
        for forbidden in ["HTTP", "599", "private.example", "/home/", "token"] {
            assert!(!message.contains(forbidden), "{message}");
        }
    }
}
