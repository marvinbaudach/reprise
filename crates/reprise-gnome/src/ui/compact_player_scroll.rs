use std::rc::Rc;

use gtk4::prelude::*;

pub(super) const VOLUME_STEP: f64 = 0.05;

pub(super) fn stepped_volume(current: f64, direction: f64) -> Option<f64> {
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

pub(super) fn install(
    region: &gtk4::Widget,
    current: Rc<dyn Fn() -> f64>,
    changed: Rc<dyn Fn(f64)>,
) {
    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll(move |_, _, direction| {
        let Some(volume) = stepped_volume(current(), direction) else {
            return gtk4::glib::Propagation::Proceed;
        };
        changed(volume);
        gtk4::glib::Propagation::Stop
    });
    region.add_controller(scroll);
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
}
