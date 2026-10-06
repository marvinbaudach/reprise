#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioEmptyState {
    List,
    NoResults,
    Empty,
    ModuleOff,
}

pub(super) fn radio_empty_state_for(
    visible_count: usize,
    filter_active: bool,
    module_enabled: bool,
) -> RadioEmptyState {
    if visible_count > 0 {
        RadioEmptyState::List
    } else if filter_active {
        RadioEmptyState::NoResults
    } else if !module_enabled {
        RadioEmptyState::ModuleOff
    } else {
        RadioEmptyState::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_empty_state_always_has_one_deterministic_next_step() {
        assert_eq!(radio_empty_state_for(3, false, true), RadioEmptyState::List);
        assert_eq!(
            radio_empty_state_for(0, true, true),
            RadioEmptyState::NoResults
        );
        assert_eq!(
            radio_empty_state_for(0, false, true),
            RadioEmptyState::Empty
        );
    }

    #[test]
    fn src_10a_a_switched_off_radio_module_with_no_stations_decides_module_off() {
        assert_eq!(
            radio_empty_state_for(0, false, false),
            RadioEmptyState::ModuleOff
        );
        assert_eq!(
            radio_empty_state_for(1, false, false),
            RadioEmptyState::List,
            "existing stations remain available when the module is off"
        );
    }
}
