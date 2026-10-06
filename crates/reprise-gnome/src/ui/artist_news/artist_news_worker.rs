use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reprise_core::db::Db;

type IsAlive = Rc<dyn Fn() -> bool>;
type OnEnabled = Rc<dyn Fn(bool)>;

#[derive(Clone)]
struct EnabledSubscriber {
    id: u64,
    is_alive: IsAlive,
    callback: OnEnabled,
}

#[derive(Default)]
struct EnabledSubscribers {
    next_id: Cell<u64>,
    entries: RefCell<Vec<EnabledSubscriber>>,
}

impl EnabledSubscribers {
    fn subscribe(
        &self,
        current: bool,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(bool) + 'static,
    ) {
        self.prune();
        let is_alive: IsAlive = Rc::new(is_alive);
        if !is_alive() {
            return;
        }
        let callback: OnEnabled = Rc::new(callback);
        callback(current);
        if !is_alive() {
            return;
        }
        let id = self.next_id.get().wrapping_add(1);
        self.next_id.set(id);
        self.entries.borrow_mut().push(EnabledSubscriber {
            id,
            is_alive,
            callback,
        });
    }

    fn notify(&self, enabled: bool) {
        self.prune();
        let entries = self.entries.borrow().clone();
        for entry in entries {
            if (entry.is_alive)() {
                (entry.callback)(enabled);
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

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

pub(in crate::ui) struct ArtistNewsRuntime {
    pub enabled: Rc<Cell<bool>>,
    subscribers: EnabledSubscribers,
}

impl ArtistNewsRuntime {
    pub(in crate::ui) fn setup(conn: &Db) -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(
                reprise_core::online_sources::network_allowed_or_off(
                    conn,
                    &reprise_core::modules::NEW_RELEASES_MODULE,
                ),
            )),
            subscribers: EnabledSubscribers::default(),
        })
    }

    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &Db,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(
            conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
            enabled,
        )?;
        self.recompute_enabled(conn);
        Ok(())
    }

    /// `NET-1a`: re-derives `enabled` from the global online-sources gate.
    pub(in crate::ui) fn recompute_enabled(&self, conn: &Db) {
        let enabled = reprise_core::online_sources::network_allowed_or_off(
            conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
        );
        if self.enabled.replace(enabled) != enabled {
            self.subscribers.notify(enabled);
        }
    }

    pub(in crate::ui) fn subscribe_enabled(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(bool) + 'static,
    ) {
        self.subscribers
            .subscribe(self.enabled.get(), is_alive, callback);
    }

    #[cfg(test)]
    fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn migrated_conn() -> Db {
        crate::test_db::open().unwrap()
    }

    #[test]
    fn runtime_defaults_off() {
        let runtime = ArtistNewsRuntime::setup(&migrated_conn());
        assert!(!runtime.enabled.get());
    }

    #[test]
    fn runtime_activation_persists_and_updates_live_state() {
        let conn = migrated_conn();
        let runtime = ArtistNewsRuntime::setup(&conn);
        runtime.set_enabled(&conn, true).unwrap();
        assert!(runtime.enabled.get());
        assert!(reprise_core::modules::is_enabled(
            &conn,
            &reprise_core::modules::NEW_RELEASES_MODULE
        )
        .unwrap());
    }

    #[test]
    fn net_1a_recompute_enabled_reflects_the_global_gate() {
        let conn = migrated_conn();
        let runtime = ArtistNewsRuntime::setup(&conn);
        runtime.set_enabled(&conn, true).unwrap();
        assert!(runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, false).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(!runtime.enabled.get());

        reprise_core::online_sources::set_enabled(&conn, true).unwrap();
        runtime.recompute_enabled(&conn);
        assert!(runtime.enabled.get());
    }

    #[test]
    fn dead_enabled_subscriber_is_removed_safely() {
        let conn = migrated_conn();
        let runtime = ArtistNewsRuntime::setup(&conn);
        let alive = Rc::new(Cell::new(true));
        let calls = Rc::new(Cell::new(0));
        runtime.subscribe_enabled(
            {
                let alive = alive.clone();
                move || alive.get()
            },
            {
                let calls = calls.clone();
                move |_| calls.set(calls.get() + 1)
            },
        );
        assert_eq!(calls.get(), 1);
        alive.set(false);
        runtime.set_enabled(&conn, true).unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(runtime.subscriber_count(), 0);
    }
}
