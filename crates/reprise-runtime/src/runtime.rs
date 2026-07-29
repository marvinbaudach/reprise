//! The runtime itself: one owner, one order, one truth.

use reprise_core::device_sync::machine::Event as DeviceEvent;
use reprise_core::device_sync::MirrorPlan;
use reprise_core::playback::StreamEvent;
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::jobs::JobCommand;
use reprise_runtime_protocol::playback::{ExternalMedia, PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueCommand, QueueSnapshot};
use reprise_runtime_protocol::PROTOCOL_VERSION;
use rusqlite::Connection;

use crate::client::{ClientHandshake, ClientId, Clients};
use crate::devices::DeviceRuns;
use crate::error::{Capability, Refused, RuntimeError, Unavailable};
use crate::event::{Delivery, RuntimeEvent, RuntimeSnapshot};
use crate::jobs;
use crate::ports::Ports;
use crate::transport::Transport;

/// What a client asks the runtime to do.
///
/// One enum rather than a method per command so the capability check, the
/// connection check and the event publication happen in exactly one place —
/// a new command cannot forget any of them.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Playback(PlaybackCommand),
    Queue(QueueCommand),
    /// Replace the context queue with these tracks and start at
    /// `start_index`, which is clamped rather than rejected.
    PlayTracks {
        track_ids: Vec<i64>,
        start_index: usize,
    },
    /// Play something that is not a library track — a stream, a podcast
    /// episode, a preview render. The queue is left where it is.
    PlayExternal(ExternalMedia),
    Job(JobCommand),
    Device(DeviceCommand),
}

/// A device-run command. The device is addressed by its display name, the
/// same address every device snapshot carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCommand {
    Start { device: String },
    Cancel { device: String },
}

impl Command {
    /// The capability this command hangs on. Every mutation has exactly one
    /// (§9.8); there is no unguarded command.
    fn capability(&self) -> Capability {
        match self {
            Self::Playback(_)
            | Self::Queue(_)
            | Self::PlayTracks { .. }
            | Self::PlayExternal(_) => Capability::PlaybackControl,
            Self::Job(_) => Capability::AiCreate,
            Self::Device(_) => Capability::DeviceSync,
        }
    }
}

/// What a successful connection yields.
#[derive(Debug, Clone, PartialEq)]
pub struct Connected {
    pub client: ClientId,
    /// The complete runtime-bound state at connection time. Every event the
    /// client subsequently drains carries a strictly greater sequence.
    pub snapshot: RuntimeSnapshot,
}

/// The single owner of playback, the queue, device runs, background jobs and
/// the database writer.
///
/// # Threading
///
/// `Runtime` is deliberately not `Sync`. Exactly one thread owns it; clients
/// reach it through a transport (Task 3.2's D-Bus service) and the ports feed
/// results back through [`Runtime::on_player_event`],
/// [`Runtime::on_device_plan`] and [`Runtime::on_device_event`]. Making the
/// state machine single-threaded is what lets every test drive it
/// deterministically: there is no scheduler to race with.
///
/// # Crash recovery
///
/// There is none to perform, and that is the design rather than an omission.
/// In-memory state — what was playing, the queue, a device run — belongs to
/// the process that held it; a new runtime starts with a stopped player and
/// an empty queue instead of resurrecting a guess. Job state lives in
/// SQLite and is simply read back. Nothing is replayed: a client that
/// reconnects after a crash takes a fresh snapshot, exactly as it would after
/// an ordinary reconnect (§9.5).
pub struct Runtime {
    /// The writer. Every runtime effect that touches the database goes
    /// through this one connection, which is what serializes them (§9.1).
    conn: Connection,
    ports: Ports,
    clients: Clients,
    transport: Transport,
    devices: DeviceRuns,
}

impl Runtime {
    /// Builds a runtime over an already-migrated database.
    #[must_use]
    pub fn new(conn: Connection, ports: Ports) -> Self {
        Self {
            conn,
            ports,
            clients: Clients::new(),
            transport: Transport::new(),
            devices: DeviceRuns::new(),
        }
    }

    /// Admits a client, or refuses it for good.
    ///
    /// A foreign protocol *major* version is refused rather than served: a
    /// client that cannot decode the payload is better off being told than
    /// being handed one. A lower minor is fine and deliberately so — see
    /// [`reprise_runtime_protocol::ProtocolVersion::is_compatible_with`].
    pub fn connect(&mut self, handshake: &ClientHandshake) -> Result<Connected, RuntimeError> {
        if !PROTOCOL_VERSION.is_compatible_with(handshake.protocol) {
            return Err(RuntimeError::Refused(Refused::ProtocolMajor {
                runtime: PROTOCOL_VERSION,
                client: handshake.protocol,
            }));
        }
        let snapshot = self.snapshot()?;
        let client = self.clients.connect(handshake.capabilities.clone());
        Ok(Connected { client, snapshot })
    }

    /// Drops a client and its undelivered events. Returns whether it was
    /// connected, so a repeated disconnect is a no-op rather than an error.
    pub fn disconnect(&mut self, client: ClientId) -> bool {
        self.clients.disconnect(client)
    }

    /// The complete runtime-bound state right now.
    pub fn snapshot(&self) -> Result<RuntimeSnapshot, RuntimeError> {
        Ok(RuntimeSnapshot {
            protocol: PROTOCOL_VERSION,
            sequence: self.clients.sequence(),
            playback: self.transport.playback_snapshot(),
            queue: self.transport.queue_snapshot(),
            device_runs: self.devices.snapshots(),
            jobs: jobs::snapshots(&self.conn)?,
        })
    }

    /// Hands a client its pending events and clears them.
    pub fn drain(&mut self, client: ClientId) -> Result<Delivery, RuntimeError> {
        self.clients
            .drain(client)
            .ok_or(RuntimeError::Unavailable(Unavailable::NotConnected))
    }

    /// Executes one command on behalf of a connected client.
    pub fn command(&mut self, client: ClientId, command: &Command) -> Result<(), RuntimeError> {
        if !self.clients.is_connected(client) {
            // Not buffered for a later reconnect (§9.5): executing an old
            // intention against state it never saw is the worse failure.
            return Err(RuntimeError::Unavailable(Unavailable::NotConnected));
        }
        let capability = command.capability();
        if !self.clients.holds(client, capability) {
            return Err(RuntimeError::Rejected(
                crate::error::Rejected::MissingCapability(capability),
            ));
        }
        match command {
            Command::Playback(playback) => {
                let before = self.transport_facets();
                let result = self.transport.playback_command(
                    &*self.ports.playback,
                    &*self.ports.library,
                    playback,
                );
                // Published even when the command failed: a partially
                // applied transport (a stop that reached the backend before
                // the error) must not stay invisible.
                self.publish_transport_changes(before);
                result
            }
            Command::Queue(queue) => {
                let before = self.transport_facets();
                let result = self.transport.queue_command(
                    &*self.ports.playback,
                    &*self.ports.library,
                    queue,
                );
                self.publish_transport_changes(before);
                result
            }
            Command::PlayTracks {
                track_ids,
                start_index,
            } => {
                let before = self.transport_facets();
                let result = self.transport.play_tracks(
                    &*self.ports.playback,
                    &*self.ports.library,
                    track_ids.clone(),
                    *start_index,
                );
                self.publish_transport_changes(before);
                result
            }
            Command::PlayExternal(media) => {
                let before = self.transport_facets();
                let result = self.transport.play_external(&*self.ports.playback, media);
                self.publish_transport_changes(before);
                result
            }
            Command::Job(job) => {
                let now = self.ports.clock.now_unix();
                let job_id = jobs::command(&self.conn, now, job)?;
                if let Some(snapshot) = jobs::snapshot_of(&self.conn, job_id)? {
                    self.clients.publish(RuntimeEvent::JobChanged(snapshot));
                }
                Ok(())
            }
            Command::Device(DeviceCommand::Start { device }) => {
                let before = self.devices.snapshot(device);
                let result = self.devices.start(&*self.ports.devices, device);
                self.publish_device_change(device, before.as_ref());
                result
            }
            Command::Device(DeviceCommand::Cancel { device }) => {
                let before = self.devices.snapshot(device);
                let result = self.devices.cancel(
                    &*self.ports.devices,
                    device,
                    self.ports.clock.now_monotonic_ms(),
                );
                self.publish_device_change(device, before.as_ref());
                result
            }
        }
    }

    /// Applies an asynchronous report from the audio backend.
    ///
    /// A report from a stream that has already been replaced is dropped: it
    /// describes a track nobody is listening to any more, and applying it
    /// would advance the queue past a track the user never skipped.
    pub fn on_player_event(&mut self, event: &StreamEvent) {
        if !self.transport.accepts_stream(event.generation) {
            tracing::debug!("dropped a report from a stream that has been replaced");
            return;
        }
        let before = self.transport_facets();
        self.transport
            .player_event(&*self.ports.playback, &*self.ports.library, &event.event);
        self.publish_transport_changes(before);
    }

    /// Answers a [`crate::ports::DeviceEffects::plan`] request. `None` means
    /// the plan could not be computed; the run ends without having touched
    /// the device.
    pub fn on_device_plan(&mut self, device: &str, plan: Option<MirrorPlan>) {
        let before = self.devices.snapshot(device);
        self.devices.on_plan(
            &*self.ports.devices,
            device,
            plan,
            self.ports.clock.now_monotonic_ms(),
        );
        self.publish_device_change(device, before.as_ref());
    }

    /// Answers a [`crate::ports::DeviceEffects::perform`] request.
    pub fn on_device_event(&mut self, device: &str, event: DeviceEvent) {
        let before = self.devices.snapshot(device);
        self.devices.on_event(
            &*self.ports.devices,
            device,
            event,
            self.ports.clock.now_monotonic_ms(),
        );
        self.publish_device_change(device, before.as_ref());
    }

    /// Whether all four of §9.6's conditions hold: no client connected, no
    /// track loaded (a paused one still counts), no device run, no job. Only
    /// then may the idle timer start — a runtime that abandons work to save
    /// memory is a data-loss feature.
    pub fn is_idle(&self) -> Result<bool, RuntimeError> {
        Ok(self.clients.count() == 0
            && !self.transport.is_active()
            && !self.devices.is_active()
            && !jobs::is_active(&self.conn)?)
    }

    fn transport_facets(&self) -> (PlaybackSnapshot, QueueSnapshot) {
        (
            self.transport.playback_snapshot(),
            self.transport.queue_snapshot(),
        )
    }

    /// Publishes exactly the transport facets that actually changed.
    ///
    /// Diffing against a before-image rather than having each command
    /// declare what it touched: a command that forgets to declare something
    /// produces a silently stale client, and there is no test that catches
    /// the omission reliably. Comparison cannot forget.
    fn publish_transport_changes(&mut self, before: (PlaybackSnapshot, QueueSnapshot)) {
        let (playback_before, queue_before) = before;
        let playback = self.transport.playback_snapshot();
        if playback != playback_before {
            self.clients
                .publish(RuntimeEvent::PlaybackChanged(playback));
        }
        let queue = self.transport.queue_snapshot();
        if queue != queue_before {
            self.clients.publish(RuntimeEvent::QueueChanged(queue));
        }
    }

    fn publish_device_change(&mut self, device: &str, before: Option<&DeviceRunSnapshot>) {
        let Some(after) = self.devices.snapshot(device) else {
            return;
        };
        if before != Some(&after) {
            self.clients.publish(RuntimeEvent::DeviceRunChanged(after));
        }
    }
}
