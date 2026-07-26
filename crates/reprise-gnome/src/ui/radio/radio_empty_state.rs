#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioEmptyState {
    List,
    NoResults,
    Empty,
}

pub(super) fn radio_empty_state_for(visible_count: usize, filter_active: bool) -> RadioEmptyState {
    if visible_count > 0 {
        RadioEmptyState::List
    } else if filter_active {
        RadioEmptyState::NoResults
    } else {
        RadioEmptyState::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_empty_state_always_has_one_deterministic_next_step() {
        assert_eq!(radio_empty_state_for(3, false), RadioEmptyState::List);
        assert_eq!(radio_empty_state_for(0, true), RadioEmptyState::NoResults);
        assert_eq!(radio_empty_state_for(0, false), RadioEmptyState::Empty);
    }
}
