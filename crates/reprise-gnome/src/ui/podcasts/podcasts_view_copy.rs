//! `SRC-10` copy for the Podcasts/YouTube empty-state pages — split out of
//! `podcasts_view.rs` to keep it under the file-size gate. Carries only
//! words, never a decision: `podcasts_empty_state.rs` decides which case
//! applies, this module supplies what each case says.

use reprise_core::podcasts::PodcastKind;

// `copy` is nested inside `podcasts_view`, so `podcasts_empty_state` (a
// sibling of `podcasts_view` under `podcasts`) is `super::super`, not
// `super`.
use super::super::podcasts_empty_state::PodcastsEmptyState;
use crate::ui::sidebar::sidebar_presentation::NavIcon;
use crate::ui::source_empty_state::SourceEmptyStateCopy;
use crate::ui::strings;

/// The genuine "nothing subscribed yet" empty state's copy (`SRC-10`).
pub(super) fn empty_state_copy(kind: PodcastKind) -> SourceEmptyStateCopy {
    let (icon, title, body, button, secondary) = match kind {
        PodcastKind::Rss => (
            NavIcon::Podcasts.icon_name(),
            strings::PODCAST_NO_PODCASTS,
            strings::PODCAST_NO_PODCASTS_DESCRIPTION,
            strings::PODCAST_ADD,
            strings::PODCAST_NO_PODCASTS_SECONDARY,
        ),
        PodcastKind::Youtube => (
            NavIcon::Youtube.icon_name(),
            strings::YOUTUBE_NO_CHANNELS,
            strings::YOUTUBE_NO_CHANNELS_DESCRIPTION,
            strings::YOUTUBE_NO_CHANNELS_ADD,
            strings::YOUTUBE_NO_CHANNELS_SECONDARY,
        ),
    };
    SourceEmptyStateCopy {
        icon_name: icon,
        title: strings::text(title),
        body: strings::text(body),
        button_label: strings::text(button),
        button_icon_name: "list-add-symbolic",
        secondary_line: Some(strings::text(secondary)),
    }
}

/// `SRC-10` addendum (Block B2): the module-off sibling of the genuine
/// empty state — same geometry, but the one button opens Preferences
/// instead of adding a source, so it never carries a plus icon.
pub(super) fn module_off_copy(kind: PodcastKind) -> SourceEmptyStateCopy {
    let (icon, page_title) = match kind {
        PodcastKind::Rss => (NavIcon::Podcasts.icon_name(), strings::PODCASTS),
        PodcastKind::Youtube => (NavIcon::Youtube.icon_name(), strings::YOUTUBE),
    };
    SourceEmptyStateCopy {
        icon_name: icon,
        title: strings::podcast_source_off_title(&strings::text(page_title)),
        body: strings::text(strings::PODCAST_SOURCE_OFF_DESCRIPTION),
        button_label: strings::text(strings::PODCAST_ENABLE_IN_PREFERENCES),
        // Matches `PageId::OnlineSources`'s own icon in
        // `preferences_window.rs`, so the button visually points at where
        // it lands.
        button_icon_name: "network-server-symbolic",
        secondary_line: None,
    }
}

/// Title, description, and primary-button copy for the three subscribed
/// (non-`List`) states that still share the plain `adw::StatusPage`
/// surface. `List`/`Empty`/`ModuleOff` never reach here — they render
/// elsewhere, so a call with one of those is a caller bug.
pub(super) fn status_copy(state: PodcastsEmptyState) -> (String, String, String) {
    match state {
        PodcastsEmptyState::NoEpisodes => (
            strings::text(strings::PODCAST_NO_EPISODES),
            strings::text(strings::PODCAST_NO_EPISODES_DESCRIPTION),
            strings::text(strings::PODCAST_REFRESH_NOW),
        ),
        PodcastsEmptyState::NoResults => (
            strings::text(strings::SRC_NO_RESULTS_TITLE),
            String::new(),
            strings::text(strings::SRC_CLEAR_FILTERS),
        ),
        PodcastsEmptyState::NoDownloads => (
            strings::text(strings::PODCAST_NO_DOWNLOADS),
            strings::text(strings::PODCAST_NO_DOWNLOADS_DESCRIPTION),
            strings::text(strings::SRC_CLEAR_FILTERS),
        ),
        PodcastsEmptyState::List | PodcastsEmptyState::Empty | PodcastsEmptyState::ModuleOff => {
            unreachable!("status_copy is only called for the three status-page states")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_10_the_module_off_button_never_carries_the_add_icon() {
        assert_ne!(
            module_off_copy(PodcastKind::Youtube).button_icon_name,
            "list-add-symbolic"
        );
        assert_eq!(
            module_off_copy(PodcastKind::Youtube).title,
            "YouTube is turned off"
        );
        assert_eq!(
            module_off_copy(PodcastKind::Youtube).button_label,
            "Enable in Preferences"
        );
    }

    #[test]
    fn src_10_no_results_and_no_downloads_carry_different_titles() {
        let (no_results_title, ..) = status_copy(PodcastsEmptyState::NoResults);
        let (no_downloads_title, ..) = status_copy(PodcastsEmptyState::NoDownloads);
        assert_eq!(no_results_title, "Nothing matches these filters");
        assert_eq!(no_downloads_title, "Nothing downloaded yet");
        assert_ne!(no_results_title, no_downloads_title);
    }
}
