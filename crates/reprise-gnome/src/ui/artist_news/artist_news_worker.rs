#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::artist_news::{ArtistNews, NewsError};

pub(in crate::ui) struct ArtistNewsRequest {
    pub generation: u64,
    pub artist: String,
    pub force: bool,
    pub response: async_channel::Sender<ArtistNewsResponse>,
}

#[derive(Debug)]
pub(in crate::ui) struct ArtistNewsResponse {
    pub generation: u64,
    pub result: Result<ArtistNews, NewsError>,
}

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
    worker: async_channel::Sender<ArtistNewsRequest>,
    subscribers: EnabledSubscribers,
}

impl ArtistNewsRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        let enabled = reprise_core::modules::is_enabled(
            conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read Artist News module state; defaulting to off");
            false
        });
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            worker: spawn(database_path(conn)),
            subscribers: EnabledSubscribers::default(),
        })
    }

    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(
            conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
            enabled,
        )?;
        if self.enabled.replace(enabled) != enabled {
            self.subscribers.notify(enabled);
        }
        Ok(())
    }

    pub(in crate::ui) fn subscribe_enabled(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(bool) + 'static,
    ) {
        self.subscribers
            .subscribe(self.enabled.get(), is_alive, callback);
    }

    pub(in crate::ui) fn request(&self, request: ArtistNewsRequest) {
        if !self.enabled.get() || request.artist.trim().is_empty() {
            return;
        }
        if let Err(error) = self.worker.try_send(request) {
            tracing::warn!(%error, "could not queue Artist News request");
        }
    }

    #[cfg(test)]
    fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

fn database_path(conn: &rusqlite::Connection) -> Option<PathBuf> {
    reprise_core::db::main_path(conn)
}

fn spawn(database_path: Option<PathBuf>) -> async_channel::Sender<ArtistNewsRequest> {
    let (sender, receiver) = async_channel::unbounded::<ArtistNewsRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-artist-news".into())
        .spawn(move || {
            let connection = database_path
                .as_deref()
                .map(|path| reprise_core::db::open_migrated(Some(path)));
            while let Ok(request) = receiver.recv_blocking() {
                let today = chrono::Local::now().date_naive();
                let result = match connection.as_ref() {
                    Some(Ok(conn)) => reprise_core::artist_news::configured_fetch_scope(conn)
                        .map_err(|error| NewsError::Database(error.to_string()))
                        .and_then(|scope| {
                            reprise_core::artist_news::refresh(
                                conn,
                                today,
                                scope,
                                request.force,
                                crate::ui::updates::release_cover::fallback_accent_for_artist,
                            )
                        })
                        .and_then(|_| {
                            reprise_core::artist_news::query_artist_news_by_name(
                                conn,
                                &request.artist,
                                today,
                            )
                            .map_err(|error| NewsError::Database(error.to_string()))?
                            .ok_or(NewsError::Unmatched)
                        }),
                    Some(Err(error)) => Err(NewsError::Database(error.to_string())),
                    None => Err(NewsError::Database(
                        "the active database has no persistent path".into(),
                    )),
                };
                let _ = request.response.try_send(ArtistNewsResponse {
                    generation: request.generation,
                    result,
                });
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start Artist News worker");
    }
    sender
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn migrated_conn() -> rusqlite::Connection {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn
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
