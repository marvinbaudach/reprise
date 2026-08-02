//! GTK renderer for filter-bar messages described by `reprise-view`.

use reprise_view::strings::browse as messages;

pub(in crate::ui) use messages::{
    ADD_FILTER, BACK, BROWSE_ALBUM, BROWSE_ARTIST, BROWSE_GENRE, BROWSE_RATING, BROWSE_YEAR,
    CLEAR_ALL, FILTERS, NO_FILTERS_AVAILABLE, SEARCH_VALUES, UNKNOWN_ALBUM, UNKNOWN_ARTIST,
    UNKNOWN_GENRE, UNKNOWN_RATING, UNKNOWN_YEAR,
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

pub(in crate::ui) fn search_chip_label(query: &str) -> String {
    render(&messages::search_chip_label(query))
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
        for (name, value) in &mut message.args {
            if *name == "filtered" {
                *value = format!("<b>{value}</b>");
                break;
            }
        }
    }
    (render(&message), restricted)
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

    // UX FIL-1a: the headerbar search renders as the first chip.
    #[test]
    fn fil_1a_search_chip_label_quotes_the_query() {
        assert_eq!(search_chip_label("falling"), "⌕ “falling” in any field");
        assert_eq!(remove_search_label("falling"), "Remove search: falling");
    }

    // UX FIL-2: the count is accented (bold markup) only under restriction.
    #[test]
    fn fil_2_count_markup_accents_only_when_restricted() {
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
