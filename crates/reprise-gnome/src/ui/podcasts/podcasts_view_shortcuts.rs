//! Selection keyboard handling for the Podcasts root surface.

use gtk4::gdk;

use super::*;

fn escape_propagation(
    key: gdk::Key,
    modifiers: gdk::ModifierType,
    clear_selection: impl FnOnce() -> bool,
) -> glib::Propagation {
    if key != gdk::Key::Escape || !modifiers.is_empty() {
        return glib::Propagation::Proceed;
    }
    if clear_selection() {
        glib::Propagation::Stop
    } else {
        glib::Propagation::Proceed
    }
}

fn is_select_all(key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    key == gdk::Key::a && modifiers == gdk::ModifierType::CONTROL_MASK
}

impl PodcastsView {
    pub(super) fn install_selection_shortcuts(self: &Rc<Self>) {
        let controller = gtk4::EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        let weak = Rc::downgrade(self);
        controller.connect_key_pressed(move |_, key, _, modifiers| {
            if is_select_all(key, modifiers) {
                let Some(view) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                return if view.select_all_in_focused_source() {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                };
            }
            escape_propagation(key, modifiers, || {
                weak.upgrade()
                    .is_some_and(|view| view.clear_visible_selection())
            })
        });
        self.root.add_controller(controller);
    }

    pub(super) fn clear_visible_selection(&self) -> bool {
        if self.stack.visible_child_name().as_deref() == Some("youtube-channel") {
            return self.youtube_detail.clear_selection();
        }
        let cleared = self.selection.borrow_mut().clear();
        if cleared {
            self.render();
        }
        cleared
    }

    fn select_all_in_focused_source(self: &Rc<Self>) -> bool {
        if self.stack.visible_child_name().as_deref() == Some("youtube-channel") {
            return self.youtube_detail.select_all_visible();
        }
        let rendered = self.rendered_order();
        let rows = self
            .selection_widgets
            .borrow()
            .iter()
            .map(|(episode_id, widgets)| (*episode_id, widgets.row.clone()))
            .collect::<Vec<_>>();
        let focused = rows
            .iter()
            .find(|(_, row)| row.has_focus())
            .map(|(episode_id, _)| *episode_id);
        let visible = rows
            .iter()
            .map(|(episode_id, _)| *episode_id)
            .collect::<std::collections::BTreeSet<_>>();
        let subscriptions = self
            .groups
            .borrow()
            .iter()
            .flat_map(|group| {
                group
                    .episodes
                    .iter()
                    .map(move |episode| (episode.id, group.subscription_id))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let order = rendered
            .into_iter()
            .filter(|episode_id| visible.contains(episode_id))
            .filter_map(|episode_id| {
                subscriptions
                    .get(&episode_id)
                    .map(|subscription_id| (*subscription_id, episode_id))
            })
            .collect::<Vec<_>>();
        let selected = super::super::podcasts_selection::select_all_in_source(&order, focused);
        self.selection.borrow_mut().replace_with(selected);
        self.apply_selection();
        true
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn src_12a_escape_stops_only_after_clearing_a_selection() {
        assert_eq!(
            escape_propagation(gdk::Key::Escape, gdk::ModifierType::empty(), || true),
            glib::Propagation::Stop
        );
        assert_eq!(
            escape_propagation(gdk::Key::Escape, gdk::ModifierType::empty(), || false),
            glib::Propagation::Proceed
        );
    }

    #[test]
    fn src_12a_ctrl_a_requires_exactly_the_control_modifier() {
        assert!(is_select_all(gdk::Key::a, gdk::ModifierType::CONTROL_MASK));
        assert!(!is_select_all(gdk::Key::a, gdk::ModifierType::empty()));
        assert!(!is_select_all(
            gdk::Key::a,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
        ));
    }

    #[test]
    fn src_12a_modified_escape_and_other_keys_proceed_without_clearing() {
        for (key, modifiers) in [
            (gdk::Key::Escape, gdk::ModifierType::CONTROL_MASK),
            (gdk::Key::Escape, gdk::ModifierType::SHIFT_MASK),
            (gdk::Key::Escape, gdk::ModifierType::ALT_MASK),
            (gdk::Key::Escape, gdk::ModifierType::SUPER_MASK),
            (gdk::Key::Return, gdk::ModifierType::empty()),
        ] {
            let cleared = Cell::new(false);

            assert_eq!(
                escape_propagation(key, modifiers, || {
                    cleared.set(true);
                    true
                }),
                glib::Propagation::Proceed
            );
            assert!(!cleared.get());
        }
    }
}
