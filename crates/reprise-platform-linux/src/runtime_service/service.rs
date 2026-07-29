//! `org.reprise.Reprise1` — the runtime on the session bus.
//!
//! ## Thread model
//!
//! The runtime is single-threaded by design: one owner, one order, no
//! scheduler to race with. The bus is not — zbus dispatches method calls on
//! its own executor. So the thread that calls [`RuntimeService::serve`]
//! keeps the [`Runtime`] and never lets go of it; the interface handlers
//! send a [`Request`] and wait for its reply. Nothing else ever touches
//! runtime state.
//!
//! The same loop carries three other inputs, which is why it is a loop and
//! not a callback: asynchronous reports from the audio backend, a periodic
//! tick that advances the idle grace, and the bus telling us a peer
//! vanished.
//!
//! ## Why a client is identified by its bus name
//!
//! There is no session token. A caller is identified by the unique name the
//! bus itself assigns and verifies, so a client cannot hold the wrong
//! session by accident and cannot borrow another's by guessing a number. It
//! also gives the runtime the one thing a token cannot: `NameOwnerChanged`
//! tells us when a client died without saying goodbye, and a client that
//! crashed must stop holding the runtime awake (§9.6).

use std::collections::HashMap;
use std::time::Duration;

use reprise_runtime::{
    Capability, ClientHandshake, ClientId, Command, LifecycleChange, LifecycleMachine, Runtime,
    RuntimeError, RuntimeEvent,
};
use reprise_runtime_protocol::ProtocolVersion;

use super::lease::{LeaseError, RuntimeLease};

pub use reprise_runtime_protocol::{BUS_NAME, INTERFACE_NAME, OBJECT_PATH};

/// How often the loop reconsiders the idle grace. Coarse on purpose: this
/// wakes an otherwise-sleeping process, and the grace it serves is measured
/// in minutes.
const TICK: Duration = Duration::from_secs(5);

/// What went wrong before the runtime ever served a request.
#[derive(Debug)]
pub enum ServiceError {
    /// Another process owns the runtime (§9.3). The only correct response is
    /// to exit; this is `Refused`, not a retry.
    Lease(LeaseError),
    /// The session bus refused the connection or the well-known name.
    Bus(zbus::Error),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lease(error) => write!(formatter, "lease: {error}"),
            Self::Bus(error) => write!(formatter, "session bus: {error}"),
        }
    }
}

impl std::error::Error for ServiceError {}

/// A reply channel for one request. Bounded at one: a request has exactly
/// one answer, and a sender that outlives its caller should fail rather than
/// buffer.
pub type Reply<T> = async_channel::Sender<T>;

/// One thing the serving loop has to handle.
// `Player` carries a `PlayerEvent`, whose `Spectrum` variant is a fixed
// 64-band frame emitted ~60x/s. `reprise-core` declines to box it for that
// reason and so does this: an allocation per frame on the audio path costs
// more than the padding in an enum that never sits in an array.
#[allow(clippy::large_enum_variant)]
pub enum Request {
    /// A peer completed the handshake.
    Connect {
        peer: String,
        protocol: ProtocolVersion,
        capabilities: Vec<String>,
        reply: Reply<Result<reprise_runtime_protocol::runtime::RuntimeSnapshot, RuntimeError>>,
    },
    /// A peer said goodbye.
    Disconnect { peer: String, reply: Reply<()> },
    /// A peer's current view, for a client that has to start over.
    Snapshot {
        peer: String,
        reply: Reply<Result<reprise_runtime_protocol::runtime::RuntimeSnapshot, RuntimeError>>,
    },
    /// A peer issued a command.
    Command {
        peer: String,
        command: Command,
        reply: Reply<Result<(), RuntimeError>>,
    },
    /// The bus reported that a peer's name has no owner any more.
    PeerVanished { peer: String },
    /// An asynchronous report from the audio backend.
    Player(reprise_core::playback::PlayerEvent),
    /// A device port finished computing what a run would change. `None`
    /// means it could not.
    DevicePlan {
        device: String,
        plan: Option<Box<reprise_core::device_sync::MirrorPlan>>,
    },
    /// The idle grace should be reconsidered.
    Tick,
}

/// The serving loop's inbox.
///
/// Created before the runtime, because the ports the runtime is built from
/// need the sender: work that finishes asynchronously — a device plan, a
/// transcode — posts its result back here rather than touching runtime state
/// from whatever thread it happened to finish on.
pub struct ServiceInbox {
    sender: async_channel::Sender<Request>,
    receiver: async_channel::Receiver<Request>,
}

impl Default for ServiceInbox {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceInbox {
    #[must_use]
    pub fn new() -> Self {
        let (sender, receiver) = async_channel::unbounded();
        Self { sender, receiver }
    }

    /// A handle ports keep so they can answer.
    #[must_use]
    pub fn sender(&self) -> async_channel::Sender<Request> {
        self.sender.clone()
    }
}

/// Knobs the binary and the tests differ on.
pub struct ServeOptions {
    /// The well-known name to claim. Tests use a private one so they never
    /// fight the user's real runtime for the session identity.
    pub bus_name: String,
    /// How long the runtime stays up with nothing to do.
    pub grace: Duration,
    /// How often the loop reconsiders that grace.
    pub tick: Duration,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            bus_name: BUS_NAME.to_owned(),
            grace: reprise_runtime::IDLE_GRACE,
            tick: TICK,
        }
    }
}

/// The runtime, published on the session bus.
pub struct RuntimeService;

impl RuntimeService {
    /// Claims the lease, publishes the interface, and serves until the idle
    /// grace expires.
    ///
    /// The lease is claimed by the caller and handed in already held,
    /// because §9.3 requires it to be taken *before* GStreamer, devices or
    /// the writer are opened — and by the time a [`Runtime`] exists, all
    /// three have been.
    pub fn serve(
        runtime: Runtime,
        lease: RuntimeLease,
        options: &ServeOptions,
        inbox: ServiceInbox,
        player_events: Option<async_channel::Receiver<reprise_core::playback::PlayerEvent>>,
    ) -> Result<(), ServiceError> {
        let ServiceInbox {
            sender,
            receiver: requests,
        } = inbox;
        let connection = zbus::blocking::connection::Builder::session()
            .map_err(ServiceError::Bus)?
            .name(options.bus_name.as_str())
            .map_err(ServiceError::Bus)?
            .serve_at(OBJECT_PATH, super::interface::Reprise1::new(sender.clone()))
            .map_err(ServiceError::Bus)?
            .build()
            .map_err(ServiceError::Bus)?;

        spawn_ticker(sender.clone(), options.tick);
        if let Some(events) = player_events {
            spawn_player_relay(sender.clone(), events);
        }
        spawn_peer_watch(&connection, sender);

        Loop {
            runtime,
            lifecycle: LifecycleMachine::with_grace(options.grace),
            peers: HashMap::new(),
            connection: connection.clone(),
            started: std::time::Instant::now(),
        }
        .run(&requests);

        // Held until here so no effect can outlive the lease.
        drop(lease);
        Ok(())
    }
}

/// Relays the periodic tick. A thread rather than a timer on the loop so the
/// loop itself can block on one channel and nothing else.
fn spawn_ticker(sender: async_channel::Sender<Request>, tick: Duration) {
    std::thread::spawn(move || {
        while sender.send_blocking(Request::Tick).is_ok() {
            std::thread::sleep(tick);
        }
    });
}

/// Forwards what the audio backend reports — except the spectrum.
///
/// A spectrum frame arrives ~60x/s and the runtime discards it: it is a
/// rendering concern of whichever surface draws a visualizer, not runtime
/// state. Relaying it anyway would put sixty messages a second into an
/// unbounded inbox for nobody, and would be the first thing to pile up if
/// the serving loop ever stalled on a slow write. When a visualizer does
/// need frames, it will ask for them and they will travel their own way.
fn spawn_player_relay(
    sender: async_channel::Sender<Request>,
    events: async_channel::Receiver<reprise_core::playback::PlayerEvent>,
) {
    std::thread::spawn(move || {
        while let Ok(event) = events.recv_blocking() {
            if matches!(event, reprise_core::playback::PlayerEvent::Spectrum(_)) {
                continue;
            }
            if sender.send_blocking(Request::Player(event)).is_err() {
                return;
            }
        }
    });
}

/// Watches for peers disappearing without disconnecting.
///
/// A client that crashed still counts as connected until somebody notices,
/// and a runtime that believes a dead client is watching never becomes idle.
/// The bus already knows; this just listens.
fn spawn_peer_watch(
    connection: &zbus::blocking::Connection,
    sender: async_channel::Sender<Request>,
) {
    let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(connection) else {
        tracing::warn!("cannot watch peer lifetimes; a crashed client will hold the runtime awake");
        return;
    };
    std::thread::spawn(move || {
        let Ok(changes) = proxy.receive_name_owner_changed() else {
            return;
        };
        for change in changes {
            let Ok(args) = change.args() else { continue };
            if args.new_owner().is_none() {
                let peer = args.name().to_string();
                if sender
                    .send_blocking(Request::PeerVanished { peer })
                    .is_err()
                {
                    return;
                }
            }
        }
    });
}

/// The serving loop's own state.
struct Loop {
    runtime: Runtime,
    lifecycle: LifecycleMachine,
    /// Bus unique name to runtime client. One entry per connected peer.
    peers: HashMap<String, ClientId>,
    connection: zbus::blocking::Connection,
    started: std::time::Instant,
}

impl Loop {
    fn run(mut self, requests: &async_channel::Receiver<Request>) {
        while let Ok(request) = requests.recv_blocking() {
            self.handle(request);
            self.publish();
            if !self.reconcile() {
                break;
            }
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Connect {
                peer,
                protocol,
                capabilities,
                reply,
            } => {
                let answer = self.connect(&peer, protocol, &capabilities);
                let _ = reply.send_blocking(answer);
            }
            Request::Disconnect { peer, reply } => {
                self.drop_peer(&peer);
                let _ = reply.send_blocking(());
            }
            Request::Snapshot { peer, reply } => {
                let answer = if self.peers.contains_key(&peer) {
                    self.runtime.snapshot().map(|snapshot| wire(&snapshot))
                } else {
                    Err(RuntimeError::Unavailable(
                        reprise_runtime::Unavailable::NotConnected,
                    ))
                };
                let _ = reply.send_blocking(answer);
            }
            Request::Command {
                peer,
                command,
                reply,
            } => {
                let answer = match self.peers.get(&peer) {
                    Some(client) => self.runtime.command(*client, &command),
                    None => Err(RuntimeError::Unavailable(
                        reprise_runtime::Unavailable::NotConnected,
                    )),
                };
                let _ = reply.send_blocking(answer);
            }
            Request::PeerVanished { peer } => self.drop_peer(&peer),
            Request::Player(event) => self.runtime.on_player_event(&event),
            Request::DevicePlan { device, plan } => {
                self.runtime.on_device_plan(&device, plan.map(|plan| *plan));
            }
            Request::Tick => {}
        }
    }

    fn connect(
        &mut self,
        peer: &str,
        protocol: ProtocolVersion,
        capabilities: &[String],
    ) -> Result<reprise_runtime_protocol::runtime::RuntimeSnapshot, RuntimeError> {
        // A peer reconnecting without disconnecting first replaces its old
        // session rather than accumulating one: the bus name is the identity.
        self.drop_peer(peer);
        let handshake = ClientHandshake {
            protocol,
            capabilities: capabilities
                .iter()
                .filter_map(|name| capability(name))
                .collect(),
        };
        let connected = self.runtime.connect(&handshake)?;
        self.peers.insert(peer.to_owned(), connected.client);
        self.lifecycle.serve();
        Ok(wire(&connected.snapshot))
    }

    fn drop_peer(&mut self, peer: &str) {
        if let Some(client) = self.peers.remove(peer) {
            self.runtime.disconnect(client);
        }
    }

    /// Sends every peer the events queued for it, as directed signals.
    ///
    /// Directed rather than broadcast because a client that connected late
    /// must not receive what happened before its snapshot — the same reason
    /// there is no replay log at all (§9.5).
    fn publish(&mut self) {
        let peers: Vec<(String, ClientId)> = self
            .peers
            .iter()
            .map(|(peer, client)| (peer.clone(), *client))
            .collect();
        for (peer, client) in peers {
            let Ok(delivery) = self.runtime.drain(client) else {
                continue;
            };
            for event in delivery.events {
                self.emit(&peer, event.sequence, &event.event);
            }
            if delivery.resynchronize {
                self.signal(&peer, "Resynchronize", &());
            }
        }
    }

    fn emit(&self, peer: &str, sequence: u64, event: &RuntimeEvent) {
        match event {
            RuntimeEvent::PlaybackChanged(snapshot) => {
                self.signal(peer, "PlaybackChanged", &(sequence, snapshot));
            }
            RuntimeEvent::QueueChanged(snapshot) => {
                self.signal(peer, "QueueChanged", &(sequence, snapshot));
            }
            RuntimeEvent::DeviceRunChanged(snapshot) => {
                self.signal(peer, "DeviceRunChanged", &(sequence, snapshot));
            }
            RuntimeEvent::JobChanged(snapshot) => {
                self.signal(peer, "JobChanged", &(sequence, snapshot));
            }
        }
    }

    fn signal<B>(&self, peer: &str, member: &str, body: &B)
    where
        B: serde::ser::Serialize + zvariant::DynamicType,
    {
        if let Err(error) =
            self.connection
                .emit_signal(Some(peer), OBJECT_PATH, INTERFACE_NAME, member, body)
        {
            // A peer that went away between the drain and the send is
            // ordinary, not a fault: it will take a fresh snapshot if it
            // comes back.
            tracing::debug!(%peer, %member, %error, "runtime signal not delivered");
        }
    }

    /// Advances the lifecycle. Returns whether the loop should keep running.
    fn reconcile(&mut self) -> bool {
        let idle = self.runtime.is_idle().unwrap_or(false);
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match self.lifecycle.observe(idle, now_ms) {
            Some(LifecycleChange::EnteredDraining) => {
                tracing::info!("runtime idle; the shutdown grace has started");
            }
            Some(LifecycleChange::LeftDraining) => {
                tracing::info!("runtime busy again; the shutdown grace was abandoned");
            }
            Some(LifecycleChange::Shutdown) => {
                tracing::info!("runtime idle for the whole grace; shutting down");
                return false;
            }
            None => {}
        }
        self.lifecycle.is_running()
    }
}

/// Maps a capability name from the wire onto the runtime's enum. An unknown
/// name is dropped rather than rejected: a newer client may hold
/// capabilities this build has never heard of, and the commands they guard
/// do not exist here anyway.
fn capability(name: &str) -> Option<Capability> {
    match name {
        "playback:control" => Some(Capability::PlaybackControl),
        "device:sync" => Some(Capability::DeviceSync),
        "ai:create" => Some(Capability::AiCreate),
        _ => None,
    }
}

/// The snapshot in its wire shape.
pub(crate) fn wire(
    snapshot: &reprise_runtime::RuntimeSnapshot,
) -> reprise_runtime_protocol::runtime::RuntimeSnapshot {
    reprise_runtime_protocol::runtime::RuntimeSnapshot {
        protocol_major: snapshot.protocol.major,
        protocol_minor: snapshot.protocol.minor,
        sequence: snapshot.sequence,
        playback: snapshot.playback.clone(),
        queue: snapshot.queue.clone(),
        device_runs: snapshot.device_runs.clone(),
        jobs: snapshot.jobs.clone(),
    }
}
