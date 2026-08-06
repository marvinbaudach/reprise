//! Sensitivity policy for the shared track-list context actions.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;

use super::track_list_context_menu::{
    current_selection_positions, ACTION_ADD_TO_QUEUE, ACTION_GO_TO_ALBUM, ACTION_GO_TO_ARTIST,
    ACTION_MOVE_DOWN, ACTION_MOVE_TO_TOP, ACTION_MOVE_UP, ACTION_PLAY_NEXT, ACTION_SHOW_IN_FILES,
};
use super::track_list_queue_menu::{self, ACTION_REMOVE_FROM_QUEUE};
use super::track_menu::{action_states, MenuContext, SelectionSummary};
use super::Shared;
use crate::ui::tag_edit_flow;

/// Greys out the menu actions the current selection cannot support.
pub(super) fn update(
    shared: &Rc<Shared>,
    context: MenuContext,
    summary: &SelectionSummary,
    playable_enqueue_enabled: bool,
) {
    let states = action_states(context, summary);
    let queue_items = current_selection_positions(shared)
        .into_iter()
        .filter_map(|position| shared.model.queue_item_at(position))
        .map(|metadata| metadata.item())
        .collect::<Vec<_>>();
    let enqueue_enabled =
        playable_enqueue_enabled && super::queue_item_menu::enqueue_enabled(&queue_items);
    let move_up = super::track_list_keyboard_reorder::is_available(
        shared,
        super::track_list_keyboard_reorder::ReorderDirection::Up,
    );
    let move_down = super::track_list_keyboard_reorder::is_available(
        shared,
        super::track_list_keyboard_reorder::ReorderDirection::Down,
    );
    let move_to_top = super::track_list_keyboard_reorder::is_available(
        shared,
        super::track_list_keyboard_reorder::ReorderDirection::Top,
    ) || (context == MenuContext::Queue
        && track_list_queue_menu::selected_rows(shared).len() > 1);
    let queue_projection_editable = context != MenuContext::Queue
        || !track_list_queue_menu::selection_has_read_only_episode_projection(shared);
    for (name, enabled) in [
        (ACTION_PLAY_NEXT, enqueue_enabled),
        (ACTION_ADD_TO_QUEUE, enqueue_enabled),
        (ACTION_MOVE_UP, move_up),
        (ACTION_MOVE_DOWN, move_down),
        (ACTION_MOVE_TO_TOP, move_to_top),
        (ACTION_REMOVE_FROM_QUEUE, queue_projection_editable),
        (ACTION_GO_TO_ALBUM, states.go_to_album),
        (ACTION_GO_TO_ARTIST, states.go_to_artist),
        (ACTION_SHOW_IN_FILES, states.show_in_files),
        ("trash-selected-tracks", states.trash),
        (tag_edit_flow::ACTION_EDIT_TAGS, states.edit_tags),
    ] {
        let Some(action) = shared.menu_actions.lookup_action(name) else {
            continue;
        };
        if let Ok(action) = action.downcast::<gio::SimpleAction>() {
            action.set_enabled(enabled);
        }
    }
}
