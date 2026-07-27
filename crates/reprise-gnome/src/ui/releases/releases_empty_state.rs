#![allow(dead_code)]

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_14_releases_empty_state_matrix_has_one_next_step() {
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
}
