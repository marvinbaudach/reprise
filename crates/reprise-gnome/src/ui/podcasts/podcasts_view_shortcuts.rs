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

impl PodcastsView {
    pub(super) fn install_selection_shortcuts(self: &Rc<Self>) {
        let controller = gtk4::EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        let weak = Rc::downgrade(self);
        controller.connect_key_pressed(move |_, key, _, modifiers| {
            escape_propagation(key, modifiers, || {
                weak.upgrade()
                    .is_some_and(|view| view.clear_visible_selection())
            })
        });
        self.root.add_controller(controller);
    }

    fn clear_visible_selection(&self) -> bool {
        if self.stack.visible_child_name().as_deref() == Some("youtube-channel") {
            return self.youtube_detail.clear_selection();
        }
        let cleared = self.selection.borrow_mut().clear();
        if cleared {
            self.render();
        }
        cleared
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn src_12_escape_stops_only_after_clearing_a_selection() {
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
    fn src_12_modified_escape_and_other_keys_proceed_without_clearing() {
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
