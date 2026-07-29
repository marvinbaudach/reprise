//! The runtime itself: one owner, one order, one truth.

use reprise_core::device_sync::machine::Event as DeviceEvent;
use reprise_core::device_sync::MirrorPlan;
use reprise_core::library::settings::{self, TrackTransition};
use reprise_core::playback::StreamEvent;
use reprise_runtime_protocol::command::CommandOutcome;
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::effects::EffectsRequest;
use reprise_runtime_protocol::jobs::JobCommand;
use reprise_runtime_protocol::playback::{ExternalMedia, PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueCommand, QueueSnapshot};
use reprise_runtime_protocol::session::RestoredQueue;
use reprise_runtime_protocol::PROTOCOL_VERSION;
use rusqlite::Connection;

use crate::client::{ClientHandshake, ClientId, Clients};
use crate::devices::DeviceRuns;
use crate::effects::Effects;
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
    /// Put back the queue a surface saved when it last closed, *without*
    /// starting it.
    ///
    /// Separate from `PlayTracks` because every other way of filling the
    /// queue also starts it, and opening the app is not a request to play.
    /// It carries the stored play order rather than only the ids: restoring
    /// the ids and reshuffling would change what comes next behind the back
    /// of a user who left mid-session.
    ///
    /// No position. GTK does not store one either, and a runtime that
    /// offered to restore one would be promising something no surface can
    /// supply.
    RestoreSession {
        context: RestoredQueue,
        play_next: Vec<i64>,
    },
    Job(JobCommand),
    Device(DeviceCommand),
    /// Apply the equalizer and ReplayGain, and store them if the audio path
    /// accepts them.
    SetAudioEffects(EffectsRequest),
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
            | Self::PlayExternal(_)
            | Self::RestoreSession { .. }
            | Self::SetAudioEffects(_) => Capability::PlaybackControl,
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
    effects: Effects,
    /// How many times the queue facet has observably changed.
    ///
    /// Kept here rather than in `Transport` on purpose: it has to count what
    /// a *client* saw, and the only place that knows a change was worth
    /// publishing is the diff below. Counting edits inside the transport
    /// would miss a track ending — which renumbers every context position,
    /// because the window starts at the cursor — and that is precisely the
    /// moment a stale position would slip through, since the user was not
    /// touching anything.
    queue_revision: u64,
}

impl Runtime {
    /// Builds a runtime over an already-migrated database.
    #[must_use]
    pub fn new(conn: Connection, ports: Ports) -> Self {
        // Before the struct, because the effects the backend accepts are part
        // of what it is built with rather than something applied to it after
        // the fact — and because a refusal has to be recorded, not discovered
        // later by a surface wondering why the equalizer does nothing.
        let effects = Effects::apply_stored(&conn, &*ports.playback);
        let runtime = Self {
            conn,
            ports,
            clients: Clients::new(),
            transport: Transport::new(),
            devices: DeviceRuns::new(),
            effects,
            queue_revision: 0,
        };
        // The backend is told the transition mode once at startup, the way
        // `PlaybackBackend::set_transition` asks: without it the pre-feeding
        // below arrives at a backend still on its own default.
        let (mode, crossfade) = runtime.transition();
        runtime.ports.playback.set_transition(mode, crossfade);
        runtime
    }

    /// The configured handoff mode, read fresh rather than cached: a setting
    /// changed in another surface has to take effect without a restart, and
    /// that is exactly what the GTK controller does on every pre-feed.
    fn transition(&self) -> (TrackTransition, u8) {
        (
            settings::get_track_transition(&self.conn),
            settings::get_crossfade_seconds(&self.conn),
        )
    }

    /// Re-tells the backend what to hand off to. Called after anything that
    /// can change the answer.
    fn refresh_pre_feed(&mut self) {
        let (mode, _) = self.transition();
        self.transport
            .refresh_pre_feed(&*self.ports.playback, &*self.ports.library, mode);
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

    /// The writer, for tests that assert on what a command persisted.
    /// Deliberately test-only: a client asks the runtime, and a second
    /// writer to this database is the thing §9.1 exists to prevent.
    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The complete runtime-bound state right now.
    pub fn snapshot(&self) -> Result<RuntimeSnapshot, RuntimeError> {
        Ok(RuntimeSnapshot {
            protocol: PROTOCOL_VERSION,
            sequence: self.clients.sequence(),
            playback: self.transport.playback_snapshot(),
            queue: self.stamped_queue(self.transport.queue_snapshot()),
            effects: self.effects.snapshot(),
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

    /// Executes one command on behalf of a connected client, and reports
    /// what it did — see [`CommandOutcome`].
    pub fn command(
        &mut self,
        client: ClientId,
        command: &Command,
    ) -> Result<CommandOutcome, RuntimeError> {
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
        let outcome = match command {
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
                self.publish_transport_changes(Some(client), before);
                result.map(|()| self.outcome(0))
            }
            Command::Queue(queue) => {
                // Before anything is applied: a position read from a queue
                // that has moved names a different row than the user did.
                // In range is not the same as still correct, so a bounds
                // check cannot stand in for this.
                if let Some(expected) = queue.expected_revision() {
                    if expected != self.queue_revision {
                        return Err(RuntimeError::Rejected(crate::error::Rejected::StaleQueue));
                    }
                }
                let before = self.transport_facets();
                let result = self.transport.queue_command(
                    &*self.ports.playback,
                    &*self.ports.library,
                    queue,
                );
                self.publish_transport_changes(Some(client), before);
                result.map(|affected| self.outcome(affected))
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
                    Some(client.into()),
                );
                self.publish_transport_changes(Some(client), before);
                result.map(|()| self.outcome(0))
            }
            Command::RestoreSession { context, play_next } => {
                let before = self.transport_facets();
                let result = self.transport.restore_session(context, play_next);
                self.publish_transport_changes(Some(client), before);
                result.map(|()| self.outcome(0))
            }
            Command::PlayExternal(media) => {
                let before = self.transport_facets();
                let result =
                    self.transport
                        .play_external(&*self.ports.playback, media, Some(client.into()));
                self.publish_transport_changes(Some(client), before);
                result.map(|()| self.outcome(0))
            }
            Command::Job(job) => {
                let now = self.ports.clock.now_unix();
                let job_id = jobs::command(&self.conn, now, job)?;
                if let Some(snapshot) = jobs::snapshot_of(&self.conn, job_id)? {
                    self.clients
                        .publish(Some(client), RuntimeEvent::JobChanged(snapshot));
                }
                Ok(self.outcome(0))
            }
            Command::Device(DeviceCommand::Start { device }) => {
                let before = self.devices.snapshot(device);
                let result = self.devices.start(&*self.ports.devices, device);
                self.publish_device_change(Some(client), device, before.as_ref());
                result.map(|()| self.outcome(0))
            }
            Command::Device(DeviceCommand::Cancel { device }) => {
                let before = self.devices.snapshot(device);
                let result = self.devices.cancel(
                    &*self.ports.devices,
                    device,
                    self.ports.clock.now_monotonic_ms(),
                );
                self.publish_device_change(Some(client), device, before.as_ref());
                result.map(|()| self.outcome(0))
            }
            Command::SetAudioEffects(requested) => {
                let before = self.effects.snapshot();
                let result = self
                    .effects
                    .set(&self.conn, &*self.ports.playback, requested);
                // Published on failure too: `set` reports a refusal only
                // after the backend has been asked, and a refusal that also
                // cleared a previous `degraded` is a change a surface has to
                // see.
                let after = self.effects.snapshot();
                if after != before {
                    self.clients
                        .publish(Some(client), RuntimeEvent::EffectsChanged(after));
                }
                result.map(|()| self.outcome(0))
            }
        };
        // The upcoming track may be a different one now — a queue edit, a
        // skip, a repeat toggle all change the answer, and `set_next` is
        // last-write-wins.
        self.refresh_pre_feed();
        outcome
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
        self.publish_transport_changes(None, before);
        // A finished track, a gapless handoff and a failure all move what
        // comes next; one handoff has to set up the one after it or gapless
        // works exactly once.
        self.refresh_pre_feed();
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
        self.publish_device_change(None, device, before.as_ref());
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
        self.publish_device_change(None, device, before.as_ref());
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

    /// What a command did, with the queue revision it left behind.
    ///
    /// `affected` is zero for everything that does not edit the queue, which
    /// is the answer rather than the absence of one — see
    /// [`CommandOutcome::affected`].
    fn outcome(&self, affected: u64) -> CommandOutcome {
        CommandOutcome {
            queue_revision: self.queue_revision,
            affected,
        }
    }

    /// Puts the current revision on a snapshot the transport built without
    /// one. Every queue snapshot that leaves the runtime goes through here.
    fn stamped_queue(&self, queue: QueueSnapshot) -> QueueSnapshot {
        QueueSnapshot {
            revision: self.queue_revision,
            ..queue
        }
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
    fn publish_transport_changes(
        &mut self,
        initiator: Option<ClientId>,
        before: (PlaybackSnapshot, QueueSnapshot),
    ) {
        let (playback_before, queue_before) = before;
        let playback = self.transport.playback_snapshot();
        if playback != playback_before {
            self.clients
                .publish(initiator, RuntimeEvent::PlaybackChanged(playback));
        }
        let queue = self.transport.queue_snapshot();
        if queue != queue_before {
            // The revision counts exactly this: a change worth telling a
            // client about. Both sides of the comparison are unstamped, so
            // the count itself cannot make the queue look changed.
            self.queue_revision += 1;
            let queue = self.stamped_queue(queue);
            self.clients
                .publish(initiator, RuntimeEvent::QueueChanged(queue));
        }
    }

    fn publish_device_change(
        &mut self,
        initiator: Option<ClientId>,
        device: &str,
        before: Option<&DeviceRunSnapshot>,
    ) {
        let Some(after) = self.devices.snapshot(device) else {
            return;
        };
        if before != Some(&after) {
            self.clients
                .publish(initiator, RuntimeEvent::DeviceRunChanged(after));
        }
    }
}
