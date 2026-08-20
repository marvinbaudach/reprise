//! NAV-17 selection-anchor wiring for the Releases table.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::releases_view::Shared;
use crate::ui::table_selection::{AnchorState, Anchored, SelectionOp};

#[derive(Default)]
pub(super) struct ReleasesAnchor(RefCell<AnchorState<String>>);

fn anchored_at(shared: &Shared, position: u32) -> Option<Anchored<String>> {
    shared
        .model
        .store()
        .item(position)
        .and_downcast::<super::releases_model::ReleaseObject>()
        .map(|object| Anchored {
            position,
            id: object.entry().release_group_mbid,
        })
}

fn validated_state(shared: &Shared) -> AnchorState<String> {
    let state = shared.selection_anchor.0.borrow().clone();
    let state = crate::ui::table_selection::validate(state, |position| {
        anchored_at(shared, position).map(|anchored| anchored.id)
    });
    *shared.selection_anchor.0.borrow_mut() = state.clone();
    state
}

pub(super) fn wire_cell(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let item = item.clone();
    let shared = shared.clone();
    gesture.connect_pressed(move |gesture, _, _, _| {
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let Some(target) = anchored_at(&shared, position) else {
            return;
        };
        let Some(mode) = crate::ui::table_selection::pointer_mode(gesture.current_event_state())
        else {
            *shared.selection_anchor.0.borrow_mut() = AnchorState {
                anchor: Some(target.clone()),
                cursor: Some(target),
            };
            return;
        };
        let state = validated_state(&shared);
        let (op, next) = crate::ui::table_selection::resolve(state, None, target, mode);
        *shared.selection_anchor.0.borrow_mut() = next;
        apply(&shared, op);
        gesture.set_state(gtk4::EventSequenceState::Claimed);
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

pub(super) fn apply(shared: &Rc<Shared>, op: SelectionOp) {
    match op {
        SelectionOp::SelectOnly(position) => {
            shared.model.selection().select_range(position, 1, true);
        }
        SelectionOp::Toggle(position) => {
            if shared.model.selection().is_selected(position) {
                shared.model.selection().unselect_item(position);
            } else {
                shared.model.selection().select_item(position, false);
            }
        }
        SelectionOp::SelectRange {
            start,
            len,
            replace,
        } => {
            shared.model.selection().select_range(start, len, replace);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::table_selection::{resolve, SelectMode};

    #[test]
    fn nav_17_a_release_range_starts_at_the_anchor_and_never_moves_it() {
        let state = AnchorState {
            anchor: Some(Anchored {
                position: 4,
                id: "anchor".to_owned(),
            }),
            cursor: Some(Anchored {
                position: 4,
                id: "anchor".to_owned(),
            }),
        };
        let target = Anchored {
            position: 1,
            id: "target".to_owned(),
        };

        let (op, next) = resolve(state, None, target, SelectMode::Range);

        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 1,
                len: 4,
                replace: true
            }
        );
        assert_eq!(
            next.anchor.map(|anchored| anchored.position),
            Some(4),
            "a range never moves the anchor"
        );
    }

    #[test]
    fn nav_17_a_release_range_without_an_anchor_takes_only_the_clicked_row() {
        let state = AnchorState {
            anchor: None,
            cursor: None,
        };
        let target = Anchored {
            position: 2,
            id: "target".to_owned(),
        };

        let (op, _) = resolve(state, None, target, SelectMode::Range);

        assert_eq!(
            op,
            SelectionOp::SelectOnly(2),
            "releases have no playing row to fall back on"
        );
    }
}
