//! Test-only: letting GTK finish what a display test just asked for.
//!
//! A display test almost always has to wait for something — a window to map,
//! an allocation to happen, a scroll to settle, a CSS transition to run. GTK
//! does that work across later main-loop turns, most of it driven by the frame
//! clock, which advances in real time.
//!
//! Two shapes were in use before this module, and one of them is wrong:
//!
//! ```ignore
//! while gtk4::glib::MainContext::default().iteration(false) {}   // drains, then returns
//! while Instant::now() < deadline {                              // ... in a busy loop
//!     while gtk4::glib::MainContext::default().iteration(false) {}
//! }
//! ```
//!
//! `iteration(false)` returns immediately when nothing is pending, so the
//! deadline loop spins at full CPU — starving the frame clock it is waiting
//! for. On an idle machine it still works, because there is a spare core. Under
//! contention, which is exactly what parallel display-test workers create, it
//! competes with the thing it is waiting for.
//!
//! `iteration(true)` blocks until a source fires, so the test thread sleeps
//! and GTK gets the CPU. Both helpers here use it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;

/// Runs the main loop for `duration`, then returns.
///
/// Use this when the test needs time to pass — a CSS transition, an animation
/// — rather than a condition to become true. When there *is* a condition,
/// [`settle_until`] is both faster and more honest about what it waits for.
pub(in crate::ui) fn settle_for(duration: Duration) {
    let main_loop = glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    glib::timeout_add_local_once(duration, move || quit.quit());
    main_loop.run();
}

/// Runs the main loop until `ready` returns true, or `timeout` elapses.
///
/// Returns whether the condition held. Callers still assert what they came to
/// assert: a test that treats the return value as its assertion reports
/// "timed out" instead of showing the value that was wrong.
///
/// The repeating tick exists so the blocking iteration returns on a schedule.
/// Without it the loop would only re-check when GTK happened to have work, and
/// a condition that becomes true through a side effect no source announces
/// would never be noticed.
pub(in crate::ui) fn settle_until(timeout: Duration, mut ready: impl FnMut() -> bool) -> bool {
    if ready() {
        return true;
    }
    let ticked = Rc::new(Cell::new(false));
    let setter = ticked.clone();
    let tick = glib::timeout_add_local(Duration::from_millis(5), move || {
        setter.set(true);
        glib::ControlFlow::Continue
    });
    let context = glib::MainContext::default();
    let deadline = Instant::now() + timeout;
    let mut settled = false;
    while Instant::now() < deadline {
        context.iteration(true);
        ticked.set(false);
        if ready() {
            settled = true;
            break;
        }
    }
    tick.remove();
    settled
}

/// The wait a display test should use when it has no better bound.
///
/// Generous on purpose: the cost of a long timeout is paid only by a test that
/// is about to fail anyway, while a short one is paid by every run on a busy
/// machine.
pub(in crate::ui) const DISPLAY_TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Waits until `widget` is mapped and has been allocated a height.
///
/// Mapping goes through the X server, so it is not guaranteed by draining the
/// pending events once — that is what made
/// `contrast_2a_status_overlay_renders_with_content` fail under load while
/// passing in isolation.
pub(in crate::ui) fn settle_until_mapped(widget: &impl IsA<gtk4::Widget>) -> bool {
    let widget = widget.as_ref().clone();
    settle_until(DISPLAY_TEST_TIMEOUT, move || {
        widget.is_mapped() && widget.height() > 0
    })
}
