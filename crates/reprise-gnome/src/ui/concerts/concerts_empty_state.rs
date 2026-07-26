use crate::ui::strings;

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

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ConcertsEmptyStatePresentation {
    pub icon: &'static str,
    pub title: String,
    pub description: String,
    pub action: Option<String>,
}

pub(super) fn concerts_empty_state_presentation(
    state: ConcertsEmptyState,
    total: usize,
) -> ConcertsEmptyStatePresentation {
    let (icon, title, description, action) = match state {
        ConcertsEmptyState::NoCredentials => (
            "x-office-calendar-symbolic",
            strings::text(strings::CONCERTS_NO_DATA_TITLE),
            String::new(),
            None,
        ),
        ConcertsEmptyState::NeverFetched => (
            "x-office-calendar-symbolic",
            strings::text(strings::CONCERTS_NO_DATA_TITLE),
            String::new(),
            Some(strings::text(strings::FETCH_NOW)),
        ),
        ConcertsEmptyState::NoResults => (
            "system-search-symbolic",
            strings::text(strings::NO_RESULTS_TITLE),
            strings::text(strings::NO_RESULTS_DESCRIPTION),
            Some(strings::show_all_concerts(total)),
        ),
        ConcertsEmptyState::Empty => (
            "emblem-ok-symbolic",
            strings::text(strings::CONCERTS_NO_UPCOMING_TITLE),
            String::new(),
            Some(strings::text(strings::FETCH_NOW)),
        ),
        ConcertsEmptyState::List => unreachable!("list state has no status presentation"),
    };
    ConcertsEmptyStatePresentation {
        icon,
        title,
        description,
        action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conc_4b_empty_state_matrix_has_one_deterministic_next_step() {
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

    #[test]
    fn conc_4b_missing_credentials_have_no_key_entry_prompt() {
        let presentation = concerts_empty_state_presentation(ConcertsEmptyState::NoCredentials, 0);

        assert_eq!(
            presentation.title,
            strings::text(strings::CONCERTS_NO_DATA_TITLE)
        );
        assert!(presentation.description.is_empty());
        assert_eq!(presentation.action, None);
    }
}
