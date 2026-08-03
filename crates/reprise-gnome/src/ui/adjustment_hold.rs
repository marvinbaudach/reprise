//! Short-lived protection against asynchronous GTK scroll resets.
//!
//! `GtkColumnView` can change its vertical adjustment after the API call that
//! triggered the work has returned: a full model replacement and
//! `scroll_to(...FOCUS...)` both do this. Re-applying the value immediately
//! after those calls therefore still leaves a later position-zero frame.
//! This helper listens for both value and bounds changes during that brief
//! handover and synchronously restores the requested value.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib::{self, SignalHandlerId};
use gtk4::prelude::*;

const VALUE_EPSILON: f64 = 0.5;

#[derive(Clone)]
pub(super) struct AdjustmentHold {
    inner: Rc<HoldInner>,
}

struct HoldInner {
    adjustment: gtk4::Adjustment,
    target: Cell<f64>,
    correcting: Cell<bool>,
    active: Cell<bool>,
    handlers: RefCell<Vec<SignalHandlerId>>,
}

impl AdjustmentHold {
    pub(super) fn new(adjustment: &gtk4::Adjustment) -> Self {
        let inner = Rc::new(HoldInner {
            adjustment: adjustment.clone(),
            target: Cell::new(adjustment.value()),
            correcting: Cell::new(false),
            active: Cell::new(true),
            handlers: RefCell::new(Vec::new()),
        });

        let weak = Rc::downgrade(&inner);
        let value_handler = adjustment.connect_value_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                restore(&inner);
            }
        });
        let weak = Rc::downgrade(&inner);
        let bounds_handler = adjustment.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                restore(&inner);
            }
        });
        inner
            .handlers
            .borrow_mut()
            .extend([value_handler, bounds_handler]);
        restore(&inner);
        Self { inner }
    }

    pub(super) fn set_target(&self, target: f64) {
        if target.is_finite() {
            self.inner.target.set(target);
            restore(&self.inner);
        }
    }

    pub(super) fn release_after(self, duration: Duration) {
        glib::timeout_add_local_once(duration, move || release(&self.inner));
    }
}

fn bounded_target(lower: f64, upper: f64, page: f64, target: f64) -> Option<f64> {
    if !lower.is_finite() || !upper.is_finite() || !page.is_finite() || upper <= lower {
        return None;
    }
    Some(target.clamp(lower, (upper - page).max(lower)))
}

fn restore(inner: &HoldInner) {
    if !inner.active.get() || inner.correcting.get() {
        return;
    }
    let Some(target) = bounded_target(
        inner.adjustment.lower(),
        inner.adjustment.upper(),
        inner.adjustment.page_size(),
        inner.target.get(),
    ) else {
        return;
    };
    if (inner.adjustment.value() - target).abs() <= VALUE_EPSILON {
        return;
    }
    inner.correcting.set(true);
    inner.adjustment.set_value(target);
    inner.correcting.set(false);
}

fn release(inner: &HoldInner) {
    if !inner.active.replace(false) {
        return;
    }
    for handler in inner.handlers.borrow_mut().drain(..) {
        inner.adjustment.disconnect(handler);
    }
}

impl Drop for HoldInner {
    fn drop(&mut self) {
        release(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_clamped_to_the_live_scrollable_range() {
        assert_eq!(bounded_target(0.0, 1_000.0, 200.0, 600.0), Some(600.0));
        assert_eq!(bounded_target(0.0, 1_000.0, 200.0, 900.0), Some(800.0));
    }
}
