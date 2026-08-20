//! NAV-17 selection-anchor wiring for the Releases table.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::releases_view::Shared;
use crate::ui::table_selection::{AnchorState, Anchored, KeyIntent, SelectMode, SelectionOp};

#[derive(Default)]
pub(super) struct ReleasesAnchor(RefCell<AnchorState<String>>);

impl ReleasesAnchor {
    pub(super) fn reconcile_after_replace(&self, model: &super::releases_model::ReleasesModel) {
        let state = { self.0.borrow().clone() };
        let remap = |held: Option<Anchored<String>>| {
            held.and_then(|held| {
                model.position_of(&held.id).map(|position| Anchored {
                    position,
                    id: held.id,
                })
            })
        };
        *self.0.borrow_mut() = AnchorState {
            anchor: remap(state.anchor),
            cursor: remap(state.cursor),
        };
    }
}

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

fn resolve_key_intent(
    state: AnchorState<String>,
    n_items: u32,
    lookup: impl Fn(u32) -> Option<String>,
    intent: KeyIntent,
    mode: SelectMode,
) -> Option<(SelectionOp, AnchorState<String>)> {
    if n_items == 0 {
        return None;
    }
    let state = crate::ui::table_selection::validate(state, &lookup);
    let origin = state.cursor.as_ref().or(state.anchor.as_ref())?;
    let position = match intent {
        KeyIntent::ExtendInPlace => origin.position,
        KeyIntent::Extend(step) => {
            let stepped = i64::from(origin.position) + i64::from(step);
            stepped.clamp(0, i64::from(n_items - 1)) as u32
        }
    };
    let target = Anchored {
        position,
        id: lookup(position)?,
    };
    Some(crate::ui::table_selection::resolve(
        state, None, target, mode,
    ))
}

pub(super) fn wire_cell(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
) {
    // input-parity: ACC-8 keyboard=nav_17_release_shift_arrow_extends_from_cursor
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

pub(super) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let keys = gtk4::EventControllerKey::new();
    // Run before GTK's own selection navigation, matching the track list.
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let shared_for_keys = shared.clone();
    let column_view_for_keys = column_view.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(intent) = crate::ui::table_selection::key_intent(key, modifiers) else {
            // Native Ctrl navigation/toggle and the context-menu controller
            // retain keys that do not express shared range selection.
            return gtk4::glib::Propagation::Proceed;
        };
        let mode = if modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            SelectMode::RangeAdditive
        } else {
            SelectMode::Range
        };
        let state = shared_for_keys.selection_anchor.0.borrow().clone();
        let Some((op, next)) = resolve_key_intent(
            state,
            shared_for_keys.model.store().n_items(),
            |position| anchored_at(&shared_for_keys, position).map(|row| row.id),
            intent,
            mode,
        ) else {
            // Releases has no playing-row fallback and never invents row zero.
            // Let GTK establish the first cursor; the selection listener below
            // then seeds the same anchor that a plain pointer click would.
            return gtk4::glib::Propagation::Proceed;
        };
        let position = next.cursor.as_ref().map(|row| row.position);
        *shared_for_keys.selection_anchor.0.borrow_mut() = next;
        apply(&shared_for_keys, op);
        if matches!(intent, KeyIntent::Extend(_)) {
            if let Some(position) = position {
                column_view_for_keys.scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
            }
        }
        gtk4::glib::Propagation::Stop
    });

    {
        let shared_for_sync = shared.clone();
        shared
            .model
            .selection()
            .connect_selection_changed(move |selection, _, _| {
                let selected = selection.selection();
                if selected.size() != 1 {
                    return;
                }
                let position = selected.nth(0);
                let Some(row) = anchored_at(&shared_for_sync, position) else {
                    return;
                };
                let state = shared_for_sync.selection_anchor.0.borrow().clone();
                if state.cursor.as_ref() == Some(&row) {
                    return;
                }
                *shared_for_sync.selection_anchor.0.borrow_mut() = AnchorState {
                    anchor: Some(row.clone()),
                    cursor: Some(row),
                };
            });
    }

    column_view.add_controller(keys);
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
    use crate::ui::table_selection::{key_intent, resolve, SelectMode};

    #[test]
    fn nav_17_release_shift_arrow_extends_from_cursor() {
        let ids = ["first".to_owned(), "anchor".to_owned(), "target".to_owned()];
        let state = AnchorState {
            anchor: Some(Anchored {
                position: 1,
                id: "anchor".to_owned(),
            }),
            cursor: Some(Anchored {
                position: 1,
                id: "anchor".to_owned(),
            }),
        };
        let intent = key_intent(gtk4::gdk::Key::Down, gtk4::gdk::ModifierType::SHIFT_MASK)
            .expect("Shift+Down is shared selection input");

        let (op, next) = resolve_key_intent(
            state,
            ids.len() as u32,
            |position| ids.get(position as usize).cloned(),
            intent,
            SelectMode::Range,
        )
        .expect("the cursor has a following release row");

        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 1,
                len: 2,
                replace: true,
            }
        );
        assert_eq!(
            next.anchor.as_ref().map(|row| row.id.as_str()),
            Some("anchor")
        );
        assert_eq!(
            next.cursor.as_ref().map(|row| row.id.as_str()),
            Some("target")
        );
    }

    #[test]
    fn nav_17_release_shift_arrows_clamp_at_both_list_boundaries() {
        let ids = ["first".to_owned(), "anchor".to_owned(), "last".to_owned()];
        let lookup = |position| ids.get(position as usize).cloned();
        let shift = gtk4::gdk::ModifierType::SHIFT_MASK;
        let at_first = AnchorState {
            anchor: Some(Anchored {
                position: 1,
                id: "anchor".to_owned(),
            }),
            cursor: Some(Anchored {
                position: 0,
                id: "first".to_owned(),
            }),
        };
        let at_last = AnchorState {
            anchor: Some(Anchored {
                position: 1,
                id: "anchor".to_owned(),
            }),
            cursor: Some(Anchored {
                position: 2,
                id: "last".to_owned(),
            }),
        };

        let (_, first_next) = resolve_key_intent(
            at_first,
            ids.len() as u32,
            lookup,
            key_intent(gtk4::gdk::Key::Up, shift).unwrap(),
            SelectMode::Range,
        )
        .expect("Up at the first row clamps to the first row");
        let (_, last_next) = resolve_key_intent(
            at_last,
            ids.len() as u32,
            lookup,
            key_intent(gtk4::gdk::Key::Down, shift).unwrap(),
            SelectMode::Range,
        )
        .expect("Down at the last row clamps to the last row");

        assert_eq!(first_next.cursor.unwrap().position, 0);
        assert_eq!(last_next.cursor.unwrap().position, 2);
    }

    #[test]
    fn nav_17_release_keyboard_drops_a_stale_anchor_before_resolving() {
        let ids = [
            "replacement".to_owned(),
            "cursor".to_owned(),
            "target".to_owned(),
        ];
        let state = AnchorState {
            anchor: Some(Anchored {
                position: 0,
                id: "stale-anchor".to_owned(),
            }),
            cursor: Some(Anchored {
                position: 1,
                id: "cursor".to_owned(),
            }),
        };
        let intent = key_intent(gtk4::gdk::Key::Down, gtk4::gdk::ModifierType::SHIFT_MASK).unwrap();

        let (op, next) = resolve_key_intent(
            state,
            ids.len() as u32,
            |position| ids.get(position as usize).cloned(),
            intent,
            SelectMode::Range,
        )
        .unwrap();

        assert_eq!(op, SelectionOp::SelectOnly(2));
        assert_eq!(
            next.anchor.as_ref().map(|row| row.id.as_str()),
            Some("target")
        );
        assert_eq!(
            next.cursor.as_ref().map(|row| row.id.as_str()),
            Some("target")
        );
    }

    #[test]
    fn nav_17_release_keyboard_without_a_cursor_defers_instead_of_using_row_zero() {
        let ids = ["first".to_owned(), "second".to_owned()];
        let intent = key_intent(gtk4::gdk::Key::Down, gtk4::gdk::ModifierType::SHIFT_MASK).unwrap();

        let resolution = resolve_key_intent(
            AnchorState::default(),
            ids.len() as u32,
            |position| ids.get(position as usize).cloned(),
            intent,
            SelectMode::Range,
        );

        assert_eq!(resolution, None);
    }

    #[test]
    fn release_selection_controller_leaves_native_and_context_keys_unclaimed() {
        use gtk4::gdk::{Key, ModifierType};

        assert_eq!(key_intent(Key::Down, ModifierType::CONTROL_MASK), None);
        assert_eq!(key_intent(Key::Up, ModifierType::CONTROL_MASK), None);
        assert_eq!(key_intent(Key::space, ModifierType::CONTROL_MASK), None);
        assert_eq!(key_intent(Key::Menu, ModifierType::empty()), None);
        assert_eq!(key_intent(Key::F10, ModifierType::SHIFT_MASK), None);
    }

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
