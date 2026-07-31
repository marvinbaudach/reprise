//! Keyboard entry points for the exact playlist/queue reorder paths used by
//! drag and drop.

use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use reprise_core::up_next::QueueItem;
use reprise_core::view_source::ViewSource;

use super::track_list_context_menu::{current_selection_ids, current_selection_positions};
use super::track_list_dnd::{self, DragPayload};
use super::Shared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ReorderDirection {
    Up,
    Down,
    Top,
}

fn keyboard_reorder_target(from: u32, item_count: u32, direction: ReorderDirection) -> Option<u32> {
    let target = match direction {
        ReorderDirection::Up => from.checked_sub(1)?,
        ReorderDirection::Down => from.checked_add(1).filter(|target| *target < item_count)?,
        ReorderDirection::Top if from > 0 => 0,
        ReorderDirection::Top => return None,
    };
    Some(target)
}

fn selected_move(shared: &Rc<Shared>, direction: ReorderDirection) -> Option<(u32, u32)> {
    let positions = current_selection_positions(shared);
    let &[from] = positions.as_slice() else {
        return None;
    };
    let target = keyboard_reorder_target(from, shared.model.n_items(), direction)?;
    Some((from, target))
}

fn keyboard_queue_op(
    from: u32,
    item_count: u32,
    direction: ReorderDirection,
    sections: &[super::queue_sections::QueueSection],
) -> Option<super::queue_row_mapping::QueueReorderOp> {
    let target = keyboard_reorder_target(from, item_count, direction)?;
    super::queue_row_mapping::reorder_op(from, target, sections)
}

pub(in crate::ui) fn is_available(shared: &Rc<Shared>, direction: ReorderDirection) -> bool {
    let Some((from, target)) = selected_move(shared, direction) else {
        return false;
    };
    let source = shared.source.borrow().clone();
    match source {
        ViewSource::Playlist(_) => {
            if !super::playlist_reorder_allowed(shared) {
                return false;
            }
            let Some(source_position) = shared
                .model
                .track_at(from)
                .and_then(|track| track.playlist_position)
            else {
                return false;
            };
            let Some(target_position) = shared
                .model
                .track_at(target)
                .and_then(|track| track.playlist_position)
            else {
                return false;
            };
            let payload = DragPayload {
                items: current_selection_ids(shared)
                    .into_iter()
                    .map(QueueItem::Track)
                    .collect(),
                reorder_position: Some(source_position),
            };
            track_list_dnd::resolve_reorder_target(&payload, target_position).is_some()
        }
        ViewSource::Queue => {
            let sections = shared.queue_sections.borrow();
            keyboard_queue_op(from, shared.model.n_items(), direction, &sections).is_some()
        }
        _ => false,
    }
}

pub(in crate::ui) fn perform(shared: &Rc<Shared>, direction: ReorderDirection) -> bool {
    let Some((from, target)) = selected_move(shared, direction) else {
        return false;
    };
    let source = shared.source.borrow().clone();
    match source {
        ViewSource::Playlist(playlist_id) => {
            let Some(source_position) = shared
                .model
                .track_at(from)
                .and_then(|track| track.playlist_position)
            else {
                return false;
            };
            let payload = DragPayload {
                items: current_selection_ids(shared)
                    .into_iter()
                    .map(QueueItem::Track)
                    .collect(),
                reorder_position: Some(source_position),
            };
            track_list_dnd::handle_playlist_reorder_drop(shared, playlist_id, &payload, target)
        }
        ViewSource::Queue => {
            let op = {
                let sections = shared.queue_sections.borrow();
                keyboard_queue_op(from, shared.model.n_items(), direction, &sections)
            };
            let Some(op) = op else {
                return false;
            };
            let callback = shared.on_queue_reorder.borrow().clone();
            let moved = callback.is_some_and(|callback| callback(op));
            if moved {
                super::reload(shared);
            }
            moved
        }
        _ => false,
    }
}

fn keyboard_direction(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<ReorderDirection> {
    match (key, modifiers) {
        (gdk::Key::Up, modifiers) if modifiers == gdk::ModifierType::ALT_MASK => {
            Some(ReorderDirection::Up)
        }
        (gdk::Key::Down, modifiers) if modifiers == gdk::ModifierType::ALT_MASK => {
            Some(ReorderDirection::Down)
        }
        _ => None,
    }
}

pub(in crate::ui) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    column_view.update_property(&[gtk4::accessible::Property::KeyShortcuts(
        "Alt+ArrowUp Alt+ArrowDown",
    )]);
    let keys = gtk4::EventControllerKey::new();
    let shared = shared.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(direction) = keyboard_direction(key, modifiers) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if perform(&shared, direction) {
            gtk4::glib::Propagation::Stop
        } else {
            gtk4::glib::Propagation::Proceed
        }
    });
    column_view.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::up_next::QueueItem;

    #[test]
    fn keyboard_targets_respect_bounds_and_top() {
        assert_eq!(keyboard_reorder_target(2, 5, ReorderDirection::Up), Some(1));
        assert_eq!(
            keyboard_reorder_target(2, 5, ReorderDirection::Down),
            Some(3)
        );
        assert_eq!(
            keyboard_reorder_target(2, 5, ReorderDirection::Top),
            Some(0)
        );
        assert_eq!(keyboard_reorder_target(0, 5, ReorderDirection::Up), None);
        assert_eq!(keyboard_reorder_target(4, 5, ReorderDirection::Down), None);
        assert_eq!(keyboard_reorder_target(0, 5, ReorderDirection::Top), None);
    }

    #[test]
    fn only_alt_arrows_request_a_direct_keyboard_reorder() {
        assert_eq!(
            keyboard_direction(gdk::Key::Up, gdk::ModifierType::ALT_MASK),
            Some(ReorderDirection::Up)
        );
        assert_eq!(
            keyboard_direction(gdk::Key::Down, gdk::ModifierType::ALT_MASK),
            Some(ReorderDirection::Down)
        );
        assert_eq!(
            keyboard_direction(gdk::Key::Up, gdk::ModifierType::empty()),
            None
        );
        assert_eq!(
            keyboard_direction(gdk::Key::Down, gdk::ModifierType::SHIFT_MASK),
            None
        );
    }

    #[test]
    fn queue_keyboard_move_is_the_same_operation_as_a_drop() {
        let sections = super::super::queue_sections::compose(
            Some(QueueItem::Track(1)),
            &[QueueItem::Track(2), QueueItem::Track(3)],
            &[4, 5, 6],
            Some("Music"),
        )
        .sections;
        let keyboard = keyboard_queue_op(2, 6, ReorderDirection::Up, &sections);
        let dropped = super::super::queue_row_mapping::reorder_op(2, 1, &sections);
        assert_eq!(keyboard, dropped);
        assert_eq!(
            keyboard,
            Some(
                super::super::queue_row_mapping::QueueReorderOp::WithinPlayNext { from: 1, to: 0 }
            )
        );
    }

    #[test]
    fn episode_queue_keyboard_move_uses_positions_without_a_track_id_payload() {
        let sections = super::super::queue_sections::compose(
            Some(QueueItem::Track(1)),
            &[QueueItem::Episode(7), QueueItem::Track(8)],
            &[],
            None,
        )
        .sections;

        assert_eq!(
            keyboard_queue_op(1, 3, ReorderDirection::Down, &sections),
            super::super::queue_row_mapping::reorder_op(1, 2, &sections)
        );
    }
}
