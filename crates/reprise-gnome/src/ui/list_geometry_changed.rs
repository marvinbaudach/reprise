//! One-shot `changed` subscriptions on a `GtkAdjustment`, and the marker that
//! tells a writer it is standing inside one.
//!
//! GTK emits `changed` (and `value-changed`) from `gtk_adjustment_configure`
//! *while a widget is being allocated*. Writing an adjustment from inside such
//! an emission re-enters GTK's running layout pass: `GtkListItemManager` keeps
//! the rows it had bound for the old anchor and never asks for a new
//! allocation, so the list renders empty while the scrollbar still shows
//! complete, correct geometry. `adjustment_hold::restore_deferred` defers to
//! `HIGH_IDLE` for exactly this reason; every other signal-driven writer must
//! do the same, and [`in_changed_emission`] is how it can tell.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib::prelude::ObjectExt;
use gtk4::prelude::AdjustmentExt;

thread_local! {
    /// Nesting depth of [`on_changed_once`] callbacks running on this thread.
    static CHANGED_EMISSION_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// True while an [`on_changed_once`] callback is on the stack — that is, while
/// a synchronous adjustment write would risk re-entering GTK's layout.
pub(in crate::ui) fn in_changed_emission() -> bool {
    CHANGED_EMISSION_DEPTH.with(|depth| depth.get() > 0)
}

/// Raises [`in_changed_emission`] for its lifetime, including on unwind, so a
/// panicking callback cannot leave the marker stuck and silence every
/// assertion that depends on it.
struct ChangedEmissionGuard;

impl ChangedEmissionGuard {
    fn enter() -> Self {
        CHANGED_EMISSION_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for ChangedEmissionGuard {
    fn drop(&mut self) {
        CHANGED_EMISSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

struct OneShot<F>(RefCell<Option<F>>);

impl<F> OneShot<F> {
    fn new(callback: F) -> Self {
        Self(RefCell::new(Some(callback)))
    }

    fn take(&self) -> Option<F> {
        self.0.borrow_mut().take()
    }
}

/// Runs `callback` on the adjustment's next `changed`, once. The handler is
/// disconnected before the callback runs, so a nested emission raised by the
/// callback itself cannot re-enter this subscription.
pub(in crate::ui) fn on_changed_once(
    adjustment: &gtk4::Adjustment,
    callback: impl FnOnce(&gtk4::Adjustment) + 'static,
) {
    let handler = Rc::new(RefCell::new(None));
    let pending_callback = Rc::new(OneShot::new(callback));
    let callback_handler = handler.clone();
    let callback_slot = pending_callback.clone();
    let id = adjustment.connect_changed(move |changed| {
        let handler = callback_handler.borrow_mut().take();
        if let Some(handler) = handler {
            changed.disconnect(handler);
        }
        let callback = callback_slot.take();
        if let Some(callback) = callback {
            let _emission = ChangedEmissionGuard::enter();
            callback(changed);
        }
    });
    handler.borrow_mut().replace(id);
}

/// Like [`on_changed_once`], but hands control back to GTK before running
/// `callback` — the form every writer of the adjustment must use, since the
/// emission it reacts to may be running inside `gtk_widget_allocate`.
///
/// The idle runs at `HIGH_IDLE`, not the default priority: GDK repaints at
/// `GDK_PRIORITY_REDRAW`, which is `HIGH_IDLE + 20`, so a default-priority
/// idle would land *after* the next frame and the correction would be visible
/// as a jump. `adjustment_hold::restore_deferred` defers on the same grounds.
pub(in crate::ui) fn after_changed_once(
    adjustment: &gtk4::Adjustment,
    callback: impl FnOnce() + 'static,
) {
    on_changed_once(adjustment, move |_| {
        let mut callback = Some(callback);
        gtk4::glib::idle_add_local_full(gtk4::glib::Priority::HIGH_IDLE, move || {
            if let Some(callback) = callback.take() {
                callback();
            }
            gtk4::glib::ControlFlow::Break
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_subscription_callback_can_only_be_taken_once() {
        let callback = OneShot::new(|| 42);

        assert_eq!(callback.take().map(|callback| callback()), Some(42));
        assert!(callback.take().is_none());
    }

    #[test]
    fn the_emission_marker_is_raised_only_while_a_callback_runs() {
        assert!(!in_changed_emission());
        {
            let _outer = ChangedEmissionGuard::enter();
            assert!(in_changed_emission());
            {
                let _inner = ChangedEmissionGuard::enter();
                assert!(in_changed_emission());
            }
            // A nested emission returning must not clear the marker for the
            // one still running underneath it.
            assert!(in_changed_emission());
        }
        assert!(!in_changed_emission());
    }

    #[test]
    fn a_panicking_callback_does_not_leave_the_emission_marker_stuck() {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _emission = ChangedEmissionGuard::enter();
            assert!(in_changed_emission());
            panic!("a changed callback exploded");
        }));

        assert!(panicked.is_err());
        // Were the marker left set, every `debug_assert!` guarding an
        // adjustment write would fire for the rest of this thread's life.
        assert!(!in_changed_emission());
    }
}
