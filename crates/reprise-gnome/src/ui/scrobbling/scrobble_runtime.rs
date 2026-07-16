//! Live scrobbling worker runtime. HTTP and queue draining stay on a
//! dedicated thread; only immutable status values cross back to GTK.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use gtk4::glib;
use reprise_core::scrobbling::{
    self, ScrobbleProvider, ScrobblerTransport, TrackMetadata, TransportError,
};
use rusqlite::Connection;

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) enum ConnectionStatus {
    Disabled,
    Connecting,
    Connected {
        user_name: String,
        pending: usize,
        submitted: usize,
    },
    Offline {
        pending: usize,
        submitted: usize,
    },
    Unauthorized,
    Error {
        pending: usize,
        submitted: usize,
    },
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
struct WorkerConfig<'a> {
    database_path: &'a Path,
    provider: ScrobbleProvider,
    service: &'a str,
    credential: &'a str,
    generation: u64,
}

#[derive(Clone, Copy)]
struct WorkerCoordination<'a> {
    drain_lock: &'a Mutex<()>,
    cancelled: &'a AtomicBool,
}

type StatusCallback = Rc<dyn Fn(ConnectionStatus)>;

pub(in crate::ui) struct ScrobbleRuntime {
    database_path: PathBuf,
    provider: ScrobbleProvider,
    service: &'static str,
    generation: Cell<u64>,
    active: Cell<bool>,
    command: RefCell<Option<WorkerControl>>,
    status: RefCell<ConnectionStatus>,
    subscribers: RefCell<Vec<StatusCallback>>,
    drain_lock: Arc<Mutex<()>>,
}

impl ScrobbleRuntime {
    pub(in crate::ui) fn new(
        database_path: PathBuf,
        provider: ScrobbleProvider,
        service: &'static str,
    ) -> Rc<Self> {
        Rc::new(Self {
            database_path,
            provider,
            service,
            generation: Cell::new(0),
            active: Cell::new(false),
            command: RefCell::new(None),
            status: RefCell::new(ConnectionStatus::Disabled),
            subscribers: RefCell::new(Vec::new()),
            drain_lock: Arc::new(Mutex::new(())),
        })
    }

    pub(in crate::ui) fn is_active(&self) -> bool {
        self.active.get()
    }

    pub(in crate::ui) fn status(&self) -> ConnectionStatus {
        self.status.borrow().clone()
    }

    pub(in crate::ui) fn subscribe(&self, callback: StatusCallback) {
        let status = self.status();
        callback(status);
        self.subscribers.borrow_mut().push(callback);
    }

    pub(in crate::ui) fn configure(
        self: &Rc<Self>,
        credential: String,
        transport: Box<dyn ScrobblerTransport>,
    ) {
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
        let provider = self.provider;
        let service = self.service;
        let drain_lock = self.drain_lock.clone();
        std::thread::spawn(move || {
            run_worker(
                WorkerConfig {
                    database_path: &database_path,
                    provider,
                    service,
                    credential: &credential,
                    generation,
                },
                &command_receiver,
                &status_sender,
                transport.as_ref(),
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

    pub(in crate::ui) fn disable(&self) {
        self.active.set(false);
        self.generation.set(self.generation.get().wrapping_add(1));
        self.stop_worker();
        self.set_status(&ConnectionStatus::Disabled);
    }

    pub(in crate::ui) fn playing_now(&self, track: TrackMetadata) {
        if !self.active.get() {
            return;
        }
        self.send(WorkerCommand::PlayingNow(track));
    }

    pub(in crate::ui) fn flush(&self) {
        if self.active.get() {
            self.send(WorkerCommand::Flush);
        }
    }

    pub(in crate::ui) fn report_status(&self, status: &ConnectionStatus) {
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
                tracing::warn!(service = self.service, "scrobbling worker is unavailable");
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

impl Drop for ScrobbleRuntime {
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

fn flush_pending<T: ScrobblerTransport + ?Sized>(
    conn: &Connection,
    provider: ScrobbleProvider,
    transport: &T,
    credential: &str,
) -> Result<usize, FlushError> {
    let request_limit = match provider {
        ScrobbleProvider::ListenBrainz => 1_000,
        ScrobbleProvider::LastFm => 50,
    };
    loop {
        let listens = scrobbling::pending_for(conn, provider, request_limit)?;
        if listens.is_empty() {
            return scrobbling::pending_count_for(conn, provider).map_err(FlushError::from);
        }
        transport.submit(credential, &listens)?;
        let ids = listens
            .iter()
            .filter_map(|listen| listen.id)
            .collect::<Vec<_>>();
        if ids.len() != listens.len() {
            return Err(FlushError::MissingQueueId);
        }
        scrobbling::acknowledge_for(conn, provider, &ids)?;
    }
}

fn publish(
    sender: &async_channel::Sender<(u64, ConnectionStatus)>,
    generation: u64,
    status: ConnectionStatus,
) {
    let _ = sender.try_send((generation, status));
}

fn pending_or_zero(conn: &Connection, provider: ScrobbleProvider, service: &str) -> usize {
    scrobbling::pending_count_for(conn, provider).unwrap_or_else(|error| {
        tracing::warn!(%error, service, "could not count pending scrobbles");
        0
    })
}

fn submitted_or_zero(conn: &Connection, provider: ScrobbleProvider, service: &str) -> usize {
    scrobbling::submitted_count_for(conn, provider).unwrap_or_else(|error| {
        tracing::warn!(%error, service, "could not count submitted scrobbles");
        0
    })
}

fn run_worker<T: ScrobblerTransport + ?Sized>(
    config: WorkerConfig<'_>,
    receiver: &mpsc::Receiver<WorkerCommand>,
    status_sender: &async_channel::Sender<(u64, ConnectionStatus)>,
    transport: &T,
    coordination: WorkerCoordination<'_>,
) {
    let WorkerConfig {
        database_path,
        provider,
        service,
        credential,
        generation,
    } = config;
    let conn = match reprise_core::db::open_migrated(Some(database_path)) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(%error, service, "could not open scrobbling queue");
            publish(
                status_sender,
                generation,
                ConnectionStatus::Error {
                    pending: 0,
                    submitted: 0,
                },
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
            match transport.validate_token(credential) {
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
                            pending: pending_or_zero(&conn, provider, service),
                            submitted: submitted_or_zero(&conn, provider, service),
                        },
                    );
                    return;
                }
                Err(error) => {
                    tracing::warn!(%error, service, "scrobbling validation deferred");
                    publish(
                        status_sender,
                        generation,
                        ConnectionStatus::Offline {
                            pending: pending_or_zero(&conn, provider, service),
                            submitted: submitted_or_zero(&conn, provider, service),
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
            if let Err(error) = transport.playing_now(credential, &track) {
                if !handle_transport_error(
                    &conn,
                    provider,
                    service,
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
            flush_pending(&conn, provider, transport, credential)
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
                        submitted: submitted_or_zero(&conn, provider, service),
                    },
                );
            }
            Err(FlushError::Transport(error)) => {
                if !handle_transport_error(
                    &conn,
                    provider,
                    service,
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
                continue;
            }
            Err(FlushError::Queue(error)) => {
                tracing::warn!(%error, service, "could not drain scrobbling queue");
                publish(
                    status_sender,
                    generation,
                    ConnectionStatus::Error {
                        pending: pending_or_zero(&conn, provider, service),
                        submitted: submitted_or_zero(&conn, provider, service),
                    },
                );
                return;
            }
            Err(FlushError::MissingQueueId) => {
                tracing::warn!(service, "scrobbling queue returned a row without an id");
                publish(
                    status_sender,
                    generation,
                    ConnectionStatus::Error {
                        pending: pending_or_zero(&conn, provider, service),
                        submitted: submitted_or_zero(&conn, provider, service),
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
                if let Err(error) = transport.playing_now(credential, &track) {
                    if !handle_transport_error(
                        &conn,
                        provider,
                        service,
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
    provider: ScrobbleProvider,
    service: &str,
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
                    pending: pending_or_zero(conn, provider, service),
                    submitted: submitted_or_zero(conn, provider, service),
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
                    pending: pending_or_zero(conn, provider, service),
                    submitted: submitted_or_zero(conn, provider, service),
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
        assert_eq!(
            flush_pending(&conn, ScrobbleProvider::ListenBrainz, &transport, "token",).unwrap(),
            0
        );
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
            flush_pending(&conn, ScrobbleProvider::ListenBrainz, &transport, "token",),
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
            flush_pending(&conn, ScrobbleProvider::ListenBrainz, &transport, "token",),
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
                WorkerConfig {
                    database_path: &worker_path,
                    provider: ScrobbleProvider::ListenBrainz,
                    service: "ListenBrainz",
                    credential: "token",
                    generation: 42,
                },
                &command_receiver,
                &status_sender,
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
                    submitted: 2,
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

    mod worker_tests {
        include!("tests/scrobble_runtime_worker_tests.rs");
    }
}
