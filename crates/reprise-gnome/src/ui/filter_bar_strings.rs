//! GTK renderer for the shared filter-bar messages described by `reprise-view`.

use reprise_view::search_scope::SearchScope;
use reprise_view::strings::browse as messages;

pub(in crate::ui) use messages::{
    ADD_FILTER, BACK, BROWSE_ALBUM, BROWSE_ARTIST, BROWSE_GENRE, BROWSE_RATING, BROWSE_YEAR,
    CLEAR_ALL, NO_FILTERS_AVAILABLE, SEARCH_VALUES, UNKNOWN_ALBUM, UNKNOWN_ARTIST, UNKNOWN_GENRE,
    UNKNOWN_RATING, UNKNOWN_YEAR,
};

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(in crate::ui) fn chip_label(facet: &str, value: &str) -> String {
    render(&messages::chip_label(facet, value))
}

pub(in crate::ui) fn remove_filter_label(facet: &str, value: &str) -> String {
    render(&messages::remove_filter_label(facet, value))
}

/// FIL-1d: the same chip, naming the fields the current view searches.
pub(in crate::ui) fn scoped_search_chip_label(scope: SearchScope, query: &str) -> String {
    render(&messages::search_chip_label_in(scope, query))
}

/// SEARCH-8a: the tooltip on the insensitive lens of a view without a list.
pub(in crate::ui) fn nothing_to_filter(section: &str) -> String {
    render(&messages::nothing_to_filter(section))
}

pub(in crate::ui) fn remove_search_label(query: &str) -> String {
    render(&messages::remove_search_label(query))
}

pub(in crate::ui) fn leave_place_label(place: &str) -> String {
    render(&messages::leave_place_label(place))
}

/// Returns the count markup and whether it represents an active restriction.
/// Numbers are digits and commas only, so the inserted count is markup-safe.
pub(in crate::ui) fn result_count_markup(filtered: usize, total: usize) -> (String, bool) {
    let (mut message, restricted) = messages::result_count_state(filtered, total);
    if restricted {
        accent_filtered(&mut message);
    }
    (render(&message), restricted)
}

fn accent_filtered(message: &mut reprise_view::strings::Message) {
    for (name, value) in &mut message.args {
        if *name == messages::FILTERED_ARG {
            *value = format!("<b>{value}</b>");
            break;
        }
    }
}

fn borrowed<'a>(args: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    args.iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect()
}

fn render(message: &reprise_view::strings::Message) -> String {
    let template = match &message.plural {
        Some(plural) => crate::i18n::ngettext(
            message.id,
            plural.id,
            u32::try_from(plural.count).unwrap_or(u32::MAX),
        ),
        None => crate::i18n::gettext(message.id),
    };
    crate::i18n::format_message(&template, &borrowed(&message.args))
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-1c: the place pill's accessible name says leaving, not removing.
    #[test]
    fn fil_1c_place_pill_label_says_leave_not_remove() {
        assert_eq!(leave_place_label("Lorna Shore"), "Leave Lorna Shore");
    }

    // UX FIL-1d: Music's chip names the three fields its free-text query reads.
    #[test]
    fn fil_1d_music_search_chip_label_names_all_searched_fields() {
        assert_eq!(
            scoped_search_chip_label(SearchScope::Tracks, "falling"),
            "⌕ “falling” in track, artist and album"
        );
        assert_eq!(remove_search_label("falling"), "Remove search: falling");
    }

    // UX FIL-1d: the chip label names the fields each scope actually reads.
    // The remove label stays scope-independent.
    #[test]
    fn fil_1d_chip_label_names_the_fields_of_its_view() {
        let cases = [
            (SearchScope::Tracks, "⌕ “wer” in track, artist and album"),
            (SearchScope::Podcasts, "⌕ “wer” in episode titles"),
            (SearchScope::Youtube, "⌕ “wer” in video titles"),
            (SearchScope::Radio, "⌕ “wer” in station names"),
            (SearchScope::Releases, "⌕ “wer” in title and artist"),
            (SearchScope::Concerts, "⌕ “wer” in artist and venue"),
            (SearchScope::Missing, "⌕ “wer” in file paths"),
        ];

        for (scope, expected) in cases {
            assert_eq!(
                scoped_search_chip_label(scope, "wer"),
                expected,
                "{scope:?}"
            );
        }
        assert_eq!(remove_search_label("wer"), "Remove search: wer");
    }

    // UX SEARCH-8a: the lens explains itself where there is nothing to filter.
    #[test]
    fn search_8a_insensitive_lens_names_the_section() {
        assert_eq!(
            nothing_to_filter("My Stats"),
            "Nothing to filter in My Stats"
        );
    }

    // UX FIL-2a: the count is accented (bold markup) only under restriction.
    #[test]
    fn fil_2a_count_markup_accents_only_when_restricted() {
        assert_eq!(
            result_count_markup(15, 1664),
            ("<b>15</b> of 1,664 tracks".to_string(), true)
        );
        assert_eq!(
            result_count_markup(1664, 1664),
            ("1,664 tracks".to_string(), false)
        );
        assert_eq!(result_count_markup(1, 1), ("1 track".to_string(), false));
    }

    #[test]
    fn result_count_uses_compact_total_or_filtered_copy() {
        assert_eq!(render(&messages::result_count(96, 96)), "96 tracks");
        assert_eq!(render(&messages::result_count(7, 96)), "7 of 96 tracks");
        assert_eq!(render(&messages::result_count(1, 1)), "1 track");
    }
}
