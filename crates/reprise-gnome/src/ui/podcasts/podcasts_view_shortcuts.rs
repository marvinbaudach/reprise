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

/// Ctrl+A, read the way a keyboard actually delivers it.
///
/// Two things the obvious `key == Key::a && modifiers == CONTROL_MASK` gets
/// wrong, both of them silent: Caps Lock sends `Key::A` rather than `Key::a`,
/// and Caps or Num Lock add a lock bit to the modifier state, which an exact
/// comparison rejects. Either one leaves the user pressing Ctrl+A and nothing
/// happening at all. Masking with GTK's accelerator mask drops the lock bits
/// while keeping Shift, Alt and Super — so Ctrl+Shift+A is still not this.
fn is_select_all(key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    key.to_lower() == gdk::Key::a
        && modifiers & ACCELERATOR_MODIFIERS == gdk::ModifierType::CONTROL_MASK
}

/// The modifiers an accelerator is allowed to care about — GTK's default
/// accelerator mask, written out rather than read from
/// `gtk4::accelerator_get_default_mod_mask()`, which asserts an initialised
/// GTK and would drag this predicate behind the display gate. Everything
/// outside this set is lock and pointer-button noise that must not change what
/// a key combination means.
const ACCELERATOR_MODIFIERS: gdk::ModifierType = gdk::ModifierType::SHIFT_MASK
    .union(gdk::ModifierType::CONTROL_MASK)
    .union(gdk::ModifierType::ALT_MASK)
    .union(gdk::ModifierType::SUPER_MASK)
    .union(gdk::ModifierType::HYPER_MASK)
    .union(gdk::ModifierType::META_MASK);

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
        // The row that *contains* the focus, not the row that *is* the focus.
        // A row's own `has_focus()` is false whenever the focus sits on one of
        // its buttons — the download action or the ⋮ — and every one of those
        // is focusable. Reading only the row would report "nothing focused"
        // there, and `select_all_in_source` then documents its fallback as the
        // whole rendered list: Ctrl+A would quietly select every source
        // instead of the one the user is standing in.
        let focus = self
            .root
            .root()
            .and_downcast::<gtk4::Window>()
            .and_then(|window| gtk4::prelude::GtkWindowExt::focus(&window));
        let focused = rows
            .iter()
            .find(|(_, row)| {
                focus.as_ref().is_some_and(|focus| {
                    focus == row.upcast_ref::<gtk4::Widget>() || focus.is_ancestor(row)
                })
            })
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
    fn src_12b_escape_stops_only_after_clearing_a_selection() {
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
    fn src_12b_ctrl_a_requires_exactly_the_control_modifier() {
        assert!(is_select_all(gdk::Key::a, gdk::ModifierType::CONTROL_MASK));
        assert!(!is_select_all(gdk::Key::a, gdk::ModifierType::empty()));
        assert!(!is_select_all(
            gdk::Key::a,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
        ));
        assert!(!is_select_all(
            gdk::Key::a,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK
        ));
    }

    /// `SRC-12b`: a lock key must not disarm the shortcut. Caps Lock sends the
    /// upper-case keyval *and* sets a lock bit — a user with it engaged would
    /// otherwise press Ctrl+A and watch nothing happen, with nothing on screen
    /// to explain it. Masking with the accelerator mask also drops the other
    /// lock bits a platform may add, which is why the mask is used rather than
    /// subtracting `LOCK_MASK` by hand.
    #[test]
    fn src_12b_ctrl_a_survives_caps_lock() {
        assert!(is_select_all(
            gdk::Key::A,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::LOCK_MASK
        ));
        assert!(is_select_all(
            gdk::Key::a,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::LOCK_MASK
        ));
        // Shift still means something else, lock bits or not.
        assert!(!is_select_all(
            gdk::Key::A,
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::LOCK_MASK
        ));
    }

    #[test]
    fn src_12b_modified_escape_and_other_keys_proceed_without_clearing() {
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
