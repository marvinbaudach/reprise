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

fn route_pointer_click(
    modifiers: gtk4::gdk::ModifierType,
    claim: impl FnOnce(),
    activate: impl FnOnce(),
) {
    let selection_modifiers =
        gtk4::gdk::ModifierType::CONTROL_MASK | gtk4::gdk::ModifierType::SHIFT_MASK;
    if modifiers.intersects(selection_modifiers) {
        return;
    }

    claim();
    activate();
}

pub(in crate::ui) fn arm(
    widget: &impl IsA<gtk4::Widget>,
    accessible_label: &str,
    activate: Rc<dyn Fn()>,
) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    present(widget, accessible_label);

    // input-parity: ACC-8 keyboard=link-enter-controller
    let click = gtk4::GestureClick::new();
    let click_activate = activate.clone();
    click.connect_released(move |gesture, _, _, _| {
        route_pointer_click(
            gesture.current_event_state(),
            || {
                // Claim the sequence so an enclosing row gesture does not
                // also fire. A link inside a row that itself acts on click
                // (the My Stats song rows) must navigate *instead of*
                // triggering the row, not as well as — otherwise one click
                // both opens the library and starts a track.
                gesture.set_state(gtk4::EventSequenceState::Claimed);
            },
            || click_activate(),
        );
    });
    widget.add_controller(click);

    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| route_key(key, || activate()));
    widget.add_controller(keys);
}

/// Turns `widget` into an active link.
///
/// **The accessible role is deliberately not set here.** GTK refuses to change
/// a role once the widget is realized, so assigning one per bind never took
/// effect — it only emitted a `Gtk-CRITICAL` every single time, and in a
/// recycling `ColumnView` that is thousands of synchronous journal writes per
/// minute on the main thread. A widget that can become a link therefore
/// declares `AccessibleRole::Link` in its own `class_init` (see
/// `track_list::track_cover`), and this pair only toggles the state that GTK
/// *does* let us change at runtime: focusability, cursor, styling and label.
pub(in crate::ui) fn present(widget: &impl IsA<gtk4::Widget>, accessible_label: &str) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    // a11y-semantics: role=link name=target state=enabled action=activate
    widget.set_focusable(true);
    // input-parity: ACC-8 keyboard=link-enter-controller
    widget.set_cursor_from_name(Some("pointer"));
    widget.add_css_class(LINK_CLASS);
    widget.update_property(&[gtk4::accessible::Property::Label(accessible_label)]);
}

/// The inverse of [`present`]: the widget keeps its structural role but stops
/// being reachable, so keyboard and screen-reader navigation skip it.
pub(in crate::ui) fn unpresent(widget: &impl IsA<gtk4::Widget>, accessible_label: &str) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    widget.set_focusable(false);
    widget.set_cursor_from_name(None);
    widget.remove_css_class(LINK_CLASS);
    widget.update_property(&[gtk4::accessible::Property::Label(accessible_label)]);
}

pub(in crate::ui) fn arm_slot(
    widget: &impl IsA<gtk4::Widget>,
    accessible_label: &str,
    slot: &ActivationSlot,
) {
    let slot = slot.clone();
    arm(
        widget,
        accessible_label,
        Rc::new(move || {
            let callback = slot.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        }),
    );
}

pub(in crate::ui) fn relabel(widget: &impl IsA<gtk4::Widget>, accessible_label: &str) {
    widget
        .upcast_ref::<gtk4::Widget>()
        .update_property(&[gtk4::accessible::Property::Label(accessible_label)]);
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

    #[test]
    fn selection_modifiers_leave_pointer_click_unclaimed_and_inactive() {
        for (modifiers, expected_calls, expected_claims) in [
            (gtk4::gdk::ModifierType::empty(), 1, 1),
            (gtk4::gdk::ModifierType::CONTROL_MASK, 0, 0),
            (gtk4::gdk::ModifierType::SHIFT_MASK, 0, 0),
        ] {
            let calls = Cell::new(0);
            let claims = Cell::new(0);

            route_pointer_click(
                modifiers,
                || claims.set(claims.get() + 1),
                || calls.set(calls.get() + 1),
            );

            assert_eq!(calls.get(), expected_calls, "modifiers: {modifiers:?}");
            assert_eq!(claims.get(), expected_claims, "modifiers: {modifiers:?}");
        }
    }

    /// A recycled cell runs `present`/`unpresent` over and over on a widget
    /// that is already realized. GTK refuses a role change at that point, so
    /// any attempt to swap the role would be both ineffective and loud — one
    /// `Gtk-CRITICAL` per bind. The role must therefore already be right from
    /// `class_init` and stay untouched through the whole cycle.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn a_realized_link_keeps_its_role_through_the_whole_bind_cycle() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let widget = crate::ui::track_cover::TrackCover::new();
        let window = gtk4::Window::new();
        window.set_child(Some(&widget));
        window.present();
        while gtk4::glib::MainContext::default().pending() {
            gtk4::glib::MainContext::default().iteration(false);
        }
        assert!(
            widget.is_realized(),
            "the point of this test is the realized widget"
        );

        for (step, expected_focusable) in [
            ("initial", false),
            ("present", true),
            ("unpresent", false),
            ("present again", true),
        ] {
            match step {
                "present" | "present again" => present(&widget, "Album"),
                "unpresent" => unpresent(&widget, ""),
                _ => {}
            }
            assert_eq!(
                widget.accessible_role(),
                gtk4::AccessibleRole::Link,
                "the role must survive {step} untouched"
            );
            assert_eq!(
                widget.is_focusable(),
                expected_focusable,
                "reachability is what {step} is allowed to change"
            );
        }
        window.close();
    }
}
