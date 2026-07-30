use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::source_error::{
    FailureAction, FailureHeadline, SourceError, SourceFailurePresentation,
};

use crate::ui::source_error_details::SourceErrorDetails;
use crate::ui::strings;

pub(super) fn headline_text(headline: FailureHeadline) -> String {
    match headline {
        FailureHeadline::CouldNotCheckChannel => {
            strings::text(strings::SOURCE_COULD_NOT_CHECK_CHANNEL)
        }
        FailureHeadline::CouldNotReachYoutube => {
            strings::text(strings::SOURCE_COULD_NOT_REACH_YOUTUBE)
        }
        FailureHeadline::CouldNotReachSource => strings::text(strings::SOURCE_COULD_NOT_REACH),
        FailureHeadline::PodcastMovedOrEnded => strings::text(strings::SOURCE_PODCAST_MOVED),
        FailureHeadline::YoutubeRateLimited => strings::text(strings::SOURCE_YOUTUBE_LIMITING),
        FailureHeadline::YoutubeHelperNeedsUpdate => {
            strings::text(strings::SOURCE_YOUTUBE_HELPER_UPDATE)
        }
        FailureHeadline::RadioNotBroadcasting => strings::text(strings::RADIO_RECONNECT_FAILED),
        FailureHeadline::CouldNotRefreshConcerts => {
            strings::text(strings::CONCERTS_COULD_NOT_REFRESH)
        }
        FailureHeadline::ConcertsNeedsConfiguration => {
            strings::text(strings::CONCERTS_NEEDS_CONFIGURATION)
        }
        FailureHeadline::CouldNotRefreshNewReleases => {
            strings::text(strings::RELEASES_COULD_NOT_REFRESH)
        }
        FailureHeadline::Offline => strings::text(strings::SOURCE_OFFLINE),
        FailureHeadline::SeveralSourcesCouldNotRefresh { count } => {
            strings::source_several_failed(count)
        }
    }
}

/// The one place a failure action is named. Both the banner and the full-area
/// state offer the same actions, and two copies of this match were free to
/// drift into two different words for the same button.
pub(in crate::ui) fn action_text(action: FailureAction) -> String {
    strings::text(match action {
        FailureAction::TryAgain => strings::SOURCE_TRY_AGAIN,
        FailureAction::CheckSubscription => strings::SOURCE_CHECK_SUBSCRIPTION,
        FailureAction::Unsubscribe => strings::SOURCE_UNSUBSCRIBE,
        FailureAction::OpenPreferences => strings::SOURCE_OPEN_PREFERENCES,
        FailureAction::FindNewUrl => strings::SOURCE_FIND_NEW_URL,
    })
}

pub(super) struct SourceErrorBanner {
    root: gtk4::Revealer,
    headline: gtk4::Label,
    support: gtk4::Label,
    actions: gtk4::Box,
    details: SourceErrorDetails,
}

impl SourceErrorBanner {
    pub(super) fn new() -> Self {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        content.add_css_class("card");
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let headline = gtk4::Label::new(None);
        headline.add_css_class("heading");
        headline.set_wrap(true);
        headline.set_xalign(0.0);
        content.append(&headline);
        let support = gtk4::Label::new(None);
        support.add_css_class("dim-label");
        support.set_wrap(true);
        support.set_xalign(0.0);
        content.append(&support);
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        content.append(&actions);
        let details = SourceErrorDetails::new();
        content.append(details.widget());
        let root = gtk4::Revealer::new();
        root.set_reveal_child(false);
        root.set_child(Some(&content));
        Self {
            root,
            headline,
            support,
            actions,
            details,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn hide(&self) {
        self.root.set_reveal_child(false);
    }

    pub(super) fn show(
        &self,
        presentation: &SourceFailurePresentation,
        support: &str,
        error: &SourceError,
        occurred_at: &str,
        on_action: impl Fn(FailureAction) + 'static,
    ) {
        self.headline
            .set_text(&headline_text(presentation.headline));
        self.support.set_text(support);
        while let Some(child) = self.actions.first_child() {
            self.actions.remove(&child);
        }
        let callback: Rc<dyn Fn(FailureAction)> = Rc::new(on_action);
        for action in &presentation.actions {
            let button = gtk4::Button::with_label(&action_text(*action));
            button.add_css_class("flat");
            let action = *action;
            let callback = callback.clone();
            button.connect_clicked(move |_| callback(action));
            self.actions.append(&button);
        }
        self.details.set_error(error, occurred_at);
        self.root.set_reveal_child(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3_banner_copy_names_the_failure_without_exposing_technical_text() {
        assert_eq!(
            headline_text(FailureHeadline::CouldNotCheckChannel),
            "Couldn't check this channel for new uploads"
        );
        assert_eq!(
            headline_text(FailureHeadline::YoutubeRateLimited),
            "YouTube is limiting requests right now — try again in a few minutes"
        );
        assert!(!headline_text(FailureHeadline::CouldNotReachSource).contains("HTTP"));
    }

    #[test]
    fn net_3_three_failures_have_one_collected_notice() {
        assert_eq!(
            headline_text(FailureHeadline::SeveralSourcesCouldNotRefresh { count: 3 }),
            "Couldn't refresh 3 sources"
        );
    }

    #[test]
    fn net_3_cached_and_empty_support_copy_names_episodes_and_the_channel() {
        assert_eq!(
            strings::source_cached_episodes_still_work(10, "4 hours ago"),
            "Showing the 10 episodes from 4 hours ago. Downloads play as usual."
        );
        assert_eq!(
            strings::text(strings::SOURCE_YOUTUBE_EMPTY_FAILURE_DESCRIPTION),
            "Nothing is downloaded from this channel yet, so there's nothing to show. \
             Your other channels and your music are unaffected."
        );
    }

    #[test]
    fn pod_19_refresh_failures_use_the_banner_and_never_the_footer() {
        let requests = include_str!("podcasts/podcasts_view_requests.rs");
        assert!(requests.contains("show_refresh_failure"));
        assert!(!requests.contains("PODCAST_REFRESH_FAILED"));
        assert!(!requests.contains("Refresh failed · showing saved episodes"));
    }
}
