//! The `org.reprise.Reprise1` interface.
//!
//! Every method here is a thin translation: check nothing, decide nothing,
//! hand the request to the thread that owns the runtime and wait for its
//! answer. Keeping the interface free of judgement is what makes the
//! runtime's own tests the whole truth — there is no second place where a
//! capability could be checked differently or an event ordered differently.
//!
//! Commands are typed methods rather than one method with an action string,
//! for the same reason the protocol crate gives: D-Bus has no sum type, so
//! the alternative is a bag of optional arguments where exactly one
//! combination is valid.

use reprise_runtime::{Command, DeviceCommand, RuntimeError};
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::jobs::{JobCommand, JobSnapshot};
use reprise_runtime_protocol::playback::{ExternalMedia, PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueCommand, QueueSnapshot};
use reprise_runtime_protocol::runtime::RuntimeSnapshot;
use reprise_runtime_protocol::ProtocolVersion;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

use super::service::Request;

/// The four outcome categories from §9.7, as D-Bus error names.
///
/// Each carries the runtime's short diagnostic kind — `rejected:
/// missing_capability:playback:control`, `failed:playback_backend` — which
/// is structured, path-free, and stable enough for an agent to branch on.
/// A client that only understands the four names still knows what to do.
///
/// Exactly four, and there is no room for a fifth. A runtime whose worker
/// thread has gone is unreachable *for this caller*, which is what
/// `Unavailable` already means; giving that its own error name would put a
/// category on the bus that no client was told to expect.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.reprise.Reprise1.Error")]
pub enum Error {
    /// The runtime could not be reached for this caller; reconnect.
    Unavailable(String),
    /// Turned away for good; do not retry.
    Refused(String),
    /// Formally valid, not admissible; do not retry.
    Rejected(String),
    /// The effect ran and failed; retrying is the user's decision.
    Failed(String),
}

impl From<RuntimeError> for Error {
    fn from(error: RuntimeError) -> Self {
        let kind = error.kind();
        match error {
            RuntimeError::Unavailable(_) => Self::Unavailable(kind),
            RuntimeError::Refused(_) => Self::Refused(kind),
            RuntimeError::Rejected(_) => Self::Rejected(kind),
            RuntimeError::Failed(_) => Self::Failed(kind),
        }
    }
}

/// The bus-facing object.
pub(crate) struct Reprise1 {
    requests: async_channel::Sender<Request>,
}

impl Reprise1 {
    pub(crate) fn new(requests: async_channel::Sender<Request>) -> Self {
        Self { requests }
    }

    /// The caller's bus name, which the bus itself assigned and verified.
    fn peer(header: &Header<'_>) -> Result<String, Error> {
        header
            .sender()
            .map(ToString::to_string)
            // Every message on a bus has a sender; a call that arrived
            // without one cannot be attributed to a session, so there is
            // nobody to serve.
            .ok_or_else(|| Error::Unavailable("unavailable:no_sender".into()))
    }

    /// Sends one request and waits for its single answer.
    async fn ask<T>(
        &self,
        build: impl FnOnce(async_channel::Sender<T>) -> Request,
    ) -> Result<T, Error> {
        let (reply, answers) = async_channel::bounded(1);
        self.requests
            .send(build(reply))
            .await
            .map_err(|_| Error::Unavailable("unavailable:runtime_stopped".into()))?;
        answers
            .recv()
            .await
            .map_err(|_| Error::Unavailable("unavailable:runtime_stopped".into()))
    }

    /// Sends a command and maps its outcome.
    async fn command(&self, header: &Header<'_>, command: Command) -> Result<(), Error> {
        let peer = Self::peer(header)?;
        self.ask(|reply| Request::Command {
            peer,
            command,
            reply,
        })
        .await?
        .map_err(Error::from)
    }
}

#[zbus::interface(name = "org.reprise.Reprise1")]
impl Reprise1 {
    /// Completes the handshake and returns the whole runtime-bound state.
    ///
    /// A foreign protocol major is refused instead of served (§9.7); a lower
    /// minor is fine, and deliberately so — a client updated on disk while
    /// the older runtime still runs is the most ordinary upgrade there is.
    async fn connect(
        &self,
        #[zbus(header)] header: Header<'_>,
        protocol_major: u32,
        protocol_minor: u32,
        capabilities: Vec<String>,
    ) -> Result<RuntimeSnapshot, Error> {
        let peer = Self::peer(&header)?;
        let protocol = ProtocolVersion {
            major: protocol_major,
            minor: protocol_minor,
        };
        self.ask(|reply| Request::Connect {
            peer,
            protocol,
            capabilities,
            reply,
        })
        .await?
        .map_err(Error::from)
    }

    /// Says goodbye. Not required — a client that simply exits is noticed —
    /// but it lets an interface release the runtime the moment it closes.
    async fn disconnect(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        let peer = Self::peer(&header)?;
        self.ask(|reply| Request::Disconnect { peer, reply }).await
    }

    /// The current state, for a client that was told to resynchronize.
    async fn snapshot(&self, #[zbus(header)] header: Header<'_>) -> Result<RuntimeSnapshot, Error> {
        let peer = Self::peer(&header)?;
        self.ask(|reply| Request::Snapshot { peer, reply })
            .await?
            .map_err(Error::from)
    }

    async fn play(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::Play))
            .await
    }

    async fn pause(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::Pause))
            .await
    }

    async fn stop(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::Stop))
            .await
    }

    async fn next(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::Next))
            .await
    }

    async fn previous(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::Previous))
            .await
    }

    async fn set_volume(
        &self,
        #[zbus(header)] header: Header<'_>,
        volume: f64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::Playback(PlaybackCommand::SetVolume(volume)),
        )
        .await
    }

    /// Relative seek in milliseconds; negative seeks backwards.
    async fn seek(&self, #[zbus(header)] header: Header<'_>, delta_ms: i64) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::Seek(delta_ms)))
            .await
    }

    async fn set_shuffle(&self, #[zbus(header)] header: Header<'_>, on: bool) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::SetShuffle(on)))
            .await
    }

    /// `off`, `all` or `one`. Anything else is rejected rather than
    /// silently treated as `off`.
    async fn set_repeat(
        &self,
        #[zbus(header)] header: Header<'_>,
        mode: String,
    ) -> Result<(), Error> {
        self.command(&header, Command::Playback(PlaybackCommand::SetRepeat(mode)))
            .await
    }

    /// Replaces the context queue and starts at `start_index`, which is
    /// clamped rather than rejected.
    async fn play_tracks(
        &self,
        #[zbus(header)] header: Header<'_>,
        track_ids: Vec<i64>,
        start_index: u64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::PlayTracks {
                track_ids,
                start_index: usize::try_from(start_index).unwrap_or(usize::MAX),
            },
        )
        .await
    }

    /// Plays a stream, a podcast episode or a preview render — anything
    /// without a library id. The queue is left where it is; going back to it
    /// afterwards finds it untouched.
    ///
    /// The caller says whether `location` is remote rather than leaving the
    /// runtime to sniff the string, so a local path containing `://` is not
    /// opened as a URL.
    async fn play_external(
        &self,
        #[zbus(header)] header: Header<'_>,
        location: String,
        remote: bool,
        title: String,
        artist: String,
        duration_ms: i64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::PlayExternal(ExternalMedia {
                location,
                remote,
                title,
                artist,
                duration_ms,
            }),
        )
        .await
    }

    async fn queue_add_next(
        &self,
        #[zbus(header)] header: Header<'_>,
        track_ids: Vec<i64>,
    ) -> Result<(), Error> {
        self.command(&header, Command::Queue(QueueCommand::AddNext(track_ids)))
            .await
    }

    async fn queue_add_last(
        &self,
        #[zbus(header)] header: Header<'_>,
        track_ids: Vec<i64>,
    ) -> Result<(), Error> {
        self.command(&header, Command::Queue(QueueCommand::AddLast(track_ids)))
            .await
    }

    /// Drops the explicit queue. The current track keeps playing — clearing
    /// a queue is not a stop command.
    async fn queue_clear(&self, #[zbus(header)] header: Header<'_>) -> Result<(), Error> {
        self.command(&header, Command::Queue(QueueCommand::Clear))
            .await
    }

    /// Moves one explicit-queue entry. Positions come from the caller's
    /// last queue snapshot; a stale one is rejected rather than applied to
    /// whichever row happens to be there now.
    async fn queue_move(
        &self,
        #[zbus(header)] header: Header<'_>,
        from: u64,
        to: u64,
        expected_revision: u64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::Queue(QueueCommand::Move {
                from,
                to,
                expected_revision,
            }),
        )
        .await
    }

    async fn queue_remove_at(
        &self,
        #[zbus(header)] header: Header<'_>,
        positions: Vec<u64>,
        expected_revision: u64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::Queue(QueueCommand::RemoveAt {
                positions,
                expected_revision,
            }),
        )
        .await
    }

    async fn queue_remove_context_at(
        &self,
        #[zbus(header)] header: Header<'_>,
        positions: Vec<u64>,
        expected_revision: u64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::Queue(QueueCommand::RemoveContextAt {
                positions,
                expected_revision,
            }),
        )
        .await
    }

    async fn queue_play_next_at(
        &self,
        #[zbus(header)] header: Header<'_>,
        position: u64,
        expected_revision: u64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::Queue(QueueCommand::PlayNextAt {
                position,
                expected_revision,
            }),
        )
        .await
    }

    async fn queue_play_context_at(
        &self,
        #[zbus(header)] header: Header<'_>,
        position: u64,
        expected_revision: u64,
    ) -> Result<(), Error> {
        self.command(
            &header,
            Command::Queue(QueueCommand::PlayContextAt {
                position,
                expected_revision,
            }),
        )
        .await
    }

    /// Forgets these track ids wherever they are queued — a library deletion
    /// reaching the queue, not a user editing it.
    async fn queue_purge(
        &self,
        #[zbus(header)] header: Header<'_>,
        track_ids: Vec<i64>,
    ) -> Result<(), Error> {
        self.command(&header, Command::Queue(QueueCommand::Purge(track_ids)))
            .await
    }

    async fn device_start(
        &self,
        #[zbus(header)] header: Header<'_>,
        device: String,
    ) -> Result<(), Error> {
        self.command(&header, Command::Device(DeviceCommand::Start { device }))
            .await
    }

    async fn device_cancel(
        &self,
        #[zbus(header)] header: Header<'_>,
        device: String,
    ) -> Result<(), Error> {
        self.command(&header, Command::Device(DeviceCommand::Cancel { device }))
            .await
    }

    /// Asks the runtime to stop a job. Cancellation is a request, never an
    /// assertion: the job snapshot's `cancel_requested` reports the ask and
    /// `state` reports what actually happened.
    async fn job_cancel(
        &self,
        #[zbus(header)] header: Header<'_>,
        job_id: i64,
    ) -> Result<(), Error> {
        self.command(&header, Command::Job(JobCommand::Cancel(job_id)))
            .await
    }

    /// The protocol version this runtime speaks, readable without
    /// connecting — so a client can report a mismatch instead of a failure.
    #[zbus(property)]
    fn protocol_version(&self) -> (u32, u32) {
        (
            reprise_runtime_protocol::PROTOCOL_VERSION.major,
            reprise_runtime_protocol::PROTOCOL_VERSION.minor,
        )
    }

    /// Emitted to one peer when its playback facet changed.
    #[zbus(signal)]
    async fn playback_changed(
        emitter: &SignalEmitter<'_>,
        sequence: u64,
        initiator: u64,
        snapshot: PlaybackSnapshot,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn queue_changed(
        emitter: &SignalEmitter<'_>,
        sequence: u64,
        initiator: u64,
        snapshot: QueueSnapshot,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn device_run_changed(
        emitter: &SignalEmitter<'_>,
        sequence: u64,
        initiator: u64,
        snapshot: DeviceRunSnapshot,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn job_changed(
        emitter: &SignalEmitter<'_>,
        sequence: u64,
        initiator: u64,
        snapshot: JobSnapshot,
    ) -> zbus::Result<()>;

    /// The peer fell too far behind and its oldest events were dropped. It
    /// must take a fresh [`Reprise1::snapshot`]; applying the remaining
    /// deltas on top of a state that missed some is the silent divergence
    /// the sequence numbers exist to prevent.
    #[zbus(signal)]
    async fn resynchronize(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
