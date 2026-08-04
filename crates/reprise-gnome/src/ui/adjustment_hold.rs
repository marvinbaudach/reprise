//! Short-lived protection against asynchronous GTK scroll resets.
//!
//! `GtkColumnView` can change its vertical adjustment after the API call that
//! triggered the work has returned: a full model replacement and
//! `scroll_to(...FOCUS...)` both do this. Re-applying the value immediately
//! after those calls therefore still leaves a later position-zero frame.
//! This helper listens for both value and bounds changes during that brief
//! handover and restores the requested value from an idle callback. Signal
//! emissions can originate inside GTK's allocation pass, where writing the
//! adjustment synchronously would re-enter the running layout.
//!
//! ## One hold per adjustment, and a correction budget
//!
//! Two holds on one adjustment are not two protections — they are a fight.
//! Each answers the other's `set_value` by writing its own target back, and
//! `correcting` only stops a hold re-entering *itself*. Deleting the playing
//! track built exactly that pair within one main-loop turn: the confirmation
//! dialog's focus guard restoring the pre-delete scroll value, and the reload
//! that follows installing the post-delete anchor one row height away. The
//! two spun at 100% CPU, and neither could ever be released, because
//! `release_after` is a main-loop timeout and the spin owns the main loop —
//! the window froze until the user killed it.
//!
//! So a new hold supersedes any live hold on the same adjustment (the newest
//! request is the current intent), and, as a second line of defence against
//! any other contender for this much-contested property, a hold that has to
//! correct more times than a brief handover could ever need gives up rather
//! than wedge the main loop.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk4::glib::{self, SignalHandlerId};
use gtk4::prelude::*;

const VALUE_EPSILON: f64 = 0.5;

/// A hold covers one widget handover: a model swap, a `scroll_to`, the
/// allocation pass that follows. That settles in a handful of corrections;
/// this bound is far above the honest case and only ever trips on a fight.
const MAX_CORRECTIONS: u32 = 64;

#[derive(Clone)]
pub(super) struct AdjustmentHold {
    inner: Rc<HoldInner>,
}

struct HoldInner {
    adjustment: gtk4::Adjustment,
    target: Cell<f64>,
    correcting: Cell<bool>,
    pending: Cell<bool>,
    active: Cell<bool>,
    corrections: Cell<u32>,
    handlers: RefCell<Vec<SignalHandlerId>>,
}

thread_local! {
    /// Every live hold, so a new one can retire the ones it supersedes. Weak,
    /// so a hold dropped without an explicit release still disappears from
    /// here. All holds are built on the main thread with the widgets they
    /// guard, which is what makes a thread-local the whole registry.
    static LIVE_HOLDS: RefCell<Vec<Weak<HoldInner>>> = const { RefCell::new(Vec::new()) };
}

/// Retires every live hold on `adjustment` and forgets the dead entries.
///
/// Called before a new hold connects its own handlers, so the two never
/// overlap even for one signal emission — an overlap is the fight described
/// in the module doc, not a stronger guarantee.
fn supersede_holds_on(adjustment: &gtk4::Adjustment) {
    let superseded = LIVE_HOLDS.with(|holds| {
        let mut holds = holds.borrow_mut();
        let mut superseded = Vec::new();
        holds.retain(|weak| {
            let Some(hold) = weak.upgrade() else {
                return false;
            };
            if !hold.active.get() {
                return false;
            }
            if hold.adjustment.as_ptr() == adjustment.as_ptr() {
                superseded.push(hold);
                return false;
            }
            true
        });
        superseded
    });
    // Released outside the borrow: `release` disconnects handlers and drops
    // the last reference of a hold nobody else kept, and a `Drop` that
    // reached back into the registry would panic on the live borrow.
    for hold in superseded {
        release(&hold);
    }
}

impl AdjustmentHold {
    pub(super) fn new(adjustment: &gtk4::Adjustment) -> Self {
        supersede_holds_on(adjustment);
        let inner = Rc::new(HoldInner {
            adjustment: adjustment.clone(),
            target: Cell::new(adjustment.value()),
            correcting: Cell::new(false),
            pending: Cell::new(false),
            active: Cell::new(true),
            corrections: Cell::new(0),
            handlers: RefCell::new(Vec::new()),
        });
        LIVE_HOLDS.with(|holds| holds.borrow_mut().push(Rc::downgrade(&inner)));

        let weak = Rc::downgrade(&inner);
        let value_handler = adjustment.connect_value_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                restore_deferred(&inner);
            }
        });
        let weak = Rc::downgrade(&inner);
        let bounds_handler = adjustment.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                restore_deferred(&inner);
            }
        });
        inner
            .handlers
            .borrow_mut()
            .extend([value_handler, bounds_handler]);
        restore_direct(&inner);
        Self { inner }
    }

    pub(super) fn set_target(&self, target: f64) {
        if target.is_finite() {
            self.inner.target.set(target);
            restore_direct(&self.inner);
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

fn correction_target(inner: &HoldInner) -> Option<f64> {
    if !inner.active.get() || inner.correcting.get() {
        return None;
    }
    let target = bounded_target(
        inner.adjustment.lower(),
        inner.adjustment.upper(),
        inner.adjustment.page_size(),
        inner.target.get(),
    )?;
    if (inner.adjustment.value() - target).abs() <= VALUE_EPSILON {
        return None;
    }
    Some(target)
}

fn claim_correction(inner: &HoldInner, target: f64) -> bool {
    let corrections = inner.corrections.get() + 1;
    inner.corrections.set(corrections);
    if corrections > MAX_CORRECTIONS {
        // Something is writing this adjustment as insistently as we are.
        // Losing the scroll position is a blemish; holding the main loop
        // hostage is an app the user can only kill, so we yield.
        tracing::warn!(
            corrections,
            target,
            value = inner.adjustment.value(),
            "scroll hold outlasted its correction budget; releasing it"
        );
        release(inner);
        return false;
    }
    true
}

fn write_target(inner: &HoldInner, target: f64) {
    inner.correcting.set(true);
    inner.adjustment.set_value(target);
    inner.correcting.set(false);
}

/// Restores from ordinary application code, where no GTK signal emission or
/// allocation is on the stack. Construction and explicit target changes use
/// this path so the reload path retains its immediate pre-paint placement.
fn restore_direct(inner: &HoldInner) {
    let Some(target) = correction_target(inner) else {
        return;
    };
    if claim_correction(inner, target) {
        write_target(inner, target);
    }
}

/// Queues a restore requested by an adjustment signal. GTK may emit both
/// `changed` and `value-changed` from `gtk_adjustment_configure` while a
/// widget is being allocated, so this path must not write synchronously.
fn restore_deferred(inner: &Rc<HoldInner>) {
    if inner.pending.get() {
        return;
    }
    let Some(target) = correction_target(inner) else {
        return;
    };
    if !claim_correction(inner, target) {
        return;
    }
    inner.pending.set(true);
    let weak = Rc::downgrade(inner);
    glib::idle_add_local_once(move || {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        inner.pending.set(false);
        // The hold may have expired or been superseded, and configure may
        // have changed the range again. Re-check both at execution time.
        if let Some(target) = correction_target(&inner) {
            write_target(&inner, target);
        }
    });
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

    fn scrollable() -> gtk4::Adjustment {
        // GtkAdjustment itself is a display-free GObject. The safe gtk-rs
        // constructor nevertheless asserts that all of GTK was initialized,
        // so use the same FFI constructor directly for these unit tests.
        unsafe {
            gtk4::glib::translate::from_glib_none(gtk4::ffi::gtk_adjustment_new(
                0.0, 0.0, 10_000.0, 1.0, 10.0, 1_000.0,
            ))
        }
    }

    #[test]
    fn value_change_is_restored_only_after_the_signal_returns() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let adjustment = scrollable();
        let hold = AdjustmentHold::new(&adjustment);
        hold.set_target(5_000.0);

        adjustment.set_value(0.0);

        assert_eq!(adjustment.value(), 0.0);
        assert!(gtk4::glib::MainContext::default().iteration(false));
        assert_eq!(adjustment.value(), 5_000.0);
    }

    #[test]
    fn bounds_change_is_restored_later_against_the_latest_range() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let adjustment = scrollable();
        let hold = AdjustmentHold::new(&adjustment);
        hold.set_target(9_000.0);

        adjustment.configure(0.0, 0.0, 12_000.0, 1.0, 10.0, 1_000.0);
        adjustment.configure(0.0, 0.0, 8_000.0, 1.0, 10.0, 2_000.0);

        assert_eq!(adjustment.value(), 0.0);
        assert!(gtk4::glib::MainContext::default().iteration(false));
        assert_eq!(adjustment.value(), 6_000.0);
    }

    #[test]
    fn signal_corrections_coalesce_to_one_queued_write() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let adjustment = scrollable();
        let hold = AdjustmentHold::new(&adjustment);
        hold.set_target(5_000.0);
        let target_writes = Rc::new(Cell::new(0));
        let target_writes_for_signal = target_writes.clone();
        adjustment.connect_value_changed(move |adjustment| {
            if adjustment.value() == 5_000.0 {
                target_writes_for_signal.set(target_writes_for_signal.get() + 1);
            }
        });

        adjustment.set_value(0.0);
        adjustment.set_value(100.0);

        assert_eq!(adjustment.value(), 100.0);
        assert_eq!(target_writes.get(), 0);
        assert!(gtk4::glib::MainContext::default().iteration(false));
        assert_eq!(adjustment.value(), 5_000.0);
        assert_eq!(target_writes.get(), 1);
    }

    #[test]
    fn released_hold_does_not_run_its_queued_write() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let adjustment = scrollable();
        let hold = AdjustmentHold::new(&adjustment);
        hold.set_target(5_000.0);
        adjustment.set_value(0.0);

        release(&hold.inner);

        assert!(gtk4::glib::MainContext::default().iteration(false));
        assert_eq!(adjustment.value(), 0.0);
    }

    #[test]
    fn superseded_hold_does_not_run_its_queued_write() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let adjustment = scrollable();
        let first = AdjustmentHold::new(&adjustment);
        first.set_target(5_000.0);
        adjustment.set_value(0.0);

        let second = AdjustmentHold::new(&adjustment);
        second.set_target(6_000.0);

        assert!(!first.inner.active.get());
        assert!(gtk4::glib::MainContext::default().iteration(false));
        assert_eq!(adjustment.value(), 6_000.0);
    }

    /// Deleting the loaded track builds two holds on the table's one
    /// adjustment within the same main-loop turn: the delete dialog's focus
    /// guard restores the pre-delete scroll value, and the reload that follows
    /// installs the post-delete anchor — one row height apart. Each one's
    /// `set_value` fires `value-changed`, which the *other* answered by
    /// writing its own target back. `correcting` only guards a hold against
    /// its own re-entry, so the two spun forever at 100% CPU, and the timeout
    /// that would have released either of them could never run because it
    /// needs the main loop this very spin is monopolising.
    #[test]
    fn a_second_hold_supersedes_the_first_instead_of_fighting_it() {
        let adjustment = scrollable();
        let first = AdjustmentHold::new(&adjustment);
        first.set_target(5_000.0);
        assert_eq!(adjustment.value(), 5_000.0);

        let second = AdjustmentHold::new(&adjustment);
        second.set_target(5_033.0);

        assert_eq!(adjustment.value(), 5_033.0);
        // The superseded hold must stay quiet even when the adjustment moves
        // again, or it would resume the fight on the next GTK reset.
        adjustment.set_value(5_033.0);
        first.set_target(5_000.0);
        assert_eq!(adjustment.value(), 5_033.0);
    }

    /// The rule above settles holds against each other, but anything else on
    /// the adjustment — GTK's own layout, a scroll animation — can push back
    /// just as hard. A hold covers a brief handover, so a bounded number of
    /// corrections is all it may ever need; past that it yields rather than
    /// wedge the main loop, which is the one outcome the user cannot escape.
    #[test]
    fn a_hold_stops_correcting_once_its_budget_is_spent() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let adjustment = scrollable();
        let hold = AdjustmentHold::new(&adjustment);
        hold.set_target(5_000.0);
        assert_eq!(adjustment.value(), 5_000.0);

        // Every successful deferred restore schedules the contender's next
        // write for another idle, reproducing the asynchronous fight without
        // nesting either adjustment write inside a signal handler.
        adjustment.connect_value_changed(|adjustment| {
            if adjustment.value() == 5_000.0 {
                let adjustment = adjustment.clone();
                glib::idle_add_local_once(move || adjustment.set_value(0.0));
            }
        });
        adjustment.set_value(0.0);
        let context = gtk4::glib::MainContext::default();
        for _ in 0..MAX_CORRECTIONS * 4 {
            if !hold.inner.active.get() {
                break;
            }
            assert!(context.iteration(false));
        }

        assert!(!hold.inner.active.get());
        assert_eq!(adjustment.value(), 0.0);
    }
}
