//! Small synchronous phase timer shared by playback instrumentation.

use std::rc::Rc;

pub(super) type DeferredTask = Box<dyn FnOnce()>;
pub(super) type IdleScheduler = Rc<dyn Fn(DeferredTask)>;

pub(super) const NOW_PLAYING_LISTENER: &str = "now_playing";

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
    pub(super) now_playing_ms: u128,
    pub(super) total_ms: u128,
}

pub(super) fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    let started = std::time::Instant::now();
    let result = operation();
    (result, started.elapsed().as_millis())
}

pub(super) fn time_queue_listeners(callbacks: Vec<QueueListener>) -> QueueListenerTimes {
    let mut synchronous_ms = 0;
    let mut now_playing_ms = 0;
    let mut total_ms = 0;
    for listener in callbacks {
        let ((), elapsed_ms) = timed(|| listener.call());
        tracing::info!(
            listener = listener.name,
            elapsed_ms,
            "queue listener completed"
        );
        total_ms += elapsed_ms;
        if listener.name == NOW_PLAYING_LISTENER {
            now_playing_ms += elapsed_ms;
        } else {
            synchronous_ms += elapsed_ms;
        }
    }
    QueueListenerTimes {
        synchronous_ms,
        now_playing_ms,
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
            callback();
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
}
