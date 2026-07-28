//! What the add dialog's single input field means for one specific source.
//!
//! `SRC-3a` gives every source exactly one dialog with exactly one field, and
//! `SRC-6` binds that field to the source it was opened from. Both decisions
//! are pure projections of the typed text plus the dialog's own
//! [`PodcastKind`], so they live here rather than in the widget code.

use reprise_core::podcasts::{self, PodcastKind};
use rusqlite::Connection;

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

/// Everything that can stop a submit before any provider work starts, in one
/// place: `SRC-6` refuses a source-foreign URL, and `NET-1a` refuses any network
/// path while the global switch or this source's own switch is off.
///
/// Returns the message to show, or `None` when the submit may proceed.
pub(super) fn submit_refusal(
    conn: &Connection,
    kind: PodcastKind,
    input: &AddInput,
) -> Option<&'static str> {
    if !input_matches_dialog(input, kind) {
        return Some(foreign_url_reason(kind));
    }
    if matches!(input, AddInput::Empty) {
        return None;
    }
    // A failed lookup is treated as "not allowed": refusing a request we are
    // unsure about is the safe direction for a privacy promise.
    if podcasts::config::source_network_allowed(conn, kind).unwrap_or(false) {
        None
    } else {
        Some(strings::ONLINE_SOURCES_TURNED_OFF)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_with(youtube: bool, podcasts: bool, global: bool) -> Connection {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::YOUTUBE_MODULE, youtube)
            .unwrap();
        reprise_core::modules::set_enabled(
            &conn,
            &reprise_core::modules::PODCASTS_MODULE,
            podcasts,
        )
        .unwrap();
        reprise_core::online_sources::set_enabled(&conn, global).unwrap();
        conn
    }

    #[test]
    fn net_1a_a_disabled_source_refuses_every_network_path_of_its_dialog() {
        let off = conn_with(false, false, true);

        for input in [
            classify_input("metal interviews"),
            classify_input("https://feeds.test/show.xml"),
        ] {
            assert_eq!(
                submit_refusal(&off, PodcastKind::Rss, &input),
                Some(strings::ONLINE_SOURCES_TURNED_OFF),
                "search and URL preview must both be refused"
            );
        }
        assert_eq!(
            submit_refusal(
                &off,
                PodcastKind::Youtube,
                &classify_input("https://www.youtube.com/@example")
            ),
            Some(strings::ONLINE_SOURCES_TURNED_OFF)
        );
    }

    #[test]
    fn net_1a_the_global_switch_overrides_an_enabled_source() {
        let global_off = conn_with(true, true, false);
        let terms = classify_input("metal interviews");

        assert_eq!(
            submit_refusal(&global_off, PodcastKind::Rss, &terms),
            Some(strings::ONLINE_SOURCES_TURNED_OFF),
            "the global switch must win over an enabled source"
        );
        assert_eq!(
            submit_refusal(&global_off, PodcastKind::Youtube, &terms),
            Some(strings::ONLINE_SOURCES_TURNED_OFF)
        );
    }

    #[test]
    fn net_1a_an_enabled_source_proceeds_and_an_empty_field_never_reaches_the_network() {
        let on = conn_with(true, true, true);

        assert_eq!(
            submit_refusal(&on, PodcastKind::Rss, &classify_input("metal interviews")),
            None
        );
        assert_eq!(
            submit_refusal(&on, PodcastKind::Rss, &AddInput::Empty),
            None
        );
        // SRC-6 still outranks the gate: a foreign URL is named as foreign.
        assert_eq!(
            submit_refusal(
                &on,
                PodcastKind::Rss,
                &classify_input("https://www.youtube.com/@example")
            ),
            Some(strings::PODCAST_URL_IS_YOUTUBE)
        );
    }

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
