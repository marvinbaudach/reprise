//! Live ListenBrainz worker runtime. HTTP and queue draining stay on a
//! dedicated thread; only immutable status values cross back to GTK.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use gtk4::glib;
use reprise_core::scrobbling::{
    self, ListenBrainzClient, ScrobblerTransport, TrackMetadata, TransportError,
};
use rusqlite::Connection;

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);
const SMOKE_API_ROOT_ENV: &str = "REPRISE_SMOKE_LISTENBRAINZ_API_ROOT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConnectionStatus {
    Disabled,
    Connecting,
    Connected { user_name: String, pending: usize },
    Offline { pending: usize },
    Unauthorized,
    Error { pending: usize },
}

#[derive(Debug, thiserror::Error)]
enum FlushError {
    #[error("queue error: {0}")]
    Queue(#[from] scrobbling::QueueError),
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("durable queue returned a row without its local id")]
    MissingQueueId,
}

enum WorkerCommand {
    PlayingNow(TrackMetadata),
    Flush,
    Stop,
}

struct WorkerControl {
    sender: mpsc::Sender<WorkerCommand>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
struct WorkerCoordination<'a> {
    drain_lock: &'a Mutex<()>,
    cancelled: &'a AtomicBool,
}

type StatusCallback = Rc<dyn Fn(ConnectionStatus)>;

pub(super) struct ListenBrainzRuntime {
    database_path: PathBuf,
    generation: Cell<u64>,
    active: Cell<bool>,
    command: RefCell<Option<WorkerControl>>,
    status: RefCell<ConnectionStatus>,
    subscribers: RefCell<Vec<StatusCallback>>,
    drain_lock: Arc<Mutex<()>>,
    smoke_api_root: Option<String>,
}

impl ListenBrainzRuntime {
    pub(super) fn new(database_path: PathBuf) -> Rc<Self> {
        let smoke_api_root = smoke_api_root();
        Rc::new(Self {
            database_path,
            generation: Cell::new(0),
            active: Cell::new(false),
            command: RefCell::new(None),
            status: RefCell::new(ConnectionStatus::Disabled),
            subscribers: RefCell::new(Vec::new()),
            drain_lock: Arc::new(Mutex::new(())),
            smoke_api_root,
        })
    }

    pub(super) fn is_active(&self) -> bool {
        self.active.get()
    }

    pub(super) fn smoke_api_is_local(&self) -> bool {
        self.smoke_api_root.is_some()
    }

    pub(super) fn status(&self) -> ConnectionStatus {
        self.status.borrow().clone()
    }

    pub(super) fn subscribe(&self, callback: StatusCallback) {
        let status = self.status();
        callback(status);
        self.subscribers.borrow_mut().push(callback);
    }

    pub(super) fn configure(self: &Rc<Self>, token: String) {
        self.stop_worker();
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.active.set(true);
        self.set_status(&ConnectionStatus::Connecting);

        let (command_sender, command_receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (status_sender, status_receiver) = async_channel::unbounded();
        self.command.borrow_mut().replace(WorkerControl {
            sender: command_sender,
            cancelled: cancelled.clone(),
        });
        let database_path = self.database_path.clone();
        let drain_lock = self.drain_lock.clone();
        let client = self
            .smoke_api_root
            .as_deref()
            .map(ListenBrainzClient::with_api_root)
            .unwrap_or_default();
        std::thread::spawn(move || {
            run_worker(
                &database_path,
                &token,
                &command_receiver,
                &status_sender,
                generation,
                &client,
                WorkerCoordination {
                    drain_lock: &drain_lock,
                    cancelled: &cancelled,
                },
            );
        });

        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok((message_generation, status)) = status_receiver.recv().await {
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                if status_is_current(runtime.generation.get(), message_generation) {
                    runtime.set_status(&status);
                }
            }
        });
    }

    pub(super) fn disable(&self) {
        self.active.set(false);
        self.generation.set(self.generation.get().wrapping_add(1));
        self.stop_worker();
        self.set_status(&ConnectionStatus::Disabled);
    }

    pub(super) fn playing_now(&self, track: TrackMetadata) {
        if !self.active.get() {
            return;
        }
        self.send(WorkerCommand::PlayingNow(track));
    }

    pub(super) fn flush(&self) {
        if self.active.get() {
            self.send(WorkerCommand::Flush);
        }
    }

    pub(super) fn report_status(&self, status: &ConnectionStatus) {
        self.set_status(status);
    }

    fn send(&self, command: WorkerCommand) {
        let sender = self
            .command
            .borrow()
            .as_ref()
            .map(|control| control.sender.clone());
        if let Some(sender) = sender {
            if sender.send(command).is_err() {
                tracing::warn!("ListenBrainz worker is unavailable");
            }
        }
    }

    fn stop_worker(&self) {
        let control = self.command.borrow_mut().take();
        if let Some(control) = control {
            control.cancelled.store(true, Ordering::Release);
            let _ = control.sender.send(WorkerCommand::Stop);
        }
    }

    fn set_status(&self, status: &ConnectionStatus) {
        *self.status.borrow_mut() = status.clone();
        let callbacks = self.subscribers.borrow().clone();
        for callback in callbacks {
            callback(status.clone());
        }
    }
}

impl Drop for ListenBrainzRuntime {
    fn drop(&mut self) {
        if let Some(control) = self.command.get_mut().take() {
            control.cancelled.store(true, Ordering::Release);
            let _ = control.sender.send(WorkerCommand::Stop);
        }
    }
}

fn status_is_current(current_generation: u64, message_generation: u64) -> bool {
    current_generation == message_generation
}

fn is_loopback_smoke_root(value: &str) -> bool {
    ["http://127.0.0.1:", "http://[::1]:"]
        .into_iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .and_then(|remainder| remainder.split('/').next())
        .is_some_and(|port| port.parse::<u16>().is_ok())
}

fn smoke_api_root() -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let value = std::env::var(SMOKE_API_ROOT_ENV).ok()?;
    if is_loopback_smoke_root(&value) {
        Some(value)
    } else {
        tracing::warn!("ignored non-loopback ListenBrainz smoke API root");
        None
    }
}

fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_BACKOFF)
}

fn wait_for_retry(
    receiver: &mpsc::Receiver<WorkerCommand>,
    backoff: &mut Duration,
    deferred_playing_now: &mut Option<TrackMetadata>,
) -> bool {
    let result = receiver.recv_timeout(*backoff);
    *backoff = next_backoff(*backoff);
    match result {
        Ok(WorkerCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => false,
        Ok(WorkerCommand::PlayingNow(track)) => {
            *deferred_playing_now = Some(track);
            true
        }
        Ok(WorkerCommand::Flush) | Err(mpsc::RecvTimeoutError::Timeout) => true,
    }
}

fn flush_pending<T: ScrobblerTransport>(
    conn: &Connection,
    transport: &T,
    token: &str,
) -> Result<usize, FlushError> {
    loop {
        let listens = scrobbling::pending(conn, 1_000)?;
        if listens.is_empty() {
            return scrobbling::pending_count(conn).map_err(FlushError::from);
        }
        transport.submit(token, &listens)?;
        let ids = listens
            .iter()
            .filter_map(|listen| listen.id)
            .collect::<Vec<_>>();
        if ids.len() != listens.len() {
            return Err(FlushError::MissingQueueId);
        }
        scrobbling::acknowledge(conn, &ids)?;
    }
}

fn publish(
    sender: &async_channel::Sender<(u64, ConnectionStatus)>,
    generation: u64,
    status: ConnectionStatus,
) {
    let _ = sender.try_send((generation, status));
}

fn pending_or_zero(conn: &Connection) -> usize {
    scrobbling::pending_count(conn).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not count pending ListenBrainz listens");
        0
    })
}

fn run_worker<T: ScrobblerTransport>(
    database_path: &Path,
    token: &str,
    receiver: &mpsc::Receiver<WorkerCommand>,
    status_sender: &async_channel::Sender<(u64, ConnectionStatus)>,
    generation: u64,
    transport: &T,
    coordination: WorkerCoordination<'_>,
) {
    let conn = match reprise_core::db::open(Some(database_path))
        .and_then(|conn| reprise_core::db::migrate(&conn).map(|()| conn))
    {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(%error, "could not open ListenBrainz queue");
            publish(
                status_sender,
                generation,
                ConnectionStatus::Error { pending: 0 },
            );
            return;
        }
    };

    let mut user_name = None;
    let mut backoff = INITIAL_BACKOFF;
    let mut deferred_playing_now = None;
    loop {
        if coordination.cancelled.load(Ordering::Acquire) {
            return;
        }
        if user_name.is_none() {
            publish(status_sender, generation, ConnectionStatus::Connecting);
            match transport.validate_token(token) {
                Ok(user) => {
                    user_name = Some(user);
                    if coordination.cancelled.load(Ordering::Acquire) {
                        return;
                    }
                }
                Err(TransportError::Unauthorized) => {
                    publish(status_sender, generation, ConnectionStatus::Unauthorized);
                    return;
                }
                Err(TransportError::Rejected(_)) => {
                    publish(
                        status_sender,
                        generation,
                        ConnectionStatus::Error {
                            pending: pending_or_zero(&conn),
                        },
                    );
                    return;
                }
                Err(error) => {
                    tracing::warn!(%error, "ListenBrainz validation deferred");
                    publish(
                        status_sender,
                        generation,
                        ConnectionStatus::Offline {
                            pending: pending_or_zero(&conn),
                        },
                    );
                    if !wait_for_retry(receiver, &mut backoff, &mut deferred_playing_now) {
                        return;
                    }
                    continue;
                }
            }
        }

        if let Some(track) = deferred_playing_now.take() {
            if coordination.cancelled.load(Ordering::Acquire) {
                return;
            }
            if let Err(error) = transport.playing_now(token, &track) {
                if !handle_transport_error(&conn, status_sender, generation, error, &mut user_name)
                {
                    return;
                }
                if !wait_for_retry(receiver, &mut backoff, &mut deferred_playing_now) {
                    return;
                }
                continue;
            }
        }

        if coordination.cancelled.load(Ordering::Acquire) {
            return;
        }
        let flush_result = {
            let _guard = coordination
                .drain_lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            flush_pending(&conn, transport, token)
        };
        match flush_result {
            Ok(pending) => {
                backoff = INITIAL_BACKOFF;
                publish(
                    status_sender,
                    generation,
                    ConnectionStatus::Connected {
                        user_name: user_name.clone().unwrap_or_default(),
                        pending,
                    },
                );
            }
            Err(FlushError::Transport(error)) => {
                if !handle_transport_error(&conn, status_sender, generation, error, &mut user_name)
                {
                    return;
                }
                if !wait_for_retry(receiver, &mut backoff, &mut deferred_playing_now) {
                    return;
                }
                continue;
            }
            Err(FlushError::Queue(error)) => {
                tracing::warn!(%error, "could not drain ListenBrainz queue");
                publish(
                    status_sender,
                    generation,
                    ConnectionStatus::Error {
                        pending: pending_or_zero(&conn),
                    },
                );
                return;
            }
            Err(FlushError::MissingQueueId) => {
                tracing::warn!("ListenBrainz queue returned a row without an id");
                publish(
                    status_sender,
                    generation,
                    ConnectionStatus::Error {
                        pending: pending_or_zero(&conn),
                    },
                );
                return;
            }
        }

        match receiver.recv() {
            Ok(WorkerCommand::PlayingNow(track)) => {
                if coordination.cancelled.load(Ordering::Acquire) {
                    return;
                }
                if let Err(error) = transport.playing_now(token, &track) {
                    if !handle_transport_error(
                        &conn,
                        status_sender,
                        generation,
                        error,
                        &mut user_name,
                    ) {
                        return;
                    }
                    if !wait_for_retry(receiver, &mut backoff, &mut deferred_playing_now) {
                        return;
                    }
                }
            }
            Ok(WorkerCommand::Flush) => {}
            Ok(WorkerCommand::Stop) | Err(_) => return,
        }
    }
}

fn handle_transport_error(
    conn: &Connection,
    status_sender: &async_channel::Sender<(u64, ConnectionStatus)>,
    generation: u64,
    error: TransportError,
    user_name: &mut Option<String>,
) -> bool {
    match error {
        TransportError::Unauthorized => {
            publish(status_sender, generation, ConnectionStatus::Unauthorized);
            false
        }
        TransportError::Rejected(_) | TransportError::InvalidMetadata(_) => {
            publish(
                status_sender,
                generation,
                ConnectionStatus::Error {
                    pending: pending_or_zero(conn),
                },
            );
            false
        }
        TransportError::Retryable(_)
        | TransportError::Network
        | TransportError::InvalidResponse => {
            *user_name = None;
            publish(
                status_sender,
                generation,
                ConnectionStatus::Offline {
                    pending: pending_or_zero(conn),
                },
            );
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use reprise_core::scrobbling::{Listen, ScrobblerTransport, TrackMetadata, TransportError};

    struct FakeTransport {
        validation: Result<String, TransportError>,
        result: Result<(), TransportError>,
        submitted: Arc<Mutex<Vec<Listen>>>,
    }

    impl ScrobblerTransport for FakeTransport {
        fn validate_token(&self, _token: &str) -> Result<String, TransportError> {
            self.validation.clone()
        }

        fn playing_now(&self, _token: &str, _track: &TrackMetadata) -> Result<(), TransportError> {
            Ok(())
        }

        fn submit(&self, _token: &str, listens: &[Listen]) -> Result<(), TransportError> {
            self.submitted.lock().unwrap().extend_from_slice(listens);
            self.result
        }
    }

    fn queued_conn() -> rusqlite::Connection {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        for listened_at in [1, 2] {
            reprise_core::scrobbling::enqueue(
                &conn,
                &Listen {
                    id: None,
                    listened_at,
                    track: TrackMetadata {
                        artist_name: "Artist".to_string(),
                        track_name: format!("Track {listened_at}"),
                        release_name: None,
                        duration_ms: 120_000,
                    },
                },
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn successful_flush_submits_fifo_and_acknowledges_rows() {
        let conn = queued_conn();
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            validation: Ok("tester".to_string()),
            result: Ok(()),
            submitted: submitted.clone(),
        };
        assert_eq!(flush_pending(&conn, &transport, "token").unwrap(), 0);
        assert_eq!(
            submitted
                .lock()
                .unwrap()
                .iter()
                .map(|listen| listen.listened_at)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 0);
    }

    #[test]
    fn failed_flush_preserves_every_row() {
        let conn = queued_conn();
        let transport = FakeTransport {
            validation: Ok("tester".to_string()),
            result: Err(TransportError::Retryable(503)),
            submitted: Arc::new(Mutex::new(Vec::new())),
        };
        assert!(matches!(
            flush_pending(&conn, &transport, "token"),
            Err(FlushError::Transport(TransportError::Retryable(503)))
        ));
        assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 2);
    }

    #[test]
    fn unauthorized_flush_preserves_rows_for_new_credentials() {
        let conn = queued_conn();
        let transport = FakeTransport {
            validation: Ok("tester".to_string()),
            result: Err(TransportError::Unauthorized),
            submitted: Arc::new(Mutex::new(Vec::new())),
        };
        assert!(matches!(
            flush_pending(&conn, &transport, "token"),
            Err(FlushError::Transport(TransportError::Unauthorized))
        ));
        assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 2);
    }

    #[test]
    fn retry_backoff_doubles_and_caps_at_five_minutes() {
        assert_eq!(
            next_backoff(std::time::Duration::from_secs(5)).as_secs(),
            10
        );
        assert_eq!(
            next_backoff(std::time::Duration::from_secs(240)).as_secs(),
            300
        );
        assert_eq!(
            next_backoff(std::time::Duration::from_secs(300)).as_secs(),
            300
        );
    }

    #[test]
    fn new_work_wakes_backoff_and_preserves_latest_playing_now() {
        let (sender, receiver) = mpsc::channel();
        let track = TrackMetadata {
            artist_name: "Artist".to_string(),
            track_name: "Track".to_string(),
            release_name: None,
            duration_ms: 60_000,
        };
        sender
            .send(WorkerCommand::PlayingNow(track.clone()))
            .unwrap();
        let mut backoff = INITIAL_BACKOFF;
        let mut deferred = None;
        assert!(wait_for_retry(&receiver, &mut backoff, &mut deferred));
        assert_eq!(deferred, Some(track));
        assert_eq!(backoff, Duration::from_secs(10));

        sender.send(WorkerCommand::Stop).unwrap();
        assert!(!wait_for_retry(&receiver, &mut backoff, &mut deferred));
    }

    #[test]
    fn only_current_generation_may_update_visible_status() {
        assert!(status_is_current(7, 7));
        assert!(!status_is_current(7, 6));
        assert!(!status_is_current(7, 8));
    }

    #[test]
    fn smoke_api_override_accepts_only_explicit_loopback_http_ports() {
        assert!(is_loopback_smoke_root("http://127.0.0.1:8123"));
        assert!(is_loopback_smoke_root("http://[::1]:8123/api"));
        assert!(!is_loopback_smoke_root("https://api.listenbrainz.org"));
        assert!(!is_loopback_smoke_root("http://127.0.0.1"));
        assert!(!is_loopback_smoke_root("http://example.test:8123"));
    }

    #[test]
    fn worker_validates_flushes_and_stops_on_command() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("worker.db");
        {
            let source = queued_conn();
            let destination = reprise_core::db::open(Some(&path)).unwrap();
            reprise_core::db::migrate(&destination).unwrap();
            for listen in reprise_core::scrobbling::pending(&source, 100).unwrap() {
                reprise_core::scrobbling::enqueue(&destination, &listen).unwrap();
            }
        }
        let (command_sender, command_receiver) = mpsc::channel();
        let (status_sender, status_receiver) = async_channel::unbounded();
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let worker_submitted = submitted.clone();
        let worker_path = path.clone();
        let handle = std::thread::spawn(move || {
            run_worker(
                &worker_path,
                "token",
                &command_receiver,
                &status_sender,
                42,
                &FakeTransport {
                    validation: Ok("tester".to_string()),
                    result: Ok(()),
                    submitted: worker_submitted,
                },
                WorkerCoordination {
                    drain_lock: &Mutex::new(()),
                    cancelled: &AtomicBool::new(false),
                },
            );
        });

        loop {
            let (generation, status) = status_receiver.recv_blocking().unwrap();
            assert_eq!(generation, 42);
            if status
                == (ConnectionStatus::Connected {
                    user_name: "tester".to_string(),
                    pending: 0,
                })
            {
                break;
            }
        }
        command_sender.send(WorkerCommand::Stop).unwrap();
        handle.join().unwrap();
        assert_eq!(submitted.lock().unwrap().len(), 2);
        let conn = reprise_core::db::open(Some(&path)).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 0);
    }

    #[test]
    fn unauthorized_worker_stops_without_deleting_offline_queue() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("worker.db");
        {
            let conn = reprise_core::db::open(Some(&path)).unwrap();
            reprise_core::db::migrate(&conn).unwrap();
            let source = queued_conn();
            for listen in reprise_core::scrobbling::pending(&source, 100).unwrap() {
                reprise_core::scrobbling::enqueue(&conn, &listen).unwrap();
            }
        }
        let (_command_sender, command_receiver) = mpsc::channel();
        let (status_sender, status_receiver) = async_channel::unbounded();
        run_worker(
            &path,
            "bad-token",
            &command_receiver,
            &status_sender,
            9,
            &FakeTransport {
                validation: Err(TransportError::Unauthorized),
                result: Ok(()),
                submitted: Arc::new(Mutex::new(Vec::new())),
            },
            WorkerCoordination {
                drain_lock: &Mutex::new(()),
                cancelled: &AtomicBool::new(false),
            },
        );
        let mut statuses = Vec::new();
        while let Ok(status) = status_receiver.try_recv() {
            statuses.push(status);
        }
        assert!(statuses.contains(&(9, ConnectionStatus::Unauthorized)));
        let conn = reprise_core::db::open(Some(&path)).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 2);
    }

    #[test]
    fn cancelled_worker_performs_no_network_or_status_work() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cancelled.db");
        let conn = reprise_core::db::open(Some(&path)).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        drop(conn);
        let (_command_sender, command_receiver) = mpsc::channel();
        let (status_sender, status_receiver) = async_channel::unbounded();
        let cancelled = AtomicBool::new(true);
        run_worker(
            &path,
            "unused-token",
            &command_receiver,
            &status_sender,
            10,
            &FakeTransport {
                validation: Err(TransportError::Unauthorized),
                result: Ok(()),
                submitted: Arc::new(Mutex::new(Vec::new())),
            },
            WorkerCoordination {
                drain_lock: &Mutex::new(()),
                cancelled: &cancelled,
            },
        );
        assert!(status_receiver.try_recv().is_err());
    }
}
