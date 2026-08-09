//! One startup-wide release point for automatic background work.
//!
//! Work registered while the window is being composed waits until the first
//! mapped frame has been painted and the main loop later reaches a low-priority
//! slot after a short quiet interval. Once released, this becomes a pass-through:
//! explicit work requested by the user later in the session is never delayed.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

const QUIET_INTERVAL: Duration = Duration::from_millis(100);

type Callback = Box<dyn FnOnce()>;

#[derive(Default)]
struct Gate {
    armed: bool,
    open: bool,
    waiting: Vec<Callback>,
}

thread_local! {
    static GATE: RefCell<Gate> = RefCell::new(Gate::default());
}

/// Runs `work` once startup has reached its shared quiet point.
///
/// After the one-shot gate opens this calls `work` synchronously, which is the
/// important distinction between startup-automatic work and a later explicit
/// user request routed through the same subsystem.
pub(crate) fn run_after_quiet(work: impl FnOnce() + 'static) {
    let work = GATE.with_borrow_mut(|gate| {
        if gate.open {
            Some(Box::new(work) as Callback)
        } else {
            gate.waiting.push(Box::new(work));
            None
        }
    });
    if let Some(work) = work {
        work();
    }
}

/// Arms the one shared first-frame observer for this application lifetime.
pub(crate) fn arm(window: &impl IsA<gtk4::Widget>) {
    let should_arm = GATE.with_borrow_mut(|gate| {
        if gate.armed || gate.open {
            false
        } else {
            gate.armed = true;
            true
        }
    });
    if !should_arm {
        return;
    }

    window.add_tick_callback(move |_, frame_clock| {
        let handler = Rc::new(RefCell::new(None));
        let handler_for_callback = handler.clone();
        let id = frame_clock.connect_after_paint(move |frame_clock| {
            let id = handler_for_callback.borrow_mut().take();
            if let Some(id) = id {
                frame_clock.disconnect(id);
            }
            gtk4::glib::timeout_add_local_full(QUIET_INTERVAL, gtk4::glib::Priority::LOW, || {
                release();
                gtk4::glib::ControlFlow::Break
            });
        });
        *handler.borrow_mut() = Some(id);
        gtk4::glib::ControlFlow::Break
    });
}

fn release() {
    let waiting = GATE.with_borrow_mut(|gate| {
        if gate.open {
            return Vec::new();
        }
        gate.open = true;
        std::mem::take(&mut gate.waiting)
    });
    for work in waiting {
        work();
    }
}

#[cfg(test)]
fn reset_for_test() {
    GATE.with_borrow_mut(|gate| *gate = Gate::default());
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn automatic_work_waits_for_one_release_then_later_work_runs_immediately() {
        super::reset_for_test();
        let calls = Rc::new(RefCell::new(Vec::new()));

        for name in ["artwork", "radio", "musicbrainz", "spectrogram"] {
            let calls = calls.clone();
            super::run_after_quiet(move || calls.borrow_mut().push(name));
        }

        assert!(calls.borrow().is_empty());
        super::release();
        assert_eq!(
            calls.borrow().as_slice(),
            ["artwork", "radio", "musicbrainz", "spectrogram"]
        );

        super::release();
        assert_eq!(calls.borrow().len(), 4, "the gate fires only once");

        let calls_after_startup = calls.clone();
        super::run_after_quiet(move || calls_after_startup.borrow_mut().push("explicit"));
        assert_eq!(calls.borrow().last(), Some(&"explicit"));
    }
}
