use std::rc::Rc;

use reprise_core::connectivity::Connectivity;
use reprise_core::podcasts::pipeline::RefreshFailure;
use reprise_core::source_error::{
    source_failure_presentation, FailureAction, FailureSurface, SourceError, SourceErrorKind,
    SourceSurface,
};

use super::{PodcastsView, FAILURE_PAGE};
use crate::ui::strings;

fn safe_action_error_message(_technical_error: &str) -> String {
    strings::text(strings::SOURCE_ACTION_FAILED)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RefreshFailureNotice {
    kind: SourceErrorKind,
    subscription_id: Option<i64>,
    failed_sources: usize,
    source_titles: Vec<String>,
}

fn refresh_failure_notice(
    connectivity: Connectivity,
    failures: &[RefreshFailure],
) -> Option<RefreshFailureNotice> {
    let first = failures.first()?;
    Some(RefreshFailureNotice {
        kind: if connectivity == Connectivity::Offline {
            SourceErrorKind::Offline
        } else {
            first.kind.clone()
        },
        subscription_id: Some(first.subscription_id),
        failed_sources: failures.len(),
        source_titles: failures
            .iter()
            .map(|failure| failure.title.clone())
            .collect(),
    })
}

fn collected_failure_support(source_titles: &[String], cached_items: usize) -> Option<String> {
    const MAX_VISIBLE_SOURCE_TITLES: usize = 3;

    if source_titles.len() < MAX_VISIBLE_SOURCE_TITLES {
        return None;
    }
    let shown = source_titles
        .iter()
        .take(MAX_VISIBLE_SOURCE_TITLES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    Some(strings::source_collected_failures(
        &shown,
        source_titles.len() - MAX_VISIBLE_SOURCE_TITLES,
        cached_items > 0,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureActionRoute {
    Retry,
    OpenAddDialog,
    OpenYoutubePreferences,
    Unsubscribe(i64),
    None,
}

fn failure_action_route(action: FailureAction, subscription_id: Option<i64>) -> FailureActionRoute {
    match action {
        FailureAction::TryAgain => FailureActionRoute::Retry,
        FailureAction::CheckSubscription | FailureAction::FindNewUrl => {
            FailureActionRoute::OpenAddDialog
        }
        FailureAction::OpenPreferences => FailureActionRoute::OpenYoutubePreferences,
        FailureAction::Unsubscribe => {
            subscription_id.map_or(FailureActionRoute::None, FailureActionRoute::Unsubscribe)
        }
    }
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

    pub(super) fn show_refresh_failures(self: &Rc<Self>, failures: &[RefreshFailure]) {
        let Some(notice) = refresh_failure_notice(self.connectivity.get(), failures) else {
            self.clear_fetch_failure();
            return;
        };
        let technical_cause = format!("Failed sources: {}", notice.source_titles.join(", "));
        self.show_refresh_failure(notice, technical_cause);
    }

    pub(super) fn show_unclassified_refresh_failure(
        self: &Rc<Self>,
        technical_cause: impl Into<String>,
    ) {
        let kind = if self.connectivity.get() == Connectivity::Offline {
            SourceErrorKind::Offline
        } else {
            SourceErrorKind::Unreachable
        };
        self.show_refresh_failure(
            RefreshFailureNotice {
                kind,
                subscription_id: None,
                failed_sources: 1,
                source_titles: Vec::new(),
            },
            technical_cause,
        );
    }

    fn show_refresh_failure(
        self: &Rc<Self>,
        notice: RefreshFailureNotice,
        technical_cause: impl Into<String>,
    ) {
        let error = SourceError::new(notice.kind, "Refresh source", technical_cause);
        let cached_items = self.rows.borrow().len();
        let surface = match self.kind {
            reprise_core::podcasts::PodcastKind::Rss => SourceSurface::Podcast,
            reprise_core::podcasts::PodcastKind::Youtube => SourceSurface::Youtube,
        };
        let presentation =
            source_failure_presentation(surface, error.kind(), cached_items, notice.failed_sources);
        let collected_support = collected_failure_support(&notice.source_titles, cached_items);
        self.fetch_failure.replace(Some(error.clone()));
        let occurred_at = chrono::Utc::now().to_rfc3339();
        let last_checked = super::last_updated_text(&self.conn);
        let subscription_id = notice.subscription_id;
        let weak = Rc::downgrade(self);
        let on_action = move |action| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            match failure_action_route(action, subscription_id) {
                FailureActionRoute::Retry => {
                    view.request_refresh(true);
                }
                FailureActionRoute::OpenYoutubePreferences => {
                    if let Some(callback) = view.on_open_youtube_preferences.borrow().clone() {
                        callback();
                    }
                }
                FailureActionRoute::OpenAddDialog => {
                    view.open_add_dialog();
                }
                FailureActionRoute::Unsubscribe(subscription_id) => {
                    view.unsubscribe(subscription_id);
                }
                FailureActionRoute::None => {}
            }
        };
        match presentation.surface {
            FailureSurface::Banner => {
                let support = if matches!(error.kind(), SourceErrorKind::Offline) {
                    strings::source_offline_description(&last_checked)
                } else if let Some(support) = collected_support {
                    support
                } else {
                    strings::source_cached_episodes_still_work(cached_items, &last_checked)
                };
                self.error_banner
                    .show(&presentation, &support, &error, &occurred_at, on_action);
            }
            FailureSurface::FullArea => {
                self.error_banner.hide();
                let description = collected_support.unwrap_or_else(|| {
                    strings::text(match self.kind {
                        reprise_core::podcasts::PodcastKind::Rss => {
                            strings::SOURCE_PODCAST_EMPTY_FAILURE_DESCRIPTION
                        }
                        reprise_core::podcasts::PodcastKind::Youtube => {
                            strings::SOURCE_YOUTUBE_EMPTY_FAILURE_DESCRIPTION
                        }
                    })
                });
                self.failure_state.show(
                    &presentation,
                    &description,
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
    use reprise_core::connectivity::Connectivity;
    use reprise_core::podcasts::pipeline::RefreshFailure;

    #[test]
    fn net_3_d_technical_text_never_reaches_the_action_toast() {
        let message =
            safe_action_error_message("HTTP 599 private.example exception /home/user/token");
        for forbidden in ["HTTP", "599", "private.example", "/home/", "token"] {
            assert!(!message.contains(forbidden), "{message}");
        }
    }

    #[test]
    fn net_3_d_refresh_notice_preserves_each_typed_kind_and_subscription_target() {
        let cases = [
            SourceErrorKind::SourceGone,
            SourceErrorKind::RateLimited { retry_after: None },
            SourceErrorKind::HelperOutdated,
        ];
        for kind in cases {
            let failure = RefreshFailure {
                subscription_id: 42,
                title: "Source title".to_owned(),
                kind: kind.clone(),
            };

            let notice = refresh_failure_notice(Connectivity::Online, &[failure]).unwrap();

            assert_eq!(notice.subscription_id, Some(42));
            assert_eq!(notice.kind, kind);
            assert_eq!(notice.failed_sources, 1);
            assert_eq!(notice.source_titles, ["Source title"]);
        }
    }

    #[test]
    fn net_3_offline_connectivity_overrides_request_kind_without_losing_the_target() {
        let failure = RefreshFailure {
            subscription_id: 7,
            title: "Saved show".to_owned(),
            kind: SourceErrorKind::Unreachable,
        };

        let notice = refresh_failure_notice(Connectivity::Offline, &[failure]).unwrap();

        assert_eq!(notice.kind, SourceErrorKind::Offline);
        assert_eq!(notice.subscription_id, Some(7));
    }

    #[test]
    fn net_3_d_source_gone_actions_keep_the_subscription_target() {
        assert_eq!(
            failure_action_route(FailureAction::CheckSubscription, Some(42)),
            FailureActionRoute::OpenAddDialog
        );
        assert_eq!(
            failure_action_route(FailureAction::Unsubscribe, Some(42)),
            FailureActionRoute::Unsubscribe(42)
        );
        assert_eq!(
            failure_action_route(FailureAction::OpenPreferences, Some(42)),
            FailureActionRoute::OpenYoutubePreferences
        );
    }

    #[test]
    fn net_3_three_or_more_failures_name_sources_in_one_bounded_notice() {
        let titles = ["Alpha", "Beta", "Gamma", "Delta"].map(str::to_owned);

        let support = collected_failure_support(&titles, 12).unwrap();

        assert_eq!(
            support,
            "Affected: Alpha, Beta, Gamma, and 1 more. Saved episodes and downloads still work."
        );
    }
}
