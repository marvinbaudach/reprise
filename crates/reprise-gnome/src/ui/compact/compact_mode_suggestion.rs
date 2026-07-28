//! Once-per-session Compact Mode suggestion for constrained Library windows.

use std::cell::Cell;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use super::minimal_view::MinimalView;

const SUGGEST_MAX_WIDTH: i32 = 680;
const SUGGEST_MAX_HEIGHT: i32 = 480;

fn take_suggestion(shown: &Cell<bool>, library_mode: bool, compact_available: bool) -> bool {
    library_mode && compact_available && !shown.replace(true)
}

pub(in crate::ui) fn install(
    window: &adw::ApplicationWindow,
    overlay: &adw::ToastOverlay,
    mode: &Rc<MinimalView>,
    compact_available: bool,
) {
    let narrow = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(SUGGEST_MAX_WIDTH),
        adw::LengthUnit::Px,
    );
    let short = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxHeight,
        f64::from(SUGGEST_MAX_HEIGHT),
        adw::LengthUnit::Px,
    );
    let constrained = adw::BreakpointCondition::new_or(narrow, short);
    let breakpoint = adw::Breakpoint::new(constrained);
    let shown = Cell::new(false);
    let overlay = overlay.downgrade();
    let mode = Rc::downgrade(mode);
    breakpoint.connect_apply(move |_| {
        let Some(mode) = mode.upgrade() else {
            return;
        };
        if !take_suggestion(&shown, mode.is_library_mode(), compact_available) {
            return;
        }
        let Some(overlay) = overlay.upgrade() else {
            return;
        };
        let toast = adw::Toast::new(&crate::ui::strings::text(
            crate::ui::strings::COMPACT_MODE_SUGGESTION,
        ));
        toast.set_button_label(Some(&crate::ui::strings::text(
            crate::ui::strings::USE_COMPACT_MODE,
        )));
        toast.set_action_name(Some("win.toggle-minimal-view"));
        overlay.add_toast(toast);
    });
    window.add_breakpoint(breakpoint);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_5_suggestion_is_explicit_available_and_once_per_session() {
        let shown = Cell::new(false);

        assert!(!take_suggestion(&shown, true, false));
        assert!(
            !shown.get(),
            "an unavailable compact player must not consume the prompt"
        );
        assert!(!take_suggestion(&shown, false, true));
        assert!(
            !shown.get(),
            "Compact Mode must not prompt while already active"
        );
        assert!(take_suggestion(&shown, true, true));
        assert!(shown.get());
        assert!(!take_suggestion(&shown, true, true));
    }
}
