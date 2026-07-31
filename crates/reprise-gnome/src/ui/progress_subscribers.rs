//! Progress fan-out shared by the background library batches (covers, lyrics).
//!
//! Every subscriber registers a liveness probe next to its callback: a surface
//! that has gone away stops receiving updates and its entry is pruned, so a
//! long-lived batch does not accumulate dead callbacks for the lifetime of the
//! process.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

type IsAlive = Rc<dyn Fn() -> bool>;
type OnProgress<P> = Rc<dyn Fn(P)>;

struct ProgressSubscriber<P> {
    id: u64,
    is_alive: IsAlive,
    callback: OnProgress<P>,
}

// A manual `Clone` — deriving it would demand `P: Clone`, although the payload
// only ever travels through `Rc<dyn Fn(P)>`.
impl<P> Clone for ProgressSubscriber<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            is_alive: self.is_alive.clone(),
            callback: self.callback.clone(),
        }
    }
}

pub(in crate::ui) struct ProgressSubscribers<P> {
    next_id: Cell<u64>,
    entries: RefCell<Vec<ProgressSubscriber<P>>>,
}

impl<P> Default for ProgressSubscribers<P> {
    fn default() -> Self {
        Self {
            next_id: Cell::new(0),
            entries: RefCell::new(Vec::new()),
        }
    }
}

impl<P: Copy> ProgressSubscribers<P> {
    /// Replays the current state to the new subscriber and keeps it only while
    /// its probe reports it alive — including across the replay itself, which
    /// can be what tears the surface down.
    pub(in crate::ui) fn subscribe(
        &self,
        current: P,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(P) + 'static,
    ) {
        self.prune();
        let is_alive: IsAlive = Rc::new(is_alive);
        if !is_alive() {
            return;
        }
        let callback: OnProgress<P> = Rc::new(callback);
        callback(current);
        if !is_alive() {
            return;
        }
        let id = self.next_id.get().wrapping_add(1);
        self.next_id.set(id);
        self.entries.borrow_mut().push(ProgressSubscriber {
            id,
            is_alive,
            callback,
        });
    }

    pub(in crate::ui) fn notify(&self, progress: P) {
        self.prune();
        let entries = self.entries.borrow().clone();
        for entry in entries {
            if (entry.is_alive)() {
                (entry.callback)(progress);
            }
        }
        self.prune();
    }

    fn prune(&self) {
        let entries = self.entries.borrow().clone();
        let dead: Vec<u64> = entries
            .iter()
            .filter_map(|entry| (!(entry.is_alive)()).then_some(entry.id))
            .collect();
        if dead.is_empty() {
            return;
        }
        self.entries
            .borrow_mut()
            .retain(|entry| !dead.contains(&entry.id));
    }

    #[cfg(test)]
    pub(in crate::ui) fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::ProgressSubscribers;

    #[test]
    fn multiple_progress_subscribers_receive_current_and_future_state() {
        let subscribers = ProgressSubscribers::default();
        let first = Rc::new(RefCell::new(Vec::new()));
        let second = Rc::new(RefCell::new(Vec::new()));

        for received in [&first, &second] {
            let received = received.clone();
            subscribers.subscribe(
                0_u32,
                || true,
                move |progress| {
                    received.borrow_mut().push(progress);
                },
            );
        }
        subscribers.notify(4);

        assert_eq!(*first.borrow(), vec![0, 4]);
        assert_eq!(*second.borrow(), vec![0, 4]);
    }

    #[test]
    fn dead_subscriber_is_removed_without_replaying_state_to_live_ones() {
        let subscribers = ProgressSubscribers::default();
        let calls = Rc::new(Cell::new(0));
        let alive = Rc::new(Cell::new(true));
        let calls_for_callback = calls.clone();
        let alive_for_probe = alive.clone();
        subscribers.subscribe(
            0_u32,
            move || alive_for_probe.get(),
            move |_| calls_for_callback.set(calls_for_callback.get() + 1),
        );

        alive.set(false);
        subscribers.subscribe(0, || true, |_| {});
        subscribers.notify(2);

        assert_eq!(calls.get(), 1);
        assert_eq!(subscribers.len(), 1);
    }

    #[test]
    fn subscriber_destroyed_by_initial_callback_is_not_retained() {
        let subscribers = ProgressSubscribers::default();
        let alive = Rc::new(Cell::new(true));
        let alive_for_probe = alive.clone();
        let alive_for_callback = alive.clone();
        subscribers.subscribe(
            0_u32,
            move || alive_for_probe.get(),
            move |_| alive_for_callback.set(false),
        );

        assert_eq!(subscribers.len(), 0);
    }
}
