//! The client's own thread, its connection, and its reconnection.

use std::time::Duration;

use reprise_runtime_protocol::runtime::RuntimeSnapshot;
use reprise_runtime_protocol::PROTOCOL_VERSION;

use reprise_runtime_protocol::{BUS_NAME, INTERFACE_NAME as INTERFACE, OBJECT_PATH};

use crate::events::{Body, ClientError, ClientEvent, RuntimeCommand};

/// The first retry delay, doubled until [`MAX_BACKOFF`].
const MIN_BACKOFF: Duration = Duration::from_millis(200);
/// The longest a client waits between attempts. Bounded, not unbounded
/// exponential: a surface that reconnects a minute after the runtime came
/// back is indistinguishable from a broken one.
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// How long a single method call may take before the client gives up on it.
///
/// zbus defaults to *no* timeout, and the worker issues its calls inline on
/// the one thread that processes jobs. A runtime that accepted the message
/// and then wedged — blocked on a slow write, stopped, gone to swap — would
/// therefore park that thread forever, and with it every later job including
/// the shutdown. A bounded wait turns that into an ordinary `Unavailable`
/// that reconnects. Generous rather than snappy: a real command answering
/// slowly must not be mistaken for a dead runtime.
const METHOD_TIMEOUT: Duration = Duration::from_secs(20);

/// A handle for sending commands. Cheap to clone; every clone talks to the
/// same connection.
#[derive(Clone)]
pub struct RuntimeClient {
    requests: async_channel::Sender<Job>,
}

/// The stream of state a surface follows.
pub struct RuntimeEvents {
    events: async_channel::Receiver<ClientEvent>,
}

impl RuntimeEvents {
    /// The underlying receiver, for a surface that already has a way to pump
    /// one — the GTK frontend spawns it onto the main context.
    #[must_use]
    pub fn receiver(&self) -> async_channel::Receiver<ClientEvent> {
        self.events.clone()
    }

    /// Blocks for the next event. For headless callers with no loop of their
    /// own.
    pub fn recv_blocking(&self) -> Option<ClientEvent> {
        self.events.recv_blocking().ok()
    }
}

/// One unit of work for the client thread.
enum Job {
    /// Do it and report a failure as an event.
    Send(RuntimeCommand),
    /// Do it and answer the caller.
    Call(
        RuntimeCommand,
        async_channel::Sender<Result<(), ClientError>>,
    ),
    /// Fetch the whole state again, after a `Resynchronize`.
    Resynchronize,
    /// The backoff elapsed; try the handshake again.
    Retry,
    /// The runtime's bus name changed hands.
    OwnerChanged { owned: bool },
    /// Say goodbye and stop.
    Shutdown,
}

/// Starts a client for `capabilities`.
///
/// Never fails: a runtime that is not running yet is an ordinary state,
/// reported as [`ClientEvent::Disconnected`]. Anything else would make every
/// surface implement "not started" twice — once at construction and once at
/// runtime.
#[must_use]
pub fn start(capabilities: Vec<String>) -> (RuntimeClient, RuntimeEvents) {
    start_with_bus_name(capabilities, BUS_NAME.to_owned())
}

/// Starts a client against an explicit well-known name.
///
/// Tests use a private one so they never connect to the developer's own
/// running Reprise — the same reason the MPRIS integration has this seam.
#[must_use]
pub fn start_with_bus_name(
    capabilities: Vec<String>,
    bus_name: String,
) -> (RuntimeClient, RuntimeEvents) {
    let (requests, jobs) = async_channel::unbounded::<Job>();
    let (events, incoming) = async_channel::unbounded::<ClientEvent>();

    let watcher = requests.clone();
    std::thread::spawn(move || {
        Worker {
            bus_name,
            capabilities,
            events,
            connection: None,
            connected: false,
            backoff: MIN_BACKOFF,
        }
        .run(&jobs, &watcher);
    });

    (
        RuntimeClient { requests },
        RuntimeEvents { events: incoming },
    )
}

impl RuntimeClient {
    /// Sends a command without waiting. A failure arrives as
    /// [`ClientEvent::CommandFailed`].
    ///
    /// This is what a UI uses: a bus round trip on the main thread is a
    /// visible stall, and every command a user issues already has a visible
    /// consequence to wait for.
    pub fn send(&self, command: RuntimeCommand) {
        let _ = self.requests.send_blocking(Job::Send(command));
    }

    /// Sends a command and waits for its outcome.
    ///
    /// This is what a tool call uses: "did it work" *is* the result, and
    /// there is no interface to keep responsive.
    pub fn call(&self, command: RuntimeCommand) -> Result<(), ClientError> {
        let (reply, answer) = async_channel::bounded(1);
        self.requests
            .send_blocking(Job::Call(command, reply))
            .map_err(|_| ClientError::Unavailable("unavailable:client_stopped".into()))?;
        answer
            .recv_blocking()
            .map_err(|_| ClientError::Unavailable("unavailable:client_stopped".into()))?
    }

    /// Asks for a fresh snapshot, which arrives as
    /// [`ClientEvent::Connected`].
    pub fn resynchronize(&self) {
        let _ = self.requests.send_blocking(Job::Resynchronize);
    }

    /// Says goodbye and stops the client thread.
    pub fn shutdown(&self) {
        let _ = self.requests.send_blocking(Job::Shutdown);
    }
}

struct Worker {
    bus_name: String,
    capabilities: Vec<String>,
    events: async_channel::Sender<ClientEvent>,
    connection: Option<zbus::blocking::Connection>,
    connected: bool,
    /// How long to wait before the next handshake attempt. Reset on every
    /// success, so a runtime that restarts often is still met promptly.
    backoff: Duration,
}

impl Worker {
    fn run(mut self, jobs: &async_channel::Receiver<Job>, watcher: &async_channel::Sender<Job>) {
        self.open_bus(watcher);
        self.handshake(watcher);

        while let Ok(job) = jobs.recv_blocking() {
            match job {
                Job::Send(command) => {
                    if let Err(error) = self.invoke(&command) {
                        let _ = self
                            .events
                            .send_blocking(ClientEvent::CommandFailed { command, error });
                    }
                }
                Job::Call(command, reply) => {
                    let _ = reply.send_blocking(self.invoke(&command));
                }
                Job::Resynchronize | Job::Retry | Job::OwnerChanged { owned: true } => {
                    self.handshake(watcher);
                }
                Job::OwnerChanged { owned: false } => self.mark_disconnected(),
                Job::Shutdown => {
                    self.say_goodbye();
                    return;
                }
            }
        }
    }

    /// Connects to the session bus and starts watching who owns the runtime's
    /// name.
    ///
    /// The bus connection itself is kept for the client's whole life: the
    /// session bus does not go away, and the runtime coming and going is a
    /// change of *name ownership*, which the bus reports. That is what makes
    /// reconnection a handshake rather than a rebuilt transport.
    fn open_bus(&mut self, watcher: &async_channel::Sender<Job>) {
        let opened = zbus::blocking::connection::Builder::session()
            .map(|builder| builder.method_timeout(METHOD_TIMEOUT))
            .and_then(zbus::blocking::connection::Builder::build);
        match opened {
            Ok(connection) => {
                spawn_owner_watch(&connection, self.bus_name.clone(), watcher.clone());
                spawn_signal_relay(&connection, self.events.clone(), watcher.clone());
                self.connection = Some(connection);
            }
            Err(error) => {
                tracing::warn!(%error, "no session bus; the runtime is unreachable");
                let _ = self.events.send_blocking(ClientEvent::Disconnected);
            }
        }
    }

    /// Completes the handshake, retrying with a bounded backoff.
    ///
    /// Calling any method on the well-known name is what activates the
    /// service, so there is no separate "start it" step — and deliberately
    /// so: exactly one start path is what makes the single-owner lease worth
    /// anything (§9.4/1).
    /// Attempts the handshake exactly once and schedules the next attempt.
    ///
    /// Deliberately not a retry loop: the worker would then be unable to
    /// answer anything — including a shutdown — while a runtime that is
    /// never coming back is retried forever, which is a hang on application
    /// exit rather than a resilience feature.
    fn handshake(&mut self, watcher: &async_channel::Sender<Job>) {
        match self.try_handshake() {
            Ok(snapshot) => {
                self.connected = true;
                self.backoff = MIN_BACKOFF;
                let _ = self
                    .events
                    .send_blocking(ClientEvent::Connected(Box::new(snapshot)));
            }
            Err(error) if error.is_retryable() => {
                self.mark_disconnected();
                schedule_retry(watcher.clone(), self.backoff);
                self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
            }
            Err(error) => {
                // `Refused` — a foreign protocol major, or a lease this
                // build cannot take. Retrying cannot change either, so the
                // surface is told once and left alone.
                tracing::error!(kind = error.kind(), "the runtime refused this client");
                self.mark_disconnected();
            }
        }
    }

    fn try_handshake(&self) -> Result<RuntimeSnapshot, ClientError> {
        let proxy = self.proxy()?;
        proxy
            .call(
                "Connect",
                &(
                    PROTOCOL_VERSION.major,
                    PROTOCOL_VERSION.minor,
                    self.capabilities.clone(),
                ),
            )
            .map_err(|error| ClientError::from_bus(&error))
    }

    /// Reports that the runtime is not usable right now.
    ///
    /// Emitted on every loss rather than only on the first: a surface
    /// renders the same unavailable state either way, and collapsing repeats
    /// here would hide a runtime that keeps dying and restarting.
    fn mark_disconnected(&mut self) {
        self.connected = false;
        let _ = self.events.send_blocking(ClientEvent::Disconnected);
    }

    fn say_goodbye(&mut self) {
        if let Ok(proxy) = self.proxy() {
            let _: Result<(), _> = proxy.call("Disconnect", &());
        }
        self.connected = false;
    }

    fn proxy(&self) -> Result<zbus::blocking::Proxy<'static>, ClientError> {
        let connection = self
            .connection
            .as_ref()
            .ok_or_else(|| ClientError::Unavailable("unavailable:no_bus".into()))?;
        zbus::blocking::Proxy::new_owned(
            connection.clone(),
            self.bus_name.clone(),
            OBJECT_PATH.to_owned(),
            INTERFACE.to_owned(),
        )
        .map_err(|error| ClientError::from_bus(&error))
    }

    /// Issues one command.
    ///
    /// A command sent while disconnected fails here rather than being
    /// queued: executing an old intention after a reconnect, against state
    /// it never saw, is the more dangerous of the two failures (§9.5).
    fn invoke(&self, command: &RuntimeCommand) -> Result<(), ClientError> {
        if !self.connected {
            return Err(ClientError::Unavailable("unavailable:not_connected".into()));
        }
        let proxy = self.proxy()?;
        let (method, body) = command.wire();
        let outcome = match body {
            Body::None => proxy.call::<_, _, ()>(method, &()),
            Body::Flag(value) => proxy.call::<_, _, ()>(method, &(value,)),
            Body::Volume(value) => proxy.call::<_, _, ()>(method, &(value,)),
            Body::Delta(value) | Body::Id(value) => proxy.call::<_, _, ()>(method, &(value,)),
            Body::Text(value) => proxy.call::<_, _, ()>(method, &(value,)),
            Body::Ids(values) => proxy.call::<_, _, ()>(method, &(values,)),
            Body::Tracks(ids, start) => proxy.call::<_, _, ()>(method, &(ids, start)),
            Body::Position(position) => proxy.call::<_, _, ()>(method, &(position,)),
            Body::Positions(positions) => proxy.call::<_, _, ()>(method, &(positions,)),
            Body::Move(from, to) => proxy.call::<_, _, ()>(method, &(from, to)),
            Body::External(media) => proxy.call::<_, _, ()>(
                method,
                &(
                    media.location,
                    media.remote,
                    media.title,
                    media.artist,
                    media.duration_ms,
                ),
            ),
        };
        outcome.map_err(|error| ClientError::from_bus(&error))
    }
}

/// Posts a retry after `delay`, on a thread of its own so the worker stays
/// able to answer in the meantime.
fn schedule_retry(jobs: async_channel::Sender<Job>, delay: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let _ = jobs.send_blocking(Job::Retry);
    });
}

/// Watches who owns the runtime's well-known name, so the client learns that
/// the runtime went away — or came back — without polling for it.
fn spawn_owner_watch(
    connection: &zbus::blocking::Connection,
    bus_name: String,
    jobs: async_channel::Sender<Job>,
) {
    let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(connection) else {
        tracing::warn!("cannot watch the runtime's bus name; reconnection will be manual");
        return;
    };
    std::thread::spawn(move || {
        let Ok(changes) = proxy.receive_name_owner_changed() else {
            return;
        };
        for change in changes {
            let Ok(args) = change.args() else { continue };
            if args.name().as_str() != bus_name {
                continue;
            }
            let owned = args.new_owner().is_some();
            if jobs.send_blocking(Job::OwnerChanged { owned }).is_err() {
                return;
            }
        }
    });
}

/// Turns the runtime's directed signals into events.
fn spawn_signal_relay(
    connection: &zbus::blocking::Connection,
    events: async_channel::Sender<ClientEvent>,
    jobs: async_channel::Sender<Job>,
) {
    let built = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(INTERFACE)
        .and_then(|builder| builder.path(OBJECT_PATH));
    let Ok(builder) = built else {
        tracing::error!("the runtime signal match rule is invalid");
        return;
    };
    let rule = builder.build();
    // A bounded queue with a generous depth: a surface that stops draining
    // is a bug in the surface, and an unbounded queue would turn it into a
    // memory leak instead of a visible stall.
    let Ok(messages) = zbus::blocking::MessageIterator::for_match_rule(rule, connection, Some(256))
    else {
        tracing::error!("cannot subscribe to runtime signals");
        return;
    };
    std::thread::spawn(move || {
        for message in messages {
            let Ok(message) = message else { continue };
            let delivered = match decode(&message) {
                Some(Delta::Event(event)) => events.send_blocking(event).is_ok(),
                // The runtime dropped events this client never drained. It
                // is absorbed here rather than passed on: a surface would
                // only do the same thing, and doing it in one place means it
                // cannot be forgotten in another.
                Some(Delta::Resynchronize) => jobs.send_blocking(Job::Resynchronize).is_ok(),
                None => true,
            };
            if !delivered {
                return;
            }
        }
    });
}

/// Decodes one signal. An unknown member is ignored rather than logged as an
/// error: a newer runtime may emit deltas this build has no use for, and
/// that is exactly what a minor protocol bump is allowed to do.
enum Delta {
    Event(ClientEvent),
    Resynchronize,
}

fn decode(message: &zbus::Message) -> Option<Delta> {
    let header = message.header();
    let member = header.member()?.as_str().to_owned();
    let body = message.body();
    match member.as_str() {
        "PlaybackChanged" => {
            let (sequence, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(ClientEvent::PlaybackChanged {
                sequence,
                snapshot,
            }))
        }
        "QueueChanged" => {
            let (sequence, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(ClientEvent::QueueChanged {
                sequence,
                snapshot,
            }))
        }
        "DeviceRunChanged" => {
            let (sequence, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(ClientEvent::DeviceRunChanged {
                sequence,
                snapshot,
            }))
        }
        "JobChanged" => {
            let (sequence, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(ClientEvent::JobChanged { sequence, snapshot }))
        }
        "Resynchronize" => Some(Delta::Resynchronize),
        _ => None,
    }
}
