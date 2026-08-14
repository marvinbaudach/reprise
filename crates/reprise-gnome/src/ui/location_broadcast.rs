use std::cell::{Cell, RefCell};
use std::rc::Rc;

type IsAlive = Rc<dyn Fn() -> bool>;
type OnLocationChanged = Rc<dyn Fn()>;

#[derive(Clone)]
struct Subscriber {
    id: u64,
    is_alive: IsAlive,
    #[cfg_attr(not(test), allow(dead_code))]
    callback: OnLocationChanged,
}

/// Process-local announcement that the app-wide location or default radius changed.
///
/// This is deliberately independent of every optional module runtime: location is
/// app state, so listeners still receive updates while Concerts or online sources
/// are disabled.
#[derive(Default)]
pub(in crate::ui) struct LocationBroadcast {
    next_id: Cell<u64>,
    entries: RefCell<Vec<Subscriber>>,
}

impl LocationBroadcast {
    pub(in crate::ui) fn subscribe(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn() + 'static,
    ) {
        self.prune();
        let is_alive: IsAlive = Rc::new(is_alive);
        if !is_alive() {
            return;
        }
        let id = self.next_id.get().wrapping_add(1);
        self.next_id.set(id);
        self.entries.borrow_mut().push(Subscriber {
            id,
            is_alive,
            callback: Rc::new(callback),
        });
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::ui) fn notify(&self) {
        self.prune();
        let entries = self.entries.borrow().clone();
        for entry in entries {
            if (entry.is_alive)() {
                (entry.callback)();
            }
        }
        self.prune();
    }

    fn prune(&self) {
        let entries = self.entries.borrow().clone();
        let dead = entries
            .iter()
            .filter_map(|entry| (!(entry.is_alive)()).then_some(entry.id))
            .collect::<Vec<_>>();
        if dead.is_empty() {
            return;
        }
        self.entries
            .borrow_mut()
            .retain(|entry| !dead.contains(&entry.id));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::LocationBroadcast;

    #[test]
    fn location_broadcast_notifies_live_subscribers_without_a_module_gate() {
        let broadcast = LocationBroadcast::default();
        let live = Rc::new(Cell::new(true));
        let calls = Rc::new(Cell::new(0));
        broadcast.subscribe(
            {
                let live = live.clone();
                move || live.get()
            },
            {
                let calls = calls.clone();
                move || calls.set(calls.get() + 1)
            },
        );

        broadcast.notify();
        assert_eq!(calls.get(), 1);

        live.set(false);
        broadcast.notify();
        assert_eq!(calls.get(), 1);
    }
}
