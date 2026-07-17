//! Gettext-backed copy owned by the unified Library filter bar.
//!
//! This lives beside `strings.rs` because that central catalogue is already
//! at the repository's source-size limit.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(in crate::ui) const FILTERS: &str = N_!("FILTER");
pub(in crate::ui) const ADD_FILTER: &str = N_!("Add filter");
pub(in crate::ui) const CLEAR_ALL: &str = N_!("Clear all");
// Active filters are cleared through their removable chips, not a duplicate Reset action.
pub(in crate::ui) const BACK: &str = N_!("Back");
pub(in crate::ui) const SEARCH_VALUES: &str = N_!("Search filter values");
pub(in crate::ui) const NO_FILTERS_AVAILABLE: &str = N_!("All filters are active");
pub(in crate::ui) const BROWSE_GENRE: &str = N_!("Genre");
pub(in crate::ui) const BROWSE_ARTIST: &str = N_!("Artist");
pub(in crate::ui) const BROWSE_ALBUM: &str = N_!("Album");
pub(in crate::ui) const UNKNOWN_GENRE: &str = N_!("Unknown genre");
pub(in crate::ui) const UNKNOWN_ARTIST: &str = N_!("Unknown artist");
pub(in crate::ui) const UNKNOWN_ALBUM: &str = N_!("Unknown album");

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(in crate::ui) fn chip_label(facet: &str, value: &str) -> String {
    formatted(
        N_!("{facet}: {value}"),
        &[("facet", facet), ("value", value)],
    )
}

pub(in crate::ui) fn remove_filter_label(facet: &str, value: &str) -> String {
    formatted(
        N_!("Remove {facet} filter: {value}"),
        &[("facet", facet), ("value", value)],
    )
}

pub(in crate::ui) fn search_chip_label(query: &str) -> String {
    formatted(N_!("⌕ “{query}” in any field"), &[("query", query)])
}

pub(in crate::ui) fn remove_search_label(query: &str) -> String {
    formatted(N_!("Remove search: {query}"), &[("query", query)])
}

pub(in crate::ui) fn result_count(filtered: usize, total: usize) -> String {
    let filtered_number = i64::try_from(filtered).unwrap_or(i64::MAX);
    let total_number = i64::try_from(total).unwrap_or(i64::MAX);
    let filtered_text = reprise_core::format::format_thousands(filtered_number);
    let total_text = reprise_core::format::format_thousands(total_number);
    let plural_count = u32::try_from(total).unwrap_or(u32::MAX);
    if filtered == total {
        let template = crate::i18n::ngettext("{total} track", "{total} tracks", plural_count);
        return crate::i18n::format_message(&template, &[("total", &total_text)]);
    }
    let template = crate::i18n::ngettext(
        "{filtered} of {total} track",
        "{filtered} of {total} tracks",
        plural_count,
    );
    crate::i18n::format_message(
        &template,
        &[("filtered", &filtered_text), ("total", &total_text)],
    )
}

/// Returns the count markup and whether it represents an active restriction.
/// Numbers are digits and commas only, so the inserted count is markup-safe.
pub(in crate::ui) fn result_count_markup(filtered: usize, total: usize) -> (String, bool) {
    if filtered >= total {
        return (result_count(total, total), false);
    }
    let filtered_text =
        reprise_core::format::format_thousands(i64::try_from(filtered).unwrap_or(i64::MAX));
    let total_text =
        reprise_core::format::format_thousands(i64::try_from(total).unwrap_or(i64::MAX));
    let plural_count = u32::try_from(total).unwrap_or(u32::MAX);
    let template = crate::i18n::ngettext(
        "{filtered} of {total} track",
        "{filtered} of {total} tracks",
        plural_count,
    );
    let bold_filtered = format!("<b>{filtered_text}</b>");
    let markup = crate::i18n::format_message(
        &template,
        &[("filtered", &bold_filtered), ("total", &total_text)],
    );
    (markup, true)
}

fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(result_count(96, 96), "96 tracks");
        assert_eq!(result_count(7, 96), "7 of 96 tracks");
        assert_eq!(result_count(1, 1), "1 track");
    }
}
