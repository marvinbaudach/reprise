#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::concerts::{
    self, BandsintownProvider, ConcertError, EventProvider, RefreshSummary, TicketmasterProvider,
};

pub(in crate::ui) struct ConcertsRequest {
    pub generation: u64,
    pub force: bool,
    pub response: async_channel::Sender<ConcertsResponse>,
}

#[derive(Debug)]
pub(in crate::ui) struct ConcertsResponse {
    pub generation: u64,
    pub result: Result<RefreshSummary, ConcertError>,
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

pub(in crate::ui) struct ConcertsRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker: async_channel::Sender<ConcertsRequest>,
    subscribers: EnabledSubscribers,
    jitter_seconds: i64,
}

impl ConcertsRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        let enabled =
            reprise_core::modules::is_enabled(conn, &reprise_core::modules::CONCERTS_MODULE)
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        %error,
                        "could not read Concerts module state; defaulting to off"
                    );
                    false
                });
        let database_path = database_path(conn);
        let seed = database_path.as_deref().map_or_else(
            || "memory".into(),
            |path| path.to_string_lossy().into_owned(),
        );
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            worker: spawn(database_path),
            subscribers: EnabledSubscribers::default(),
            jitter_seconds: concerts::jitter_seconds(&seed),
        })
    }

    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::CONCERTS_MODULE, enabled)?;
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

    pub(in crate::ui) fn request(&self, request: ConcertsRequest) -> bool {
        if !self.enabled.get() {
            return false;
        }
        match self.worker.try_send(request) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not queue Concerts request");
                false
            }
        }
    }

    pub(in crate::ui) fn jitter_seconds(&self) -> i64 {
        self.jitter_seconds
    }

    #[cfg(test)]
    fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

pub(super) fn request_allowed(enabled: bool, fetching: bool, due: bool) -> bool {
    enabled && !fetching && due
}

fn database_path(conn: &rusqlite::Connection) -> Option<PathBuf> {
    let mut statement = conn.prepare("PRAGMA database_list").ok()?;
    let mut rows = statement.query([]).ok()?;
    while let Some(row) = rows.next().ok()? {
        let name = row.get::<_, String>(1).ok()?;
        let path = row.get::<_, String>(2).ok()?;
        if name == "main" && !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn spawn(database_path: Option<PathBuf>) -> async_channel::Sender<ConcertsRequest> {
    let (sender, receiver) = async_channel::unbounded::<ConcertsRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-concerts".into())
        .spawn(move || {
            let connection = database_path
                .as_deref()
                .map(|path| reprise_core::db::open_migrated(Some(path)));
            while let Ok(request) = receiver.recv_blocking() {
                let result = match connection.as_ref() {
                    Some(Ok(conn)) => refresh_configured(conn, request.force),
                    Some(Err(error)) => Err(ConcertError::InvalidData(error.to_string())),
                    None => Err(ConcertError::InvalidData(
                        "the active database has no persistent path".into(),
                    )),
                };
                let _ = request.response.try_send(ConcertsResponse {
                    generation: request.generation,
                    result,
                });
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start Concerts worker");
    }
    sender
}

fn refresh_configured(
    conn: &rusqlite::Connection,
    force: bool,
) -> Result<RefreshSummary, ConcertError> {
    let credentials = concerts::config::credentials(conn)?;
    let mut providers: Vec<Box<dyn EventProvider>> = Vec::new();
    if let Some(app_id) = credentials.bandsintown_app_id {
        providers.push(Box::new(BandsintownProvider::new(app_id)));
    }
    if let Some(api_key) = credentials.ticketmaster_api_key {
        providers.push(Box::new(TicketmasterProvider::new(api_key)));
    }
    concerts::refresh(
        conn,
        &providers,
        chrono::Local::now().date_naive(),
        chrono::Utc::now().timestamp(),
        force,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_conn() -> rusqlite::Connection {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn conc_5_only_enabled_due_idle_workers_fetch() {
        assert!(request_allowed(true, false, true));
        assert!(!request_allowed(false, false, true));
        assert!(!request_allowed(true, true, true));
        assert!(!request_allowed(true, false, false));
    }

    #[test]
    fn runtime_defaults_off_and_rejects_requests() {
        let runtime = ConcertsRuntime::setup(&migrated_conn());
        assert!(!runtime.enabled.get());
        let (response, _) = async_channel::bounded(1);
        assert!(!runtime.request(ConcertsRequest {
            generation: 1,
            force: false,
            response,
        }));
    }

    #[test]
    fn runtime_activation_persists_and_notifies_live_subscribers() {
        let conn = migrated_conn();
        let runtime = ConcertsRuntime::setup(&conn);
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
        runtime.set_enabled(&conn, true).unwrap();
        assert!(runtime.enabled.get());
        assert_eq!(calls.get(), 2);
        alive.set(false);
        runtime.set_enabled(&conn, false).unwrap();
        assert_eq!(runtime.subscriber_count(), 0);
    }
}
