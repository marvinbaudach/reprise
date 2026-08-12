//! NAV-17: input seams for the track list's selection anchor.
//!
//! The cell gesture must use the **capture phase**. GTK's selection machinery
//! is attached to the `GtkListItemWidget`, an ancestor of the cell, and wins
//! during bubbling -- `rating.rs` documents how a plain `GestureClick` on the
//! field lost there. Capture runs completely before bubbling, so this handler
//! arrives first.
//!
//! Only Shift input is claimed. A click without Shift merely remembers the row
//! and continues, allowing GTK to select as usual. Observing rather than
//! claiming matters because Ctrl+click leaves a multi-row selection from which
//! the clicked row cannot be recovered afterward.

use std::rc::Rc;

use gtk4::prelude::*;

use super::track_list::Shared;
use super::track_list_selection_anchor::{
    anchored_at, live_anchor_state, playing_anchor, resolve, store_anchor_state, SelectMode,
    SelectionOp,
};

pub(super) fn pointer_mode(state: gtk4::gdk::ModifierType) -> Option<SelectMode> {
    if !state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
        // Without Shift, observe only and leave selection to GTK.
        return None;
    }
    Some(if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
        SelectMode::RangeAdditive
    } else {
        SelectMode::Range
    })
}

pub(super) fn apply(shared: &Shared, op: SelectionOp) {
    let selection = &shared.selection;
    match op {
        SelectionOp::SelectOnly(position) => {
            selection.select_item(position, true);
        }
        SelectionOp::Toggle(position) => {
            if selection.is_selected(position) {
                selection.unselect_item(position);
            } else {
                selection.select_item(position, false);
            }
        }
        SelectionOp::SelectRange {
            start,
            len,
            replace,
        } => {
            selection.select_range(start, len, replace);
            // Task 5's pointer scenario reads this proof from stderr. Only
            // real input can establish that this handler received the event.
            tracing::info!(start, len, replace, "selection anchor range applied");
        }
    }
}

/// Attaches the anchor gesture to a freshly set-up cell, at the same site and
/// with the same lifetime as `wire_context_menu_gesture`. `ListItem::position`
/// remains a stable row handle across rebinds.
pub(in crate::ui) fn wire_cell_selection(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
) {
    // input-parity: ACC-8 keyboard=nav_17_shift_arrow_extends_from_the_playing_row
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    // Run before GTK's selection machinery on the ancestor; see the module
    // documentation.
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);

    let item = item.clone();
    let shared = shared.clone();
    gesture.connect_pressed(move |gesture, _n_press, _x, _y| {
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            tracing::warn!("selection anchor: list item has no valid position; ignoring click");
            return;
        }
        let Some(target) = anchored_at(&shared, position) else {
            tracing::warn!(position, "selection anchor: no track at the clicked row");
            return;
        };
        let modifiers = gesture.current_event_state();
        let Some(mode) = pointer_mode(modifiers) else {
            // Observe a non-Shift click and let GTK handle selection. The first
            // press of a double-click does the same, matching the episode-row
            // pointer-intent discipline.
            store_anchor_state(
                &shared,
                super::track_list_selection_anchor::AnchorState {
                    anchor: Some(target),
                    cursor: Some(target),
                },
            );
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let (op, next) = resolve(
            live_anchor_state(&shared),
            playing_anchor(&shared),
            target,
            mode,
        );
        apply(&shared, op);
        store_anchor_state(&shared, next);
    });

    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk::ModifierType;

    #[test]
    fn nav_17_shift_claims_a_range_and_ctrl_shift_an_additive_one() {
        assert_eq!(
            pointer_mode(ModifierType::SHIFT_MASK),
            Some(SelectMode::Range)
        );
        assert_eq!(
            pointer_mode(ModifierType::SHIFT_MASK | ModifierType::CONTROL_MASK),
            Some(SelectMode::RangeAdditive)
        );
    }

    #[test]
    fn nav_17_input_without_shift_is_observed_not_claimed() {
        assert_eq!(pointer_mode(ModifierType::empty()), None);
        assert_eq!(pointer_mode(ModifierType::CONTROL_MASK), None);
        assert_eq!(pointer_mode(ModifierType::ALT_MASK), None);
    }
}
