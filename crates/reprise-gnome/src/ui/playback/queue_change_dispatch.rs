//! Queue-change fan-out, including the measured slow Now Playing refresh.

use std::cell::Cell;
use std::rc::Rc;

use super::player_controller::PlayerController;

fn register_queue_listener(
    callbacks: &std::cell::RefCell<Vec<super::instrumentation::QueueListener>>,
    name: &'static str,
    callback: Rc<dyn Fn()>,
) {
    callbacks
        .borrow_mut()
        .push(super::instrumentation::QueueListener::new(name, callback));
}

fn register_deferred_queue_listener_with(
    callbacks: &std::cell::RefCell<Vec<super::instrumentation::QueueListener>>,
    callback: Rc<dyn Fn()>,
    schedule_idle: super::instrumentation::IdleScheduler,
) {
    register_queue_listener(
        callbacks,
        super::instrumentation::NOW_PLAYING_LISTENER,
        super::instrumentation::defer_queue_refresh_with(callback, schedule_idle),
    );
}

pub(super) fn clear_removed_prefed_next(
    prefed_next: &Cell<Option<i64>>,
    removed_ids: &[i64],
    clear_backend: impl FnOnce(),
) -> bool {
    if !prefed_next
        .get()
        .is_some_and(|id| removed_ids.contains(&id))
    {
        return false;
    }
    prefed_next.set(None);
    clear_backend();
    true
}

impl PlayerController {
    #[track_caller]
    pub(in crate::ui) fn add_on_queue_changed(&self, callback: impl Fn() + 'static) {
        register_queue_listener(
            &self.queue_changed,
            std::panic::Location::caller().file(),
            Rc::new(callback),
        );
    }

    pub(in crate::ui) fn add_on_queue_changed_deferred(&self, callback: impl Fn() + 'static) {
        register_deferred_queue_listener_with(
            &self.queue_changed,
            Rc::new(callback),
            Rc::new(|task| {
                gtk4::glib::idle_add_local_once(task);
            }),
        );
    }

    pub(super) fn clear_prefed_next_if_removed(&self, ids: &[i64]) {
        clear_removed_prefed_next(&self.prefed_next_track, ids, || {
            self.player.set_next(None);
        });
    }

    pub(in crate::ui) fn notify_queue_changed(&self) {
        let up_next_len = self.up_next.borrow().len();
        let ((), mirror_ms) = super::instrumentation::timed(|| self.update_agent_queue_mirror());
        let callbacks = self.queue_changed.borrow().clone();
        let listener_times = super::instrumentation::time_queue_listeners(callbacks);
        // Measurements kept the gapless pre-feed synchronous. Every caller
        // holds no live queue borrow across this short operation.
        let ((), feed_ms) = super::instrumentation::timed(|| self.feed_next());
        tracing::info!(
            up_next_len,
            mirror_ms,
            listeners_ms = listener_times.total_ms,
            synchronous_listeners_ms = listener_times.synchronous_ms,
            now_playing_ms = listener_times.now_playing_ms,
            feed_ms,
            "up next changed"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn deferred_registration_wraps_that_listener_after_any_number_of_earlier_listeners() {
        let callbacks = RefCell::new(Vec::new());
        let synchronous_calls = Rc::new(Cell::new(0));
        for _ in 0..4 {
            let synchronous_calls = synchronous_calls.clone();
            super::register_queue_listener(
                &callbacks,
                "synchronous",
                Rc::new(move || synchronous_calls.set(synchronous_calls.get() + 1)),
            );
        }
        let deferred_calls = Rc::new(Cell::new(0));
        let tasks = Rc::new(RefCell::new(Vec::<
            super::super::instrumentation::DeferredTask,
        >::new()));
        let schedule = {
            let tasks = tasks.clone();
            Rc::new(move |task| tasks.borrow_mut().push(task))
                as super::super::instrumentation::IdleScheduler
        };
        let callback = {
            let deferred_calls = deferred_calls.clone();
            Rc::new(move || deferred_calls.set(deferred_calls.get() + 1)) as Rc<dyn Fn()>
        };
        super::register_deferred_queue_listener_with(&callbacks, callback, schedule);

        for listener in callbacks.borrow().iter() {
            listener.call();
        }

        assert_eq!(synchronous_calls.get(), 4);
        assert_eq!(deferred_calls.get(), 0, "only the explicit listener defers");
        assert_eq!(tasks.borrow().len(), 1);
        tasks.borrow_mut().remove(0)();
        assert_eq!(deferred_calls.get(), 1);
    }
}
