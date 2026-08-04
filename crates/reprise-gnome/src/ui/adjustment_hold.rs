//! Short-lived protection against asynchronous GTK scroll resets.
//!
//! `GtkColumnView` can change its vertical adjustment after the API call that
//! triggered the work has returned: a full model replacement and
//! `scroll_to(...FOCUS...)` both do this. Re-applying the value immediately
//! after those calls therefore still leaves a later position-zero frame.
//! This helper listens for both value and bounds changes during that brief
//! handover and synchronously restores the requested value.
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
            active: Cell::new(true),
            corrections: Cell::new(0),
            handlers: RefCell::new(Vec::new()),
        });
        LIVE_HOLDS.with(|holds| holds.borrow_mut().push(Rc::downgrade(&inner)));

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

    fn scrollable() -> gtk4::Adjustment {
        gtk4::init().unwrap();
        gtk4::Adjustment::new(0.0, 0.0, 10_000.0, 1.0, 10.0, 1_000.0)
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
    #[ignore = "requires a display; run via xvfb-run"]
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn a_hold_stops_correcting_once_its_budget_is_spent() {
        let adjustment = scrollable();
        let hold = AdjustmentHold::new(&adjustment);
        hold.set_target(5_000.0);
        assert_eq!(adjustment.value(), 5_000.0);

        // One foreign write per round, each answered by one correction —
        // a fight's shape, played out by hand because producing a real one
        // needs two handlers racing inside GTK's own layout.
        for _ in 0..MAX_CORRECTIONS + 8 {
            adjustment.set_value(0.0);
        }

        assert_eq!(adjustment.value(), 0.0);
    }
}
