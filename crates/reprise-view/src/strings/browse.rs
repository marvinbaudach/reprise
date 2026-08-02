//! Filter-bar messages shared by every surface.

use super::{plural, Message, Plural};

pub const FILTERS: &str = N_!("FILTER");
pub const ADD_FILTER: &str = N_!("Add filter");
pub const CLEAR_ALL: &str = N_!("Clear all");
// Active filters are cleared through their removable chips, not a duplicate Reset action.
pub const BACK: &str = N_!("Back");
pub const SEARCH_VALUES: &str = N_!("Search filter values");
pub const NO_FILTERS_AVAILABLE: &str = N_!("All filters are active");
pub const BROWSE_GENRE: &str = N_!("Genre");
pub const BROWSE_ARTIST: &str = N_!("Artist");
pub const BROWSE_ALBUM: &str = N_!("Album");
pub const BROWSE_YEAR: &str = N_!("Year");
pub const BROWSE_RATING: &str = N_!("Rating");
pub const UNKNOWN_GENRE: &str = N_!("Unknown genre");
pub const UNKNOWN_ARTIST: &str = N_!("Unknown artist");
pub const UNKNOWN_ALBUM: &str = N_!("Unknown album");
pub const UNKNOWN_YEAR: &str = N_!("Unknown year");
pub const UNKNOWN_RATING: &str = N_!("Unrated");

const CHIP_LABEL: &str = N_!("{facet}: {value}");
const REMOVE_FILTER_LABEL: &str = N_!("Remove {facet} filter: {value}");
const SEARCH_CHIP_LABEL: &str = N_!("⌕ “{query}” in any field");
const REMOVE_SEARCH_LABEL: &str = N_!("Remove search: {query}");
const LEAVE_PLACE_LABEL: &str = N_!("Leave {place}");
const TOTAL_TRACKS: (&str, &str) = plural("{total} track", "{total} tracks");
const FILTERED_TRACKS: (&str, &str) = plural(
    "{filtered} of {total} track",
    "{filtered} of {total} tracks",
);

pub fn chip_label(facet: &str, value: &str) -> Message {
    Message {
        id: CHIP_LABEL,
        plural: None,
        args: vec![("facet", facet.to_owned()), ("value", value.to_owned())],
    }
}

pub fn remove_filter_label(facet: &str, value: &str) -> Message {
    Message {
        id: REMOVE_FILTER_LABEL,
        plural: None,
        args: vec![("facet", facet.to_owned()), ("value", value.to_owned())],
    }
}

pub fn search_chip_label(query: &str) -> Message {
    message_with_one_arg(SEARCH_CHIP_LABEL, "query", query)
}

pub fn remove_search_label(query: &str) -> Message {
    message_with_one_arg(REMOVE_SEARCH_LABEL, "query", query)
}

pub fn leave_place_label(place: &str) -> Message {
    message_with_one_arg(LEAVE_PLACE_LABEL, "place", place)
}

/// The argument a surface may single out when a restriction is active — GTK
/// renders it bold. Naming it here rather than repeating the literal on the
/// other side of the crate boundary means a rename breaks the build instead
/// of silently dropping the accent.
pub const FILTERED_ARG: &str = "filtered";

pub fn result_count(filtered: usize, total: usize) -> Message {
    let filtered_number = i64::try_from(filtered).unwrap_or(i64::MAX);
    let total_number = i64::try_from(total).unwrap_or(i64::MAX);
    let filtered_text = reprise_core::format::format_thousands(filtered_number);
    let total_text = reprise_core::format::format_thousands(total_number);
    let plural_count = u32::try_from(total).unwrap_or(u32::MAX);
    if filtered == total {
        return Message {
            id: TOTAL_TRACKS.0,
            plural: Some(Plural {
                id: TOTAL_TRACKS.1,
                count: u64::from(plural_count),
            }),
            args: vec![("total", total_text)],
        };
    }
    Message {
        id: FILTERED_TRACKS.0,
        plural: Some(Plural {
            id: FILTERED_TRACKS.1,
            count: u64::from(plural_count),
        }),
        args: vec![(FILTERED_ARG, filtered_text), ("total", total_text)],
    }
}

/// Returns the count message and whether it represents an active restriction.
pub fn result_count_state(filtered: usize, total: usize) -> (Message, bool) {
    if filtered >= total {
        return (result_count(total, total), false);
    }
    (result_count(filtered, total), true)
}

fn message_with_one_arg(id: &'static str, name: &'static str, value: &str) -> Message {
    Message {
        id,
        plural: None,
        args: vec![(name, value.to_owned())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_labels_and_messages_preserve_the_existing_catalog_msgids() {
        assert_eq!(
            [
                FILTERS,
                ADD_FILTER,
                CLEAR_ALL,
                BACK,
                SEARCH_VALUES,
                NO_FILTERS_AVAILABLE,
                BROWSE_GENRE,
                BROWSE_ARTIST,
                BROWSE_ALBUM,
                BROWSE_YEAR,
                BROWSE_RATING,
                UNKNOWN_GENRE,
                UNKNOWN_ARTIST,
                UNKNOWN_ALBUM,
                UNKNOWN_YEAR,
                UNKNOWN_RATING,
            ],
            [
                "FILTER",
                "Add filter",
                "Clear all",
                "Back",
                "Search filter values",
                "All filters are active",
                "Genre",
                "Artist",
                "Album",
                "Year",
                "Rating",
                "Unknown genre",
                "Unknown artist",
                "Unknown album",
                "Unknown year",
                "Unrated",
            ]
        );
        assert_eq!(
            chip_label("Genre", "Metal"),
            Message {
                id: "{facet}: {value}",
                plural: None,
                args: vec![("facet", "Genre".to_owned()), ("value", "Metal".to_owned()),],
            }
        );
        assert_eq!(
            remove_filter_label("Genre", "Metal"),
            Message {
                id: "Remove {facet} filter: {value}",
                plural: None,
                args: vec![("facet", "Genre".to_owned()), ("value", "Metal".to_owned()),],
            }
        );
        assert_eq!(search_chip_label("falling").id, "⌕ “{query}” in any field");
        assert_eq!(remove_search_label("falling").id, "Remove search: {query}");
        assert_eq!(leave_place_label("Lorna Shore").id, "Leave {place}");
    }

    #[test]
    fn result_count_selects_the_catalog_pair_and_carries_restriction_state() {
        assert_eq!(
            result_count(1, 1),
            Message {
                id: "{total} track",
                plural: Some(Plural {
                    id: "{total} tracks",
                    count: 1,
                }),
                args: vec![("total", "1".to_owned())],
            }
        );
        assert_eq!(
            result_count(7, 96),
            Message {
                id: "{filtered} of {total} track",
                plural: Some(Plural {
                    id: "{filtered} of {total} tracks",
                    count: 96,
                }),
                args: vec![("filtered", "7".to_owned()), ("total", "96".to_owned()),],
            }
        );
        assert!(!result_count_state(96, 96).1);
        assert!(result_count_state(7, 96).1);
        assert_eq!(result_count_state(97, 96).0, result_count(96, 96));
    }
}
