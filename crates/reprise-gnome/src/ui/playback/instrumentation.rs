//! Small synchronous phase timer shared by playback instrumentation.

use std::rc::Rc;

pub(super) type DeferredTask = Box<dyn FnOnce()>;
pub(super) type IdleScheduler = Rc<dyn Fn(DeferredTask)>;

pub(super) const NOW_PLAYING_ENQUEUE_LISTENER: &str = "now_playing_enqueue";

#[derive(Clone)]
pub(super) struct QueueListener {
    name: &'static str,
    callback: Rc<dyn Fn()>,
}

impl QueueListener {
    pub(super) fn new(name: &'static str, callback: Rc<dyn Fn()>) -> Self {
        Self { name, callback }
    }

    pub(super) fn call(&self) {
        (self.callback)();
    }
}

pub(super) struct QueueListenerTimes {
    pub(super) synchronous_ms: u128,
    pub(super) now_playing_enqueue_ms: u128,
    pub(super) total_ms: u128,
}

pub(super) fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    let started = std::time::Instant::now();
    let result = operation();
    (result, started.elapsed().as_millis())
}

pub(super) fn time_queue_listeners(callbacks: Vec<QueueListener>) -> QueueListenerTimes {
    let mut synchronous_ms = 0;
    let mut now_playing_enqueue_ms = 0;
    let mut total_ms = 0;
    for listener in callbacks {
        let ((), elapsed_ms) = timed(|| listener.call());
        tracing::info!(
            listener = listener.name,
            elapsed_ms,
            "queue listener completed"
        );
        total_ms += elapsed_ms;
        if listener.name == NOW_PLAYING_ENQUEUE_LISTENER {
            now_playing_enqueue_ms += elapsed_ms;
        } else {
            synchronous_ms += elapsed_ms;
        }
    }
    QueueListenerTimes {
        synchronous_ms,
        now_playing_enqueue_ms,
        total_ms,
    }
}

/// Wraps the measured slow queue listener so queue mutations only enqueue its
/// work. The flag belongs to the wrapper, so every notification source shares
/// one pending refresh and a refresh-triggered notification can enqueue the
/// next idle instead of being lost.
pub(super) fn defer_queue_refresh_with(
    callback: Rc<dyn Fn()>,
    schedule_idle: IdleScheduler,
) -> Rc<dyn Fn()> {
    let pending = Rc::new(std::cell::Cell::new(false));
    Rc::new(move || {
        if pending.replace(true) {
            return;
        }
        let pending = pending.clone();
        let callback = callback.clone();
        schedule_idle(Box::new(move || {
            pending.set(false);
            let ((), now_playing_deferred_ms) = timed(|| callback());
            tracing::info!(
                target: "reprise::ui::playback",
                now_playing_deferred_ms,
                "queue listeners deferred"
            );
        }));
    })
}

pub(super) fn remaining_ms(started: std::time::Instant, measured: &[u128]) -> u128 {
    measured
        .iter()
        .fold(started.elapsed().as_millis(), |total, phase| {
            total.saturating_sub(*phase)
        })
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::ui::test_log_capture::CapturedLogs;

    #[test]
    fn several_queue_notifications_coalesce_one_deferred_now_playing_refresh() {
        let refreshes = Rc::new(Cell::new(0));
        let tasks = Rc::new(RefCell::new(Vec::<super::DeferredTask>::new()));
        let callback = {
            let refreshes = refreshes.clone();
            Rc::new(move || refreshes.set(refreshes.get() + 1)) as Rc<dyn Fn()>
        };
        let schedule = {
            let tasks = tasks.clone();
            Rc::new(move |task| tasks.borrow_mut().push(task)) as super::IdleScheduler
        };
        let deferred = super::defer_queue_refresh_with(callback, schedule);

        deferred();
        deferred();
        assert_eq!(refreshes.get(), 0, "the queue-change frame stays free");
        assert_eq!(tasks.borrow().len(), 1, "only one idle is pending");

        tasks.borrow_mut().remove(0)();
        assert_eq!(refreshes.get(), 1, "one idle refresh serves both changes");

        deferred();
        assert_eq!(tasks.borrow().len(), 1);
        tasks.borrow_mut().remove(0)();
        assert_eq!(refreshes.get(), 2, "the pending flag resets after the idle");
    }

    #[test]
    fn deferred_now_playing_refresh_reports_the_callback_cost_from_inside_the_idle() {
        // The capture goes through the crate-wide helper rather than a
        // subscriber of its own: a thread-local subscriber leaves `tracing`
        // free to resolve this module's callsite against a thread that has
        // none — the sibling test above reaches the very same callsite — and
        // the buffer then stays empty for the rest of the process.
        let logs = CapturedLogs::default();
        let tasks = Rc::new(RefCell::new(Vec::<super::DeferredTask>::new()));
        let schedule = {
            let tasks = tasks.clone();
            Rc::new(move |task| tasks.borrow_mut().push(task)) as super::IdleScheduler
        };
        let deferred = super::defer_queue_refresh_with(
            Rc::new(|| std::thread::sleep(std::time::Duration::from_millis(5))),
            schedule,
        );

        deferred();
        logs.capture(|| tasks.borrow_mut().remove(0)());

        let output = logs.text();
        assert!(
            output.contains("reprise::ui::playback"),
            "captured: {output:?}"
        );
        assert!(output.contains("queue listeners deferred"));
        let elapsed = output
            .split("now_playing_deferred_ms=")
            .nth(1)
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u128>().ok())
            .expect("deferred callback timing field");
        assert!(elapsed >= 4, "the timing must include the slow callback");
    }
}
