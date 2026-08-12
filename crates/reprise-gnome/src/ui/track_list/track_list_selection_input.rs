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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KeyIntent {
    /// Step relative to the cursor.
    Extend(i32),
    /// Shift+Space rebuilds the range without moving the cursor.
    ExtendInPlace,
}

pub(super) fn key_intent(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> Option<KeyIntent> {
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

pub(in crate::ui) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let keys = gtk4::EventControllerKey::new();
    // Run before GTK's own selection navigation.
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let shared_for_keys = shared.clone();
    let column_view_for_keys = column_view.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(intent) = key_intent(key, modifiers) else {
            return gtk4::glib::Propagation::Proceed;
        };
        let state = live_anchor_state(&shared_for_keys);
        let mode = if modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            SelectMode::RangeAdditive
        } else {
            SelectMode::Range
        };
        // Without a cursor, keyboard extension starts where pointer extension
        // would start too.
        let Some(origin) = state
            .cursor
            .or(state.anchor)
            .or_else(|| playing_anchor(&shared_for_keys))
        else {
            return gtk4::glib::Propagation::Proceed;
        };
        let n_items = shared_for_keys.model.n_items();
        if n_items == 0 {
            return gtk4::glib::Propagation::Proceed;
        }
        let position = match intent {
            KeyIntent::ExtendInPlace => origin.position,
            KeyIntent::Extend(step) => {
                let stepped = i64::from(origin.position) + i64::from(step);
                stepped.clamp(0, i64::from(n_items - 1)) as u32
            }
        };
        let Some(target) = anchored_at(&shared_for_keys, position) else {
            return gtk4::glib::Propagation::Proceed;
        };
        let (op, next) = resolve(state, playing_anchor(&shared_for_keys), target, mode);
        apply(&shared_for_keys, op);
        store_anchor_state(&shared_for_keys, next);
        if matches!(intent, KeyIntent::Extend(_)) {
            // Keyboard input should bring its target into view, unlike the
            // playback changes that NAV-10b keeps still.
            column_view_for_keys.scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
        }
        gtk4::glib::Propagation::Stop
    });

    {
        let shared_for_sync = shared.clone();
        shared
            .selection
            .connect_selection_changed(move |selection, _, _| {
                let mut only = None;
                for position in 0..selection.n_items() {
                    if selection.is_selected(position) {
                        if only.is_some() {
                            return;
                        }
                        only = Some(position);
                    }
                }
                let Some(position) = only else { return };
                let Some(anchored) = anchored_at(&shared_for_sync, position) else {
                    return;
                };
                let state = shared_for_sync.selection_anchor.get();
                if state.cursor == Some(anchored) {
                    return;
                }
                store_anchor_state(
                    &shared_for_sync,
                    super::track_list_selection_anchor::AnchorState {
                        anchor: Some(anchored),
                        cursor: Some(anchored),
                    },
                );
            });
    }

    column_view.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk::ModifierType;

    /// A presented window with `rows` synthetic tracks. The window is
    /// returned because it must remain alive for the duration of the test.
    fn display_fixture(rows: i64) -> (crate::ui::track_list::TrackList, gtk4::Window) {
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        for id in 1..=rows {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (
                    id,
                    format!("/synthetic/{id:03}.flac"),
                    format!("Track {id:03}"),
                ),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = crate::ui::track_list::TrackList::new(
            std::rc::Rc::new(conn),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let window = gtk4::Window::builder()
            .default_width(900)
            .default_height(320)
            .child(track_list.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        (track_list, window)
    }

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_single_row_selection_pulls_the_anchor_along() {
        // GTK moves focus and selection for unmodified arrows. The anchor must
        // follow afterward rather than remain behind.
        let (track_list, window) = display_fixture(40);
        let shared = &track_list.shared;
        store_anchor_state(shared, Default::default());

        shared.selection.select_item(6, true);
        while gtk4::glib::MainContext::default().iteration(false) {}

        let expected = anchored_at(shared, 6).unwrap();
        let state = live_anchor_state(shared);
        assert_eq!(
            state.anchor,
            Some(expected),
            "the anchor follows a single-row selection"
        );
        assert_eq!(state.cursor, Some(expected));

        // A multi-row selection must not move it; otherwise Ctrl+click would
        // lose the identity of the clicked row.
        shared.selection.select_range(10, 3, true);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert_eq!(
            live_anchor_state(shared).anchor,
            Some(expected),
            "a multi-row selection leaves the anchor fixed"
        );

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_shift_arrow_extends_from_the_playing_row() {
        let (track_list, window) = display_fixture(40);
        let shared = &track_list.shared;
        // There is no user anchor, but row 10 is playing.
        let playing = shared.model.track_at(10).unwrap().id;
        shared.playing_track_id.set(Some(playing));

        let target = anchored_at(shared, 12).unwrap();
        let (op, next) = resolve(
            live_anchor_state(shared),
            playing_anchor(shared),
            target,
            SelectMode::Range,
        );
        apply(shared, op);
        store_anchor_state(shared, next);

        for position in 10..=12 {
            assert!(
                shared.selection.is_selected(position),
                "row {position} must be part of the range"
            );
        }
        assert!(
            !shared.selection.is_selected(9),
            "the range starts at the playing track"
        );
        assert!(!shared.selection.is_selected(13));

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_range_does_not_move_the_viewport() {
        // NAV-10b: selection moves nothing. The pointer path deliberately
        // avoids `scroll_to`; only the keyboard path uses it.
        let (track_list, window) = display_fixture(200);
        let shared = &track_list.shared;
        shared
            .column_view
            .scroll_to(120, None, gtk4::ListScrollFlags::FOCUS, None);
        let adjustment = shared.column_view.vadjustment().unwrap();
        // `scroll_to` settles over later main-loop turns. This pumps test setup,
        // not the behavior under test.
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.value() > 0.0
        });
        let before = adjustment.value();
        assert!(
            before > 0.0,
            "precondition: the list must be scrolled away from the start"
        );

        apply(
            shared,
            SelectionOp::SelectRange {
                start: 3,
                len: 9,
                replace: true,
            },
        );
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(
            (adjustment.value() - before).abs() < 1.0,
            "a range above the viewport must not move the viewport there"
        );

        window.close();
    }
}
