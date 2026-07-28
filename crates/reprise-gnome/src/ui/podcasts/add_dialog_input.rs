//! What the add dialog's single input field means for one specific source.
//!
//! `SRC-3a` gives every source exactly one dialog with exactly one field, and
//! `SRC-6` binds that field to the source it was opened from. Both decisions
//! are pure projections of the typed text plus the dialog's own
//! [`PodcastKind`], so they live here rather than in the widget code.

use reprise_core::podcasts::{self, PodcastKind};

use crate::ui::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddInput {
    Empty,
    Search(String),
    YoutubeUrl(String),
    FeedUrl(String),
}

pub(super) fn classify_input(input: &str) -> AddInput {
    let input = input.trim();
    if input.is_empty() {
        return AddInput::Empty;
    }
    match podcasts::url_detect::detect(input) {
        podcasts::url_detect::InputKind::Search => AddInput::Search(input.to_owned()),
        podcasts::url_detect::InputKind::YoutubeUrl => AddInput::YoutubeUrl(input.to_owned()),
        podcasts::url_detect::InputKind::ProbableFeedUrl => AddInput::FeedUrl(input.to_owned()),
    }
}

/// `SRC-6`: the dialog only accepts input belonging to its own provider. A
/// source-foreign URL is refused with a reason instead of being handed over to
/// the other dialog behind the user's back.
pub(super) fn input_matches_dialog(input: &AddInput, kind: PodcastKind) -> bool {
    match input {
        AddInput::Empty | AddInput::Search(_) => true,
        AddInput::YoutubeUrl(_) => kind == PodcastKind::Youtube,
        AddInput::FeedUrl(_) => kind == PodcastKind::Rss,
    }
}

/// The one-line reason shown for a refused source-foreign URL.
pub(super) const fn foreign_url_reason(kind: PodcastKind) -> &'static str {
    match kind {
        PodcastKind::Rss => strings::PODCAST_URL_IS_YOUTUBE,
        PodcastKind::Youtube => strings::YOUTUBE_URL_IS_FEED,
    }
}

pub(super) const fn dialog_title(kind: PodcastKind) -> &'static str {
    match kind {
        PodcastKind::Rss => strings::PODCAST_DIALOG_TITLE,
        PodcastKind::Youtube => strings::YOUTUBE_DIALOG_TITLE,
    }
}

pub(super) const fn dialog_hint(kind: PodcastKind) -> &'static str {
    match kind {
        PodcastKind::Rss => strings::PODCAST_DIALOG_HINT,
        PodcastKind::Youtube => strings::YOUTUBE_DIALOG_HINT,
    }
}

/// The primary button's label and whether it may be pressed.
pub(super) fn primary_action(input: &str, kind: PodcastKind) -> (&'static str, bool) {
    let parsed = classify_input(input);
    let label = match parsed {
        AddInput::Empty | AddInput::Search(_) => strings::PODCAST_SEARCH,
        AddInput::YoutubeUrl(_) | AddInput::FeedUrl(_) => strings::PODCAST_PREVIEW,
    };
    let sensitive = !matches!(parsed, AddInput::Empty) && input_matches_dialog(&parsed, kind);
    (label, sensitive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_3a_add_dialog_submits_search_or_url_through_one_field() {
        assert_eq!(classify_input("   "), AddInput::Empty);
        assert!(matches!(
            classify_input("metal interviews"),
            AddInput::Search(_)
        ));
        assert!(matches!(
            classify_input("https://feeds.test/show.xml"),
            AddInput::FeedUrl(_)
        ));
        assert!(matches!(
            classify_input("https://www.youtube.com/@example"),
            AddInput::YoutubeUrl(_)
        ));
    }

    #[test]
    fn src_6_source_foreign_urls_are_refused_by_each_dialog() {
        let feed = classify_input("https://feeds.test/show.xml");
        let channel = classify_input("https://www.youtube.com/@example");

        assert!(input_matches_dialog(&feed, PodcastKind::Rss));
        assert!(!input_matches_dialog(&feed, PodcastKind::Youtube));
        assert!(input_matches_dialog(&channel, PodcastKind::Youtube));
        assert!(!input_matches_dialog(&channel, PodcastKind::Rss));
    }

    #[test]
    fn src_6_plain_search_terms_belong_to_both_dialogs() {
        let terms = classify_input("metal interviews");

        assert!(input_matches_dialog(&terms, PodcastKind::Rss));
        assert!(input_matches_dialog(&terms, PodcastKind::Youtube));
        assert!(input_matches_dialog(&AddInput::Empty, PodcastKind::Rss));
    }

    #[test]
    fn src_6_a_refused_url_never_enables_the_primary_action() {
        let (label, sensitive) =
            primary_action("https://www.youtube.com/@example", PodcastKind::Rss);

        assert_eq!(label, strings::PODCAST_PREVIEW);
        assert!(
            !sensitive,
            "a source-foreign URL must not be submittable from the podcast dialog"
        );
        assert_eq!(
            foreign_url_reason(PodcastKind::Rss),
            strings::PODCAST_URL_IS_YOUTUBE
        );
        assert_eq!(
            foreign_url_reason(PodcastKind::Youtube),
            strings::YOUTUBE_URL_IS_FEED
        );
    }

    #[test]
    fn src_6_each_dialog_carries_its_own_identity() {
        assert_eq!(
            dialog_title(PodcastKind::Rss),
            strings::PODCAST_DIALOG_TITLE
        );
        assert_eq!(
            dialog_title(PodcastKind::Youtube),
            strings::YOUTUBE_DIALOG_TITLE
        );
        assert_eq!(dialog_hint(PodcastKind::Rss), strings::PODCAST_DIALOG_HINT);
        assert_eq!(
            dialog_hint(PodcastKind::Youtube),
            strings::YOUTUBE_DIALOG_HINT
        );
    }
}
