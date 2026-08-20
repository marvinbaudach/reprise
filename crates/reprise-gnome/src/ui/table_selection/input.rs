pub(in crate::ui) fn pointer_mode(state: gtk4::gdk::ModifierType) -> Option<super::SelectMode> {
    if !state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
        // Without Shift, observe only and leave selection to GTK.
        return None;
    }
    Some(if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
        super::SelectMode::RangeAdditive
    } else {
        super::SelectMode::Range
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum KeyIntent {
    /// Step relative to the cursor.
    Extend(i32),
    /// Shift+Space rebuilds the range without moving the cursor.
    ExtendInPlace,
}

pub(in crate::ui) fn key_intent(
    key: gtk4::gdk::Key,
    state: gtk4::gdk::ModifierType,
) -> Option<KeyIntent> {
    if !state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
        return None;
    }
    // Alt+arrow belongs to row reordering, not selection.
    if state.contains(gtk4::gdk::ModifierType::ALT_MASK) {
        return None;
    }
    match key {
        gtk4::gdk::Key::Down | gtk4::gdk::Key::KP_Down => Some(KeyIntent::Extend(1)),
        gtk4::gdk::Key::Up | gtk4::gdk::Key::KP_Up => Some(KeyIntent::Extend(-1)),
        gtk4::gdk::Key::space | gtk4::gdk::Key::KP_Space => Some(KeyIntent::ExtendInPlace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk::ModifierType;

    #[test]
    fn nav_17_shift_claims_a_range_and_ctrl_shift_an_additive_one() {
        assert_eq!(
            pointer_mode(ModifierType::SHIFT_MASK),
            Some(super::super::SelectMode::Range)
        );
        assert_eq!(
            pointer_mode(ModifierType::SHIFT_MASK | ModifierType::CONTROL_MASK),
            Some(super::super::SelectMode::RangeAdditive)
        );
    }

    #[test]
    fn nav_17_input_without_shift_is_observed_not_claimed() {
        assert_eq!(pointer_mode(ModifierType::empty()), None);
        assert_eq!(pointer_mode(ModifierType::CONTROL_MASK), None);
        assert_eq!(pointer_mode(ModifierType::ALT_MASK), None);
    }

    #[test]
    fn nav_17_shift_arrows_step_and_shift_space_stays() {
        use gtk4::gdk::Key;
        assert_eq!(
            key_intent(Key::Down, ModifierType::SHIFT_MASK),
            Some(KeyIntent::Extend(1))
        );
        assert_eq!(
            key_intent(Key::Up, ModifierType::SHIFT_MASK),
            Some(KeyIntent::Extend(-1))
        );
        assert_eq!(
            key_intent(Key::space, ModifierType::SHIFT_MASK),
            Some(KeyIntent::ExtendInPlace)
        );
    }

    #[test]
    fn nav_17_arrows_without_shift_stay_with_gtk() {
        use gtk4::gdk::Key;
        assert_eq!(key_intent(Key::Down, ModifierType::empty()), None);
        assert_eq!(key_intent(Key::space, ModifierType::empty()), None);
        // Alt+arrow belongs to the reorder controller.
        assert_eq!(key_intent(Key::Down, ModifierType::ALT_MASK), None);
        assert_eq!(
            key_intent(Key::Down, ModifierType::SHIFT_MASK | ModifierType::ALT_MASK),
            None
        );
    }
}
