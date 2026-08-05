use std::rc::Rc;

use gtk4::{gio, prelude::*};
use reprise_core::modules::SOUND_SIMILARITY_MODULE;

use super::{track_menu::SelectionSummary, Shared};

const ACTION_FIND_SIMILAR: &str = "find-similar-tracks";

pub(super) fn append_menu_item(menu: &gio::Menu, shared: &Rc<Shared>, summary: SelectionSummary) {
    let enabled = reprise_core::modules::is_enabled(&shared.conn, &SOUND_SIMILARITY_MODULE)
        .unwrap_or(SOUND_SIMILARITY_MODULE.default_enabled);
    append_entry(menu, enabled && summary.count == 1 && !summary.all_missing);
}

fn append_entry(menu: &gio::Menu, visible: bool) {
    if !visible {
        return;
    }
    let section = gio::Menu::new();
    section.append(
        Some(&crate::ui::strings::text(
            crate::ui::strings::SOUND_FIND_SIMILAR,
        )),
        Some(ACTION_FIND_SIMILAR),
    );
    menu.append_section(None, &section);
}

pub(super) fn wire_action(action_group: &gio::SimpleActionGroup, shared: &Rc<Shared>) {
    let action = gio::SimpleAction::new(ACTION_FIND_SIMILAR, None);
    let shared = shared.clone();
    action.connect_activate(move |_, _| {
        let Some(id) = super::track_list_context_menu::current_selection_ids(&shared)
            .first()
            .copied()
        else {
            return;
        };
        let callback = shared.on_find_similar.borrow().clone();
        if let Some(callback) = callback {
            callback(id);
        }
    });
    action_group.add_action(&action);
}

#[cfg(test)]
mod tests {
    use gtk4::gio::prelude::MenuModelExt;

    use super::*;

    fn menu_labels(model: &gio::MenuModel) -> Vec<String> {
        let mut labels = Vec::new();
        for index in 0..model.n_items() {
            if let Some(label) = model
                .item_attribute_value(index, gio::MENU_ATTRIBUTE_LABEL, None)
                .and_then(|value| value.get::<String>())
            {
                labels.push(label);
            }
            if let Some(section) = model.item_link(index, gio::MENU_LINK_SECTION) {
                labels.extend(menu_labels(&section));
            }
        }
        labels
    }

    #[test]
    fn sim_7_find_similar_entry_follows_module_visibility() {
        let disabled = gio::Menu::new();
        append_entry(&disabled, false);
        assert!(!menu_labels(disabled.upcast_ref()).contains(&"Find similar tracks".to_string()));

        let enabled = gio::Menu::new();
        append_entry(&enabled, true);
        assert!(menu_labels(enabled.upcast_ref()).contains(&"Find similar tracks".to_string()));
    }
}
