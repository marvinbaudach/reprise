//! Pure seek-guard decisions shared by the player bar's GTK callbacks.

pub(super) fn should_apply_position_tick(dragging: bool) -> bool {
    !dragging
}

pub(super) fn should_clear_drag_guard_on_track_change(
    pointer_down: bool,
    gesture_active: bool,
) -> bool {
    !pointer_down && !gesture_active
}

pub(super) fn should_self_heal(dragging: bool, pointer_down: bool, gesture_active: bool) -> bool {
    dragging && !pointer_down && !gesture_active
}

pub(super) fn should_finish_observer_stop(pointer_down: bool, gesture_active: bool) -> bool {
    !pointer_down && !gesture_active
}

pub(super) fn should_finish_observer_cancel(pointer_down: bool) -> bool {
    !pointer_down
}

pub(super) fn should_update_range(last_duration_ms: i64, duration_ms: i64) -> bool {
    duration_ms != last_duration_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_tick_applies_when_not_dragging() {
        assert!(should_apply_position_tick(false));
    }

    #[test]
    fn position_tick_is_suppressed_while_dragging() {
        assert!(!should_apply_position_tick(true));
    }

    #[test]
    fn self_heal_waits_for_raw_pointer_release() {
        assert!(should_self_heal(true, false, false));
        // GtkRange can deny the observing GestureClick while the physical
        // pointer remains down. That is still a genuine drag.
        assert!(!should_self_heal(true, true, false));
        assert!(!should_self_heal(true, false, true));
        assert!(!should_self_heal(false, false, false));
    }

    #[test]
    fn track_change_keeps_guard_for_either_live_observer() {
        assert!(!should_clear_drag_guard_on_track_change(true, false));
        assert!(!should_clear_drag_guard_on_track_change(false, true));
        assert!(should_clear_drag_guard_on_track_change(false, false));
    }

    #[test]
    fn observer_stop_cannot_end_a_physical_drag() {
        assert!(!should_finish_observer_stop(true, false));
        assert!(!should_finish_observer_stop(false, true));
        assert!(should_finish_observer_stop(false, false));
    }

    #[test]
    fn observer_cancel_cannot_end_a_physical_drag() {
        assert!(!should_finish_observer_cancel(true));
        assert!(should_finish_observer_cancel(false));
    }

    #[test]
    fn range_updates_when_duration_changes() {
        assert!(should_update_range(0, 180_000));
        assert!(should_update_range(180_000, 0));
    }

    #[test]
    fn range_update_is_skipped_when_duration_is_unchanged() {
        assert!(!should_update_range(180_000, 180_000));
        assert!(!should_update_range(0, 0));
    }
}
