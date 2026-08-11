#![allow(dead_code)]

use reprise_core::artist_news::ReleasesFilter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReleasesEmptyState {
    List,
    NeverFetched,
    NoResults,
    Empty,
}

pub(super) fn releases_empty_state_for(
    row_count: usize,
    has_filter: bool,
    never_fetched: bool,
) -> ReleasesEmptyState {
    if row_count > 0 {
        ReleasesEmptyState::List
    } else if never_fetched {
        ReleasesEmptyState::NeverFetched
    } else if has_filter {
        ReleasesEmptyState::NoResults
    } else {
        ReleasesEmptyState::Empty
    }
}

pub(super) fn releases_scope_is_filtered(filter: &ReleasesFilter, query: &str) -> bool {
    !filter.is_widest() || !query.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fil_6_releases_empty_state_matrix_has_one_next_step() {
        assert_eq!(
            releases_empty_state_for(1, false, true),
            ReleasesEmptyState::List
        );
        assert_eq!(
            releases_empty_state_for(0, false, true),
            ReleasesEmptyState::NeverFetched
        );
        assert_eq!(
            releases_empty_state_for(0, true, false),
            ReleasesEmptyState::NoResults
        );
        assert_eq!(
            releases_empty_state_for(0, false, false),
            ReleasesEmptyState::Empty
        );
    }

    #[test]
    fn nr_31_gaps_beyond_the_window_offer_show_all() {
        let filter = ReleasesFilter::default();
        assert!(releases_scope_is_filtered(&filter, ""));
        assert_eq!(
            releases_empty_state_for(0, true, false),
            ReleasesEmptyState::NoResults
        );
    }
}
