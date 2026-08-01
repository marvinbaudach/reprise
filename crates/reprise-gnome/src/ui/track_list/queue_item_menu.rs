//! Context-menu routing for typed rows in the Queue management surface.

use gtk4::gio;
use reprise_core::up_next::QueueItem;

use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum QueueMenuRoute {
    TrackBuilder,
    CommonQueueItems,
}

pub(in crate::ui) fn route(items: &[QueueItem]) -> QueueMenuRoute {
    if items
        .iter()
        .any(|item| matches!(item, QueueItem::Episode(_)))
    {
        QueueMenuRoute::CommonQueueItems
    } else {
        QueueMenuRoute::TrackBuilder
    }
}

pub(in crate::ui) fn build_common_queue_menu(count: usize, editable: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if !editable {
        return menu;
    }
    let primary = gio::Menu::new();
    primary.append(
        Some(&strings::text(strings::CONTEXT_MENU_MOVE_TO_TOP)),
        Some("tracklist.move-to-top"),
    );
    menu.append_section(None, &primary);

    let destructive = gio::Menu::new();
    destructive.append(
        Some(&strings::remove_from_queue_label(count)),
        Some("tracklist.remove-from-queue"),
    );
    menu.append_section(None, &destructive);
    menu
}

#[cfg(test)]
mod tests {
    use gtk4::gio::prelude::*;
    use reprise_core::up_next::QueueItem;

    use super::{build_common_queue_menu, route, QueueMenuRoute};

    fn labels(menu: &gtk4::gio::Menu) -> Vec<String> {
        (0..menu.n_items())
            .flat_map(|section| {
                let section = menu
                    .item_link(section, gtk4::gio::MENU_LINK_SECTION)
                    .expect("top-level item is a section");
                (0..section.n_items()).filter_map(move |index| {
                    section
                        .item_attribute_value(index, gtk4::gio::MENU_ATTRIBUTE_LABEL, None)
                        .and_then(|value| value.get::<String>())
                })
            })
            .collect()
    }

    #[test]
    fn ctx_11_queue_selection_routes_heterogeneous_items_to_common_actions_only() {
        assert_eq!(
            route(&[QueueItem::Track(7), QueueItem::Track(8)]),
            QueueMenuRoute::TrackBuilder
        );
        assert_eq!(
            route(&[QueueItem::Episode(7)]),
            QueueMenuRoute::CommonQueueItems
        );
        assert_eq!(
            route(&[QueueItem::Track(7), QueueItem::Episode(7)]),
            QueueMenuRoute::CommonQueueItems
        );

        let editable_labels = labels(&build_common_queue_menu(2, true));
        assert_eq!(editable_labels, ["Move to top", "Remove 2 from queue"]);
        assert!(labels(&build_common_queue_menu(1, false)).is_empty());
    }
}
