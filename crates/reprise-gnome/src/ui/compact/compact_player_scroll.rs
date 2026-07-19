use std::rc::Rc;

use gtk4::prelude::*;

pub(in crate::ui) const VOLUME_STEP: f64 = 0.05;

pub(in crate::ui) fn stepped_volume(current: f64, direction: f64) -> Option<f64> {
    if !current.is_finite() || !direction.is_finite() || direction == 0.0 {
        return None;
    }
    let delta = if direction < 0.0 {
        VOLUME_STEP
    } else {
        -VOLUME_STEP
    };
    Some((current + delta).clamp(0.0, 1.0))
}

fn keyboard_volume_direction(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
) -> Option<f64> {
    match (key, modifiers) {
        (gtk4::gdk::Key::Up, gtk4::gdk::ModifierType::CONTROL_MASK) => Some(-1.0),
        (gtk4::gdk::Key::Down, gtk4::gdk::ModifierType::CONTROL_MASK) => Some(1.0),
        _ => None,
    }
}

pub(in crate::ui) fn install(
    region: &gtk4::Widget,
    current: Rc<dyn Fn() -> f64>,
    changed: Rc<dyn Fn(f64)>,
) {
    let scroll_current = current.clone();
    let scroll_changed = changed.clone();
    // input-parity: ACC-8 keyboard=ctrl-up-down
    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll(move |_, _, direction| {
        let Some(volume) = stepped_volume(scroll_current(), direction) else {
            return gtk4::glib::Propagation::Proceed;
        };
        scroll_changed(volume);
        gtk4::glib::Propagation::Stop
    });
    region.add_controller(scroll);

    region.update_property(&[gtk4::accessible::Property::KeyShortcuts(
        "Control+ArrowUp Control+ArrowDown",
    )]);
    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(direction) = keyboard_volume_direction(key, modifiers) else {
            return gtk4::glib::Propagation::Proceed;
        };
        let Some(volume) = stepped_volume(current(), direction) else {
            return gtk4::glib::Propagation::Proceed;
        };
        changed(volume);
        gtk4::glib::Propagation::Stop
    });
    region.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_scroll_changes_volume_in_five_percent_steps() {
        assert_eq!(stepped_volume(0.50, -1.0), Some(0.55));
        assert_eq!(stepped_volume(0.50, 1.0), Some(0.45));
    }

    #[test]
    fn volume_steps_are_clamped_and_invalid_scroll_is_ignored() {
        assert_eq!(stepped_volume(0.98, -1.0), Some(1.0));
        assert_eq!(stepped_volume(0.02, 1.0), Some(0.0));
        assert_eq!(stepped_volume(0.50, 0.0), None);
        assert_eq!(stepped_volume(0.50, f64::NAN), None);
        assert_eq!(stepped_volume(f64::INFINITY, 1.0), None);
    }

    #[test]
    fn ctrl_arrows_are_the_keyboard_partner_for_volume_scroll() {
        let ctrl = gtk4::gdk::ModifierType::CONTROL_MASK;
        assert_eq!(
            keyboard_volume_direction(gtk4::gdk::Key::Up, ctrl),
            Some(-1.0)
        );
        assert_eq!(
            keyboard_volume_direction(gtk4::gdk::Key::Down, ctrl),
            Some(1.0)
        );
        assert_eq!(
            keyboard_volume_direction(gtk4::gdk::Key::Up, gtk4::gdk::ModifierType::empty()),
            None
        );
    }
}
