#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConcertsEmptyState {
    List,
    NoCredentials,
    NeverFetched,
    NoResults,
    Empty,
}

pub(super) fn concerts_empty_state_for(
    row_count: usize,
    has_filter: bool,
    has_credentials: bool,
    never_fetched: bool,
) -> ConcertsEmptyState {
    if row_count > 0 {
        ConcertsEmptyState::List
    } else if !has_credentials {
        ConcertsEmptyState::NoCredentials
    } else if never_fetched {
        ConcertsEmptyState::NeverFetched
    } else if has_filter {
        ConcertsEmptyState::NoResults
    } else {
        ConcertsEmptyState::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conc_4_empty_state_matrix_has_one_deterministic_next_step() {
        assert_eq!(
            concerts_empty_state_for(1, false, false, true),
            ConcertsEmptyState::List
        );
        assert_eq!(
            concerts_empty_state_for(0, false, false, true),
            ConcertsEmptyState::NoCredentials
        );
        assert_eq!(
            concerts_empty_state_for(0, false, true, true),
            ConcertsEmptyState::NeverFetched
        );
        assert_eq!(
            concerts_empty_state_for(0, true, true, false),
            ConcertsEmptyState::NoResults
        );
        assert_eq!(
            concerts_empty_state_for(0, false, true, false),
            ConcertsEmptyState::Empty
        );
    }
}
