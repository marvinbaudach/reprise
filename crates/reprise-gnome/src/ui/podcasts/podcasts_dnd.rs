//! Drag source for grouped podcast and YouTube episode rows.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::up_next::QueueItem;

use super::podcasts_selection::PodcastSelection;

fn drag_items(episode_id: i64, selection: &PodcastSelection) -> Vec<QueueItem> {
    let ids = if selection.contains(episode_id) {
        selection.selected_ids()
    } else {
        vec![episode_id]
    };
    ids.into_iter().map(QueueItem::Episode).collect()
}

pub(super) fn wire_episode_drag_source(
    row: &gtk4::Box,
    episode_id: i64,
    selection: &Rc<RefCell<PodcastSelection>>,
) {
    // input-parity: ACC-8 keyboard=episode-menu-queue-actions
    let source = gtk4::DragSource::new();
    source.set_actions(gtk4::gdk::DragAction::COPY);
    let selection = selection.clone();
    source.connect_prepare(move |_, _, _| {
        let items = {
            let selection = selection.borrow();
            drag_items(episode_id, &selection)
        };
        let payload = crate::ui::track_list_dnd::format_drag_payload(&items, None);
        Some(gtk4::gdk::ContentProvider::for_value(&payload.to_value()))
    });
    row.add_controller(source);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_episode_drag_carries_the_whole_current_selection() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(11, true);
        selection.set_selected(21, true);

        assert_eq!(
            drag_items(21, &selection),
            vec![QueueItem::Episode(11), QueueItem::Episode(21)]
        );
    }

    #[test]
    fn unselected_episode_drag_carries_only_the_dragged_row() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(11, true);
        selection.set_selected(21, true);

        assert_eq!(drag_items(30, &selection), vec![QueueItem::Episode(30)]);
        assert_eq!(
            crate::ui::track_list_dnd::format_drag_payload(&drag_items(30, &selection), None),
            "e30|-"
        );
    }
}
