//! What the add dialog's single input field means for one specific source.
//!
//! `SRC-3a` gives every source exactly one dialog with exactly one field, and
//! `SRC-6` binds that field to the source it was opened from. Both decisions
//! are pure projections of the typed text plus the dialog's own
//! [`PodcastKind`], so they live here rather than in the widget code.

use reprise_core::connectivity::Connectivity;
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

/// `NET-3` point 4: the one-line reason shown when search needs the network
/// while offline. Distinct from `foreign_url_reason` — this is about
/// connectivity, not which dialog owns the URL; the two never fire at once
/// since `submit_refusal` checks the foreign-URL case first.
pub(super) const fn search_needs_network_reason(kind: PodcastKind) -> &'static str {
    match kind {
        PodcastKind::Rss => strings::PODCAST_SEARCH_NEEDS_NETWORK,
        PodcastKind::Youtube => strings::YOUTUBE_SEARCH_NEEDS_NETWORK,
    }
}

/// `NET-3` point 4: the inline note shown under the entry, live as the user
/// types — `SRC-6`'s foreign-URL refusal first (it would refuse the submit
/// outright), then the offline-search reason, then nothing. `AddInput::
/// Empty` counts as "search mode" here too, so the reason is visible before
/// the user types anything rather than only after a failed attempt.
pub(super) fn dialog_status_hint(
    input: &AddInput,
    kind: PodcastKind,
    connectivity: Connectivity,
) -> &'static str {
    if !input_matches_dialog(input, kind) {
        return foreign_url_reason(kind);
    }
    if connectivity.is_offline() && matches!(input, AddInput::Empty | AddInput::Search(_)) {
        return search_needs_network_reason(kind);
    }
    ""
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

/// `NET-3` point 4 layered on top of [`primary_action`]: offline disables
/// the button only while search is still the classified action — a URL
/// keeps its normal `SRC-6` sensitivity regardless of connectivity, since
/// the URL path never needed `primary_action`'s network to begin with.
pub(super) fn primary_action_for_connectivity(
    input: &str,
    kind: PodcastKind,
    connectivity: Connectivity,
) -> (&'static str, bool) {
    let (label, sensitive) = primary_action(input, kind);
    let offline_search = connectivity.is_offline()
        && matches!(classify_input(input), AddInput::Empty | AddInput::Search(_));
    (label, sensitive && !offline_search)
}

/// Everything that can stop a submit before any provider work starts, in one
/// place: `SRC-6` refuses a source-foreign URL, `NET-1a` refuses any network
/// path while the global switch or this source's own switch is off, and
/// `NET-3` point 4 refuses only the search path while offline — a URL still
/// proceeds, handled separately by `add_dialog::submit`, which skips the
/// preview fetch and subscribes directly instead of failing here.
///
/// Returns the message to show, or `None` when the submit may proceed.
pub(super) fn submit_refusal(
    conn: &Connection,
    kind: PodcastKind,
    input: &AddInput,
    connectivity: Connectivity,
) -> Option<&'static str> {
    if !input_matches_dialog(input, kind) {
        return Some(foreign_url_reason(kind));
    }
    if matches!(input, AddInput::Empty) {
        return None;
    }
    // A failed lookup is treated as "not allowed": refusing a request we are
    // unsure about is the safe direction for a privacy promise.
    if !podcasts::config::source_network_allowed(conn, kind).unwrap_or(false) {
        return Some(strings::ONLINE_SOURCES_TURNED_OFF);
    }
    if connectivity.is_offline() && matches!(input, AddInput::Search(_)) {
        return Some(search_needs_network_reason(kind));
    }
    None
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
                submit_refusal(&off, PodcastKind::Rss, &input, Connectivity::Online),
                Some(strings::ONLINE_SOURCES_TURNED_OFF),
                "search and URL preview must both be refused"
            );
        }
        assert_eq!(
            submit_refusal(
                &off,
                PodcastKind::Youtube,
                &classify_input("https://www.youtube.com/@example"),
                Connectivity::Online,
            ),
            Some(strings::ONLINE_SOURCES_TURNED_OFF)
        );
    }

    #[test]
    fn net_1a_the_global_switch_overrides_an_enabled_source() {
        let global_off = conn_with(true, true, false);
        let terms = classify_input("metal interviews");

        assert_eq!(
            submit_refusal(&global_off, PodcastKind::Rss, &terms, Connectivity::Online),
            Some(strings::ONLINE_SOURCES_TURNED_OFF),
            "the global switch must win over an enabled source"
        );
        assert_eq!(
            submit_refusal(
                &global_off,
                PodcastKind::Youtube,
                &terms,
                Connectivity::Online
            ),
            Some(strings::ONLINE_SOURCES_TURNED_OFF)
        );
    }

    #[test]
    fn net_1a_an_enabled_source_proceeds_and_an_empty_field_never_reaches_the_network() {
        let on = conn_with(true, true, true);

        assert_eq!(
            submit_refusal(
                &on,
                PodcastKind::Rss,
                &classify_input("metal interviews"),
                Connectivity::Online,
            ),
            None
        );
        assert_eq!(
            submit_refusal(
                &on,
                PodcastKind::Rss,
                &AddInput::Empty,
                Connectivity::Online
            ),
            None
        );
        // SRC-6 still outranks the gate: a foreign URL is named as foreign.
        assert_eq!(
            submit_refusal(
                &on,
                PodcastKind::Rss,
                &classify_input("https://www.youtube.com/@example"),
                Connectivity::Online,
            ),
            Some(strings::PODCAST_URL_IS_YOUTUBE)
        );
    }

    #[test]
    fn net_3_offline_refuses_only_search_and_leaves_the_url_path_open() {
        let on = conn_with(true, true, true);
        let search = classify_input("metal interviews");
        let feed_url = classify_input("https://feeds.test/show.xml");
        let channel_url = classify_input("https://www.youtube.com/@example");

        assert_eq!(
            submit_refusal(&on, PodcastKind::Rss, &search, Connectivity::Offline),
            Some(strings::PODCAST_SEARCH_NEEDS_NETWORK),
            "search needs the network and must be refused offline"
        );
        assert_eq!(
            submit_refusal(&on, PodcastKind::Youtube, &search, Connectivity::Offline),
            Some(strings::YOUTUBE_SEARCH_NEEDS_NETWORK)
        );
        assert_eq!(
            submit_refusal(
                &on,
                PodcastKind::Rss,
                &AddInput::Empty,
                Connectivity::Offline
            ),
            None,
            "an empty field is not itself a search attempt"
        );
        assert_eq!(
            submit_refusal(&on, PodcastKind::Rss, &feed_url, Connectivity::Offline),
            None,
            "a URL belonging to this dialog must proceed even while offline"
        );
        assert_eq!(
            submit_refusal(
                &on,
                PodcastKind::Youtube,
                &channel_url,
                Connectivity::Offline
            ),
            None
        );
    }

    #[test]
    fn net_3_dialog_status_hint_prefers_the_foreign_url_reason_over_offline() {
        // `SRC-6`'s refusal would stop the submit outright, so it must win
        // even while offline instead of being masked by the offline reason.
        let foreign = classify_input("https://www.youtube.com/@example");
        assert_eq!(
            dialog_status_hint(&foreign, PodcastKind::Rss, Connectivity::Offline),
            strings::PODCAST_URL_IS_YOUTUBE
        );
    }

    #[test]
    fn net_3_dialog_status_hint_shows_the_offline_reason_before_typing_too() {
        assert_eq!(
            dialog_status_hint(&AddInput::Empty, PodcastKind::Rss, Connectivity::Offline),
            strings::PODCAST_SEARCH_NEEDS_NETWORK,
            "the reason should be visible before the user even starts typing"
        );
        assert_eq!(
            dialog_status_hint(&AddInput::Empty, PodcastKind::Rss, Connectivity::Online),
            "",
            "online has nothing to warn about"
        );
    }

    #[test]
    fn net_3_primary_action_disables_search_offline_but_leaves_a_url_untouched() {
        assert_eq!(
            primary_action_for_connectivity(
                "metal interviews",
                PodcastKind::Rss,
                Connectivity::Offline
            ),
            (strings::PODCAST_SEARCH, false),
            "search must be disabled while offline"
        );
        assert_eq!(
            primary_action_for_connectivity(
                "https://feeds.test/show.xml",
                PodcastKind::Rss,
                Connectivity::Offline
            ),
            (strings::PODCAST_PREVIEW, true),
            "a matching URL keeps its normal sensitivity while offline"
        );
        assert_eq!(
            primary_action_for_connectivity(
                "metal interviews",
                PodcastKind::Rss,
                Connectivity::Online
            ),
            (strings::PODCAST_SEARCH, true),
            "online search stays exactly as before"
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

    // F4's search-refusal half is already covered above by
    // `net_3_offline_refuses_only_search_and_leaves_the_url_path_open`. The
    // other half — that the URL path still actually creates a subscription
    // — is proven by `podcasts::offline_add`'s
    // `net_3_the_offline_url_path_persists_a_real_subscription`, which needs
    // no GTK type at all since that logic now lives entirely in core.
}
