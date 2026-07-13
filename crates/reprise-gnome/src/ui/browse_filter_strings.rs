//! Gettext-backed copy owned by the unified Library filter bar.
//!
//! This lives beside `strings.rs` because that central catalogue is already
//! at the repository's source-size limit.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(super) const FILTERS: &str = N_!("FILTER");
pub(super) const ADD_FILTER: &str = N_!("Add filter");
pub(super) const RESET: &str = N_!("Reset");
pub(super) const BACK: &str = N_!("Back");
pub(super) const SEARCH_VALUES: &str = N_!("Search filter values");
pub(super) const NO_FILTERS_AVAILABLE: &str = N_!("All filters are active");
pub(super) const BROWSE_GENRE: &str = N_!("Genre");
pub(super) const BROWSE_ARTIST: &str = N_!("Artist");
pub(super) const BROWSE_ALBUM: &str = N_!("Album");
pub(super) const UNKNOWN_GENRE: &str = N_!("Unknown genre");
pub(super) const UNKNOWN_ARTIST: &str = N_!("Unknown artist");
pub(super) const UNKNOWN_ALBUM: &str = N_!("Unknown album");

pub(super) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(super) fn chip_label(facet: &str, value: &str) -> String {
    formatted(
        N_!("{facet}: {value}"),
        &[("facet", facet), ("value", value)],
    )
}

pub(super) fn remove_filter_label(facet: &str, value: &str) -> String {
    formatted(
        N_!("Remove {facet} filter: {value}"),
        &[("facet", facet), ("value", value)],
    )
}

pub(super) fn result_count(filtered: usize, total: usize) -> String {
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

fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_count_uses_compact_total_or_filtered_copy() {
        assert_eq!(result_count(96, 96), "96 tracks");
        assert_eq!(result_count(7, 96), "7 of 96 tracks");
        assert_eq!(result_count(1, 1), "1 track");
    }
}
