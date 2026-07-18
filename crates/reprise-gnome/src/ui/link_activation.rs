//! Shared click/Enter activation for link-like non-button player surfaces.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

pub(in crate::ui) const LINK_CLASS: &str = "reprise-link-surface";
pub(in crate::ui) type Activation = Rc<dyn Fn()>;
pub(in crate::ui) type ActivationSlot = Rc<RefCell<Option<Activation>>>;

pub(in crate::ui) fn route_key(
    key: gtk4::gdk::Key,
    activate: impl FnOnce(),
) -> gtk4::glib::Propagation {
    if matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter) {
        activate();
        gtk4::glib::Propagation::Stop
    } else {
        gtk4::glib::Propagation::Proceed
    }
}

pub(in crate::ui) fn arm(widget: &impl IsA<gtk4::Widget>, activate: Rc<dyn Fn()>) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    // a11y-semantics: role=link name=reveal-playing-album state=enabled action=activate
    widget.set_focusable(true);
    // input-parity: ACC-8 keyboard=link-enter-controller
    widget.set_cursor_from_name(Some("pointer"));
    widget.set_accessible_role(gtk4::AccessibleRole::Link);
    widget.add_css_class(LINK_CLASS);
    let label = crate::ui::strings::text(crate::ui::strings::REVEAL_PLAYING_ALBUM);
    widget.update_property(&[gtk4::accessible::Property::Label(&label)]);

    // input-parity: ACC-8 keyboard=link-enter-controller
    let click = gtk4::GestureClick::new();
    let click_activate = activate.clone();
    click.connect_released(move |_, _, _, _| click_activate());
    widget.add_controller(click);

    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| route_key(key, || activate()));
    widget.add_controller(keys);
}

pub(in crate::ui) fn arm_slot(widget: &impl IsA<gtk4::Widget>, slot: &ActivationSlot) {
    let slot = slot.clone();
    arm(
        widget,
        Rc::new(move || {
            let callback = slot.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        }),
    );
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{LINK_CLASS}:focus-visible {{ \
           outline: 2px solid @accent_color; outline-offset: 2px; \
           border-radius: 4px; }}"
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn enter_activates_link_surface_and_space_propagates() {
        let calls = Cell::new(0);
        assert_eq!(
            route_key(gtk4::gdk::Key::Return, || calls.set(calls.get() + 1)),
            gtk4::glib::Propagation::Stop
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            route_key(gtk4::gdk::Key::space, || calls.set(calls.get() + 1)),
            gtk4::glib::Propagation::Proceed
        );
        assert_eq!(calls.get(), 1);
    }
}
