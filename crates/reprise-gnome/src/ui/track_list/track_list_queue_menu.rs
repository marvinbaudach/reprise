use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;

use crate::ui::track_list::{show_toast, Shared};
use crate::ui::track_list_context_menu::current_selection_positions;
use crate::ui::{strings, track_actions};
use reprise_core::view_source::ViewSource;

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(in crate::ui) const ACTION_REMOVE_FROM_QUEUE: &str = "remove-from-queue";
const REMOVED_ONE: &str = N_!("Removed one track from Queue");
const REMOVED_MANY: &str = N_!("Removed {count} tracks from Queue");

pub(in crate::ui) fn add_remove_action(group: &gio::SimpleActionGroup, shared: &Rc<Shared>) {
    let action = gio::SimpleAction::new(ACTION_REMOVE_FROM_QUEUE, None);
    let shared = shared.clone();
    action.connect_activate(move |_, _| remove_selected(&shared));
    group.add_action(&action);
}

pub(in crate::ui) fn add_selected(shared: &Rc<Shared>, ids: &[i64]) {
    let Some(ids) = track_actions::queue_selected_ids(ids) else {
        return;
    };
    let count = ids.len();
    let callback = shared.on_queue_selected.borrow().clone();
    match callback {
        Some(callback) => {
            callback(ids);
            show_toast(shared, &strings::tracks_added_to_queue_toast(count));
        }
        None => tracing::warn!("add-to-queue fired without a callback"),
    }
}

/// Context-menu "Play next" (QUE-3): prepends the selection to the manual
/// line via `on_play_next_selected`. Same toast as "Add to queue" — both
/// land in the queue, only the position differs.
pub(in crate::ui) fn play_next_selected(shared: &Rc<Shared>, ids: &[i64]) {
    let Some(ids) = track_actions::queue_selected_ids(ids) else {
        return;
    };
    let count = ids.len();
    let callback = shared.on_play_next_selected.borrow().clone();
    match callback {
        Some(callback) => {
            callback(ids);
            show_toast(shared, &strings::tracks_added_to_queue_toast(count));
        }
        None => tracing::warn!("play-next fired without a callback"),
    }
}

/// Context-menu "Play" (PLAY-4b) when the current source is the Queue view:
/// rather than restarting playback from a synthesized `(ids, start_index)`
/// list, jump the existing queue directly to the clicked row via
/// `on_queue_activate` — the same handler `track_list_activation::
/// activate_track` uses for a Queue-view double-click. Returns `false`
/// (caller falls back to `handle_play`) for every other source.
pub(in crate::ui) fn play_position_if_queue(shared: &Rc<Shared>, position: u32) -> bool {
    if !matches!(*shared.source.borrow(), ViewSource::Queue) {
        return false;
    }
    let row = {
        let sections = shared.queue_sections.borrow();
        crate::ui::track_list::queue_row_mapping::classify(position, &sections)
    };
    let Some(row) = row else {
        tracing::warn!(
            position,
            "queue play action outside every section; ignoring"
        );
        return true;
    };
    let callback = shared.on_queue_activate.borrow().clone();
    match callback {
        Some(callback) => callback(row),
        None => tracing::warn!("queue play action fired without an activation callback"),
    }
    true
}

pub(in crate::ui) fn selected_rows(
    shared: &Rc<Shared>,
) -> Vec<crate::ui::track_list::queue_row_mapping::QueueRow> {
    if !matches!(*shared.source.borrow(), ViewSource::Queue) {
        return Vec::new();
    }
    let sections = shared.queue_sections.borrow();
    current_selection_positions(shared)
        .into_iter()
        .filter_map(|position| {
            crate::ui::track_list::queue_row_mapping::classify(position, &sections)
        })
        .collect()
}

pub(in crate::ui) fn remove_selected(shared: &Rc<Shared>) {
    if !matches!(*shared.source.borrow(), ViewSource::Queue) {
        tracing::warn!("remove-from-queue fired outside the Queue source");
        return;
    }
    let rows = selected_rows(shared);
    let callback = shared.on_queue_remove.borrow().clone();
    let removed = callback.map_or(0, |callback| callback(&rows));
    if removed == 0 {
        return;
    }
    let message = if removed == 1 {
        crate::i18n::gettext(REMOVED_ONE)
    } else {
        crate::i18n::format_message(
            &crate::i18n::gettext(REMOVED_MANY),
            &[("count", &removed.to_string())],
        )
    };
    show_toast(shared, &message);
}
