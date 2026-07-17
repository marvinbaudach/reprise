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
const REMOVE_FROM_QUEUE: &str = N_!("Remove from Queue");
const REMOVED_ONE: &str = N_!("Removed one track from Queue");
const REMOVED_MANY: &str = N_!("Removed {count} tracks from Queue");

fn primary_action_name(source: &ViewSource) -> &'static str {
    if matches!(source, ViewSource::Queue) {
        ACTION_REMOVE_FROM_QUEUE
    } else {
        "add-to-queue"
    }
}

pub(in crate::ui) fn append_queue_primary_action(
    primary: &gio::Menu,
    shared: &Rc<Shared>,
    group: &str,
    add_action: &str,
    add_label: &str,
) {
    let action = primary_action_name(&shared.source.borrow());
    if action == ACTION_REMOVE_FROM_QUEUE {
        primary.append(
            Some(&crate::i18n::gettext(REMOVE_FROM_QUEUE)),
            Some(&format!("{group}.{ACTION_REMOVE_FROM_QUEUE}")),
        );
    } else {
        debug_assert_eq!(action, add_action);
        primary.append(Some(add_label), Some(&format!("{group}.{action}")));
    }
}

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

pub(in crate::ui) fn play_selected_if_queue(shared: &Rc<Shared>) -> bool {
    if !matches!(*shared.source.borrow(), ViewSource::Queue) {
        return false;
    }
    let Some(position) = current_selection_positions(shared).first().copied() else {
        return true;
    };
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

pub(in crate::ui) fn remove_selected(shared: &Rc<Shared>) {
    if !matches!(*shared.source.borrow(), ViewSource::Queue) {
        tracing::warn!("remove-from-queue fired outside the Queue source");
        return;
    }
    let rows: Vec<_> = {
        let sections = shared.queue_sections.borrow();
        current_selection_positions(shared)
            .into_iter()
            .filter_map(|position| {
                crate::ui::track_list::queue_row_mapping::classify(position, &sections)
            })
            .collect()
    };
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

#[cfg(test)]
mod tests {
    use super::primary_action_name;
    use reprise_core::view_source::ViewSource;

    #[test]
    fn queue_replaces_add_with_remove_in_the_primary_menu() {
        assert_eq!(primary_action_name(&ViewSource::Queue), "remove-from-queue");
        assert_eq!(primary_action_name(&ViewSource::Library), "add-to-queue");
        assert_eq!(
            primary_action_name(&ViewSource::Playlist(7)),
            "add-to-queue"
        );
    }
}
