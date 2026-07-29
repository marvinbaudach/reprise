//! The client's own thread, its connection, and its reconnection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reprise_runtime_protocol::command::CommandOutcome;
use reprise_runtime_protocol::runtime::RuntimeSnapshot;
use reprise_runtime_protocol::PROTOCOL_VERSION;

use reprise_runtime_protocol::{
    ProtocolVersion, BUS_NAME, INTERFACE_NAME as INTERFACE, OBJECT_PATH,
};

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

/// Which connection a command was formed against.
///
/// Zero means "no connection". Every successful handshake takes the next
/// value, so a command formed while disconnected — or against a session that
/// has since been replaced — is recognisable as such no matter how long it
/// sat in the queue.
type Generation = u64;

/// The generation that means "not connected to anything".
const DISCONNECTED: Generation = 0;

/// A handle for sending commands. Cheap to clone; every clone talks to the
/// same connection.
#[derive(Clone)]
pub struct RuntimeClient {
    requests: async_channel::Sender<Job>,
    /// Read at submission time and carried with the command. This is what
    /// makes section 9.5's rule true rather than merely intended: a command
    /// sent while disconnected must *fail*, not sit in a queue and execute
    /// once a later handshake succeeds. Without it the worker's own state is
    /// read too late — after a reconnection the caller never saw.
    generation: Arc<AtomicU64>,
    /// Hands each `send` its own id. Two identical commands are otherwise
    /// indistinguishable in the event stream — "remove row 0" twice is a
    /// perfectly ordinary thing for a user to do — and a surface could not
    /// tell which of them the outcome belonged to.
    next_request: Arc<AtomicU64>,
}

/// Identifies one `send`, so its outcome can be recognised when it arrives.
///
/// Opaque and per-client: it means nothing to the runtime, which never sees
/// it. Correlation is the client's own bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u64);

/// An id means nothing outside the client that minted it — the runtime never
/// sees one — so it converts both ways freely: a surface that keeps its
/// pending commands in a map wants the number, and a test wants to make one.
impl From<RequestId> for u64 {
    fn from(id: RequestId) -> Self {
        id.0
    }
}

impl From<u64> for RequestId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "request-{}", self.0)
    }
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

    /// The next event if one is already waiting.
    ///
    /// For a caller that must stay responsive to something else — a test
    /// with a deadline, a loop with its own work — and cannot afford to
    /// block on a client that may have gone quiet.
    pub fn try_recv(&self) -> Option<ClientEvent> {
        self.events.try_recv().ok()
    }
}

/// One unit of work for the client thread.
enum Job {
    /// Do it and report what it did — or that it failed — as an event
    /// carrying the id `send` handed the caller.
    Send(RuntimeCommand, Generation, RequestId),
    /// Do it and answer the caller.
    Call(
        RuntimeCommand,
        Generation,
        async_channel::Sender<Result<CommandOutcome, ClientError>>,
    ),
    /// A signal arrived. Routed through the worker rather than straight to
    /// the surface so one thread decides the order everything is published
    /// in: a delta must never reach a client before the `Connected` snapshot
    /// that opens its session, and the two arrive on different threads.
    Signal(Delta, Generation),
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
    start_with_bus_name_and_version(capabilities, bus_name, PROTOCOL_VERSION)
}

/// Starts a client that announces an explicit protocol version.
///
/// Only a test has any business claiming a version its build does not speak;
/// this exists so the refusal path can be driven at all, since a runtime
/// refuses on the *client's* version and there is no other way to present a
/// foreign one.
#[must_use]
pub fn start_with_bus_name_and_version(
    capabilities: Vec<String>,
    bus_name: String,
    protocol: ProtocolVersion,
) -> (RuntimeClient, RuntimeEvents) {
    let (requests, jobs) = async_channel::unbounded::<Job>();
    let (events, incoming) = async_channel::unbounded::<ClientEvent>();
    let generation = Arc::new(AtomicU64::new(DISCONNECTED));

    let watcher = requests.clone();
    let worker_generation = Arc::clone(&generation);
    std::thread::spawn(move || {
        Worker {
            bus_name,
            capabilities,
            events,
            connection: None,
            generation: worker_generation,
            protocol,
            backoff: MIN_BACKOFF,
        }
        .run(&jobs, &watcher);
    });

    (
        RuntimeClient {
            requests,
            generation,
            next_request: Arc::new(AtomicU64::new(1)),
        },
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
    pub fn send(&self, command: RuntimeCommand) -> RequestId {
        let generation = self.generation.load(Ordering::SeqCst);
        let request = RequestId(self.next_request.fetch_add(1, Ordering::SeqCst));
        let _ = self
            .requests
            .send_blocking(Job::Send(command, generation, request));
        request
    }

    /// Sends a command and waits for its outcome.
    ///
    /// This is what a tool call uses: "did it work" *is* the result, and
    /// there is no interface to keep responsive.
    pub fn call(&self, command: RuntimeCommand) -> Result<CommandOutcome, ClientError> {
        let (reply, answer) = async_channel::bounded(1);
        let generation = self.generation.load(Ordering::SeqCst);
        self.requests
            .send_blocking(Job::Call(command, generation, reply))
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
    /// The current session, shared with every handle and with the signal
    /// relay. `DISCONNECTED` while there is none.
    generation: Arc<AtomicU64>,
    /// The version this client announces. Always this build's, except in the
    /// tests that drive the refusal path.
    protocol: ProtocolVersion,
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
                Job::Send(command, formed_in, request) => {
                    let event = match self.invoke(&command, formed_in) {
                        Ok(outcome) => ClientEvent::CommandCompleted { request, outcome },
                        Err(error) => ClientEvent::CommandFailed {
                            request,
                            command,
                            error,
                        },
                    };
                    let _ = self.events.send_blocking(event);
                }
                Job::Call(command, formed_in, reply) => {
                    let _ = reply.send_blocking(self.invoke(&command, formed_in));
                }
                Job::Signal(delta, seen_in) => self.publish(delta, seen_in, watcher),
                // A resynchronization is asked for by the runtime and always
                // acted on. The other two only mean anything when there is
                // no session: re-handshaking on top of a live one would hand
                // the surface a second full snapshot for the same connection
                // and make the runtime tear down and rebuild the peer it
                // already has.
                Job::Resynchronize => self.handshake(watcher),
                Job::Retry | Job::OwnerChanged { owned: true } => {
                    if !self.is_connected() {
                        self.handshake(watcher);
                    }
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
                spawn_signal_relay(
                    &connection,
                    &self.bus_name,
                    Arc::clone(&self.generation),
                    watcher.clone(),
                );
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
                // The session opens here, on this thread, and the snapshot
                // goes out before any signal job queued behind it — which is
                // what guarantees a surface never sees a delta belonging to
                // a session it has not been told about yet.
                self.generation.fetch_add(1, Ordering::SeqCst);
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
                // Turned away for good — a foreign protocol major. Retrying
                // cannot change that, so the surface is told *why* and left
                // alone: a plain disconnection would have it reconnecting
                // forever against a runtime that will never accept it.
                tracing::error!(kind = error.kind(), "the runtime refused this client");
                self.generation.store(DISCONNECTED, Ordering::SeqCst);
                let _ = self.events.send_blocking(ClientEvent::Refused(error));
            }
        }
    }

    fn try_handshake(&self) -> Result<RuntimeSnapshot, ClientError> {
        let proxy = self.proxy()?;
        proxy
            .call(
                "Connect",
                &(
                    self.protocol.major,
                    self.protocol.minor,
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
        self.generation.store(DISCONNECTED, Ordering::SeqCst);
        let _ = self.events.send_blocking(ClientEvent::Disconnected);
    }

    fn is_connected(&self) -> bool {
        self.generation.load(Ordering::SeqCst) != DISCONNECTED
    }

    /// Forwards one signal, unless it belongs to a session that has ended.
    ///
    /// Routing signals through this thread is what orders them against the
    /// `Connected` snapshot; dropping the ones stamped with an older
    /// generation is what stops a delta from a previous session being
    /// applied on top of the new one's snapshot.
    fn publish(&mut self, delta: Delta, seen_in: Generation, watcher: &async_channel::Sender<Job>) {
        if seen_in != self.generation.load(Ordering::SeqCst) {
            return;
        }
        match delta {
            Delta::Event(event) => {
                let _ = self.events.send_blocking(*event);
            }
            // The runtime dropped events this client never drained. Absorbed
            // here rather than passed on: a surface would only do the same
            // thing, and doing it in one place means it cannot be forgotten
            // in another.
            Delta::Resynchronize => self.handshake(watcher),
        }
    }

    fn say_goodbye(&mut self) {
        if let Ok(proxy) = self.proxy() {
            let _: Result<(), _> = proxy.call("Disconnect", &());
        }
        self.generation.store(DISCONNECTED, Ordering::SeqCst);
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
    /// A command is refused unless the session it was formed against is
    /// still the current one.
    ///
    /// Checking the generation the *caller* saw rather than the worker's
    /// state now is the whole point: a command submitted while disconnected
    /// sits in the queue, and by the time this thread reaches it a handshake
    /// may well have succeeded. Executing it then would run an old intention
    /// against state it never saw, which §9.5 calls the more dangerous of
    /// the two failures — so it fails instead, and the surface decides
    /// whether to offer it again.
    fn invoke(
        &self,
        command: &RuntimeCommand,
        formed_in: Generation,
    ) -> Result<CommandOutcome, ClientError> {
        if !is_current(formed_in, self.generation.load(Ordering::SeqCst)) {
            return Err(ClientError::Unavailable("unavailable:not_connected".into()));
        }
        let proxy = self.proxy()?;
        let (method, body) = command.wire();
        let outcome = match body {
            Body::None => proxy.call::<_, _, CommandOutcome>(method, &()),
            Body::Flag(value) => proxy.call::<_, _, CommandOutcome>(method, &(value,)),
            Body::Volume(value) => proxy.call::<_, _, CommandOutcome>(method, &(value,)),
            Body::Delta(value) | Body::Id(value) => {
                proxy.call::<_, _, CommandOutcome>(method, &(value,))
            }
            Body::Text(value) => proxy.call::<_, _, CommandOutcome>(method, &(value,)),
            Body::Ids(values) => proxy.call::<_, _, CommandOutcome>(method, &(values,)),
            Body::Tracks(ids, start) => proxy.call::<_, _, CommandOutcome>(method, &(ids, start)),
            Body::Position(position, revision) => {
                proxy.call::<_, _, CommandOutcome>(method, &(position, revision))
            }
            Body::Positions(positions, revision) => {
                proxy.call::<_, _, CommandOutcome>(method, &(positions, revision))
            }
            Body::Move(from, to, revision) => {
                proxy.call::<_, _, CommandOutcome>(method, &(from, to, revision))
            }
            Body::Session(context, play_next) => {
                proxy.call::<_, _, CommandOutcome>(method, &(context, play_next))
            }
            Body::External(media) => proxy.call::<_, _, CommandOutcome>(
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

/// Whether a command formed in generation `formed_in` may still run, given
/// that the client is now in generation `current`.
///
/// Two ways to fail, and they are different failures wearing the same
/// answer: `formed_in == DISCONNECTED` is a command whose caller never had a
/// runtime, and `formed_in != current` is one whose session ended while it
/// waited. Both mean the caller reasoned about state this command would now
/// be applied to blind.
///
/// A free function because it is the whole rule: everything else about
/// generations is bookkeeping, and a rule that only exists inside a
/// worker-thread method can only be tested by racing that thread.
fn is_current(formed_in: Generation, current: Generation) -> bool {
    formed_in != DISCONNECTED && formed_in == current
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
    bus_name: &str,
    generation: Arc<AtomicU64>,
    jobs: async_channel::Sender<Job>,
) {
    // The sender is part of the rule, not an afterthought. Without it any
    // process on the session bus could broadcast a `PlaybackChanged` on this
    // interface and path, and this client would fold it into the state it
    // renders as the runtime's. Directed signals protect their *destination*;
    // they do not make a receiving rule sender-safe.
    let built = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(bus_name)
        .and_then(|builder| builder.interface(INTERFACE))
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
            let Some(delta) = decode(&message) else {
                continue;
            };
            // Stamped where it is received, so the worker can tell a delta
            // belonging to the session it is serving from one left over from
            // a session that has already ended.
            let seen_in = generation.load(Ordering::SeqCst);
            if jobs.send_blocking(Job::Signal(delta, seen_in)).is_err() {
                return;
            }
        }
    });
}

/// Decodes one signal. An unknown member is ignored rather than logged as an
/// error: a newer runtime may emit deltas this build has no use for, and
/// that is exactly what a minor protocol bump is allowed to do.
enum Delta {
    /// Boxed because the other variant carries nothing: an unboxed event
    /// would make every `Resynchronize` as large as the biggest snapshot a
    /// delta can hold.
    Event(Box<ClientEvent>),
    Resynchronize,
}

/// The wire carries "nobody asked" as zero, because a signal argument has no
/// room for an absent value and client ids start at one. Unpacked here, once,
/// so no caller has to remember what zero means.
fn initiator_of(raw: u64) -> Option<u64> {
    (raw != 0).then_some(raw)
}

fn decode(message: &zbus::Message) -> Option<Delta> {
    let header = message.header();
    let member = header.member()?.as_str().to_owned();
    let body = message.body();
    match member.as_str() {
        "PlaybackChanged" => {
            let (sequence, initiator, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(Box::new(ClientEvent::PlaybackChanged {
                sequence,
                initiator: initiator_of(initiator),
                snapshot,
            })))
        }
        "QueueChanged" => {
            let (sequence, initiator, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(Box::new(ClientEvent::QueueChanged {
                sequence,
                initiator: initiator_of(initiator),
                snapshot,
            })))
        }
        "DeviceRunChanged" => {
            let (sequence, initiator, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(Box::new(ClientEvent::DeviceRunChanged {
                sequence,
                initiator: initiator_of(initiator),
                snapshot,
            })))
        }
        "JobChanged" => {
            let (sequence, initiator, snapshot) = body.deserialize().ok()?;
            Some(Delta::Event(Box::new(ClientEvent::JobChanged {
                sequence,
                initiator: initiator_of(initiator),
                snapshot,
            })))
        }
        "Resynchronize" => Some(Delta::Resynchronize),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_current, DISCONNECTED};

    #[test]
    fn a_command_formed_without_a_connection_never_becomes_current() {
        assert!(
            !is_current(DISCONNECTED, 7),
            "its caller had no runtime to reason about; a handshake that \
             happened afterwards does not retroactively give it one"
        );
        assert!(!is_current(DISCONNECTED, DISCONNECTED));
    }

    #[test]
    fn a_command_outlives_nothing_but_its_own_session() {
        assert!(is_current(7, 7), "the session it was formed in is still on");
        assert!(
            !is_current(7, 8),
            "the session ended and another began; the state this command was \
             aimed at is gone, and applying it to the new one is exactly the \
             stale intention §9.5 refuses to execute"
        );
        assert!(
            !is_current(8, 7),
            "a generation that ran backwards is a bug, and guessing which \
             way to resolve it would hide it"
        );
    }

    #[test]
    fn a_command_is_refused_once_its_session_has_ended() {
        assert!(
            !is_current(7, DISCONNECTED),
            "there is nothing to send it to"
        );
    }
}
