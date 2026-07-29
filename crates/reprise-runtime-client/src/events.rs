//! What a client sends and what it hears back.

use reprise_runtime_protocol::command::CommandOutcome;
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::effects::{EffectsRequest, EffectsSnapshot};
use reprise_runtime_protocol::jobs::{JobCommand, JobSnapshot};
use reprise_runtime_protocol::playback::{ExternalMedia, PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueCommand, QueueSnapshot};
use reprise_runtime_protocol::session::RestoredQueue;

use crate::client::RequestId;
use reprise_runtime_protocol::runtime::RuntimeSnapshot;

/// One thing a surface asks the runtime to do.
///
/// The same shape the runtime's own command enum has, so the translation on
/// either side of the bus is mechanical and there is no third vocabulary to
/// keep in step.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeCommand {
    Playback(PlaybackCommand),
    Queue(QueueCommand),
    PlayTracks {
        track_ids: Vec<i64>,
        start_index: usize,
    },
    /// Play a stream, a podcast episode or a preview render.
    /// Put back a queue a surface saved when it last closed, without
    /// starting it.
    RestoreSession {
        context: RestoredQueue,
        play_next: Vec<i64>,
    },
    PlayExternal(ExternalMedia),
    /// Apply the equalizer and ReplayGain.
    SetAudioEffects(EffectsRequest),
    Job(JobCommand),
    DeviceStart {
        device: String,
    },
    DeviceCancel {
        device: String,
    },
}

impl RuntimeCommand {
    /// The interface method and its arguments.
    pub(super) fn wire(&self) -> (&'static str, Body) {
        match self {
            Self::Playback(PlaybackCommand::Play) => ("Play", Body::None),
            Self::Playback(PlaybackCommand::Pause) => ("Pause", Body::None),
            Self::Playback(PlaybackCommand::Stop) => ("Stop", Body::None),
            Self::Playback(PlaybackCommand::Next) => ("Next", Body::None),
            Self::Playback(PlaybackCommand::Previous) => ("Previous", Body::None),
            Self::Playback(PlaybackCommand::SetVolume(volume)) => {
                ("SetVolume", Body::Volume(*volume))
            }
            Self::Playback(PlaybackCommand::Seek(delta_ms)) => ("Seek", Body::Delta(*delta_ms)),
            Self::Playback(PlaybackCommand::SetShuffle(on)) => ("SetShuffle", Body::Flag(*on)),
            Self::Playback(PlaybackCommand::SetRepeat(mode)) => {
                ("SetRepeat", Body::Text(mode.clone()))
            }
            Self::Queue(QueueCommand::AddNext(ids)) => ("QueueAddNext", Body::Ids(ids.clone())),
            Self::Queue(QueueCommand::AddLast(ids)) => ("QueueAddLast", Body::Ids(ids.clone())),
            Self::Queue(QueueCommand::Clear) => ("QueueClear", Body::None),
            Self::Queue(QueueCommand::Move {
                from,
                to,
                expected_revision,
            }) => ("QueueMove", Body::Move(*from, *to, *expected_revision)),
            Self::Queue(QueueCommand::RemoveAt {
                positions,
                expected_revision,
            }) => (
                "QueueRemoveAt",
                Body::Positions(positions.clone(), *expected_revision),
            ),
            Self::Queue(QueueCommand::RemoveContextAt {
                positions,
                expected_revision,
            }) => (
                "QueueRemoveContextAt",
                Body::Positions(positions.clone(), *expected_revision),
            ),
            Self::Queue(QueueCommand::PlayNextAt {
                position,
                expected_revision,
            }) => (
                "QueuePlayNextAt",
                Body::Position(*position, *expected_revision),
            ),
            Self::Queue(QueueCommand::PlayContextAt {
                position,
                expected_revision,
            }) => (
                "QueuePlayContextAt",
                Body::Position(*position, *expected_revision),
            ),
            Self::Queue(QueueCommand::Purge(ids)) => ("QueuePurge", Body::Ids(ids.clone())),
            Self::PlayTracks {
                track_ids,
                start_index,
            } => (
                "PlayTracks",
                Body::Tracks(track_ids.clone(), *start_index as u64),
            ),
            Self::PlayExternal(media) => ("PlayExternal", Body::External(media.clone())),
            Self::SetAudioEffects(effects) => ("SetAudioEffects", Body::Effects(effects.clone())),
            Self::RestoreSession { context, play_next } => (
                "RestoreSession",
                Body::Session(context.clone(), play_next.clone()),
            ),
            Self::Job(JobCommand::Cancel(job_id)) => ("JobCancel", Body::Id(*job_id)),
            Self::Job(JobCommand::Save(job_id)) => ("JobSave", Body::Id(*job_id)),
            Self::Job(JobCommand::Discard(job_id)) => ("JobDiscard", Body::Id(*job_id)),
            Self::DeviceStart { device } => ("DeviceStart", Body::Text(device.clone())),
            Self::DeviceCancel { device } => ("DeviceCancel", Body::Text(device.clone())),
        }
    }
}

/// A method call's arguments, as one of the few shapes the interface uses.
///
/// An enum rather than a boxed `dyn Serialize` because zbus needs a concrete
/// type at the call site, and because the set of argument shapes is small
/// and closed — a new one is a deliberate addition, not an accident.
pub(super) enum Body {
    None,
    Flag(bool),
    Volume(f64),
    Delta(i64),
    Id(i64),
    Text(String),
    Ids(Vec<i64>),
    Tracks(Vec<i64>, u64),
    /// A position plus the queue revision it was read from — the two always
    /// travel together, because a position without one names a row nobody
    /// can check.
    Position(u64, u64),
    Positions(Vec<u64>, u64),
    Move(u64, u64, u64),
    External(ExternalMedia),
    Effects(EffectsRequest),
    Session(RestoredQueue, Vec<i64>),
}

/// What a surface hears from the runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    /// The runtime is reachable and this is its complete state. Everything
    /// runtime-bound that the surface holds is replaced, not merged: a
    /// snapshot is the truth, and reconciling it with a stale mirror is how
    /// two views of one player start to disagree.
    Connected(Box<RuntimeSnapshot>),
    /// The runtime is gone. Transport, queue and device actions are shown as
    /// unavailable rather than as a dummy built from the last known state
    /// (RUN-2).
    Disconnected,
    /// The runtime turned this client away for good — its protocol major
    /// version is foreign, or it was refused for another reason retrying
    /// cannot change.
    ///
    /// Separate from [`Self::Disconnected`] because the two ask different
    /// things of a surface: a disconnection is a wait, and this is a
    /// sentence. Folding it into a plain disconnection would leave a client
    /// reconnecting forever against a runtime that will never accept it,
    /// with nothing to show the user but a spinner.
    Refused(ClientError),
    EffectsChanged {
        sequence: u64,
        initiator: Option<u64>,
        snapshot: EffectsSnapshot,
    },
    PlaybackChanged {
        sequence: u64,
        /// Who provoked this, or `None` when nothing a client asked for did —
        /// a position tick, a track ending, an idle deadline. A surface compares
        /// it against [`RuntimeSnapshot::client_id`] to tell its own change from
        /// somebody else's: RUN-5 says an external change is followed quietly,
        /// which is only decidable if "external" is decidable.
        initiator: Option<u64>,
        snapshot: PlaybackSnapshot,
    },
    QueueChanged {
        sequence: u64,
        initiator: Option<u64>,
        snapshot: QueueSnapshot,
    },
    DeviceRunChanged {
        sequence: u64,
        initiator: Option<u64>,
        snapshot: DeviceRunSnapshot,
    },
    JobChanged {
        sequence: u64,
        initiator: Option<u64>,
        snapshot: JobSnapshot,
    },
    /// A command this client sent took effect, and this is what it did.
    ///
    /// Carried as an event for the same reason the failure is: `send` cannot
    /// wait for the answer without stalling the thread it was called on, and
    /// the thread it is called on is a UI thread.
    CommandCompleted {
        request: RequestId,
        outcome: CommandOutcome,
    },
    /// A command this client sent did not succeed. Carried as an event
    /// because [`super::RuntimeClient::send`] cannot wait for it without
    /// stalling the thread it was called on.
    ///
    /// `request` names *which* send failed. The command alone does not: two
    /// identical commands are an ordinary thing for a user to produce.
    CommandFailed {
        request: RequestId,
        command: RuntimeCommand,
        error: ClientError,
    },
}

/// Why an interaction with the runtime did not succeed — §9.7's four
/// categories, as a client sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// Not reachable, or this client is not connected. Reconnect; the client
    /// is already trying.
    Unavailable(String),
    /// Turned away for good — the lease is held elsewhere, or the protocol
    /// major does not match. Do not retry; name the cause.
    Refused(String),
    /// Formally valid, not admissible. Do not retry; show the reason.
    Rejected(String),
    /// The effect ran and failed. Retrying is a user decision.
    Failed(String),
}

impl ClientError {
    /// The short diagnostic kind the runtime produced, or the transport's
    /// own reason when the call never reached it.
    #[must_use]
    pub fn kind(&self) -> &str {
        match self {
            Self::Unavailable(kind)
            | Self::Refused(kind)
            | Self::Rejected(kind)
            | Self::Failed(kind) => kind,
        }
    }

    /// Whether the client should try the same thing again on its own.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    /// Maps a bus-level failure onto the four categories.
    ///
    /// An unknown error name is `Failed` rather than `Unavailable`: a client
    /// that retries something it does not understand can repeat a side
    /// effect, while one that stops merely reports a failure it already had.
    pub(super) fn from_bus(error: &zbus::Error) -> Self {
        let zbus::Error::MethodError(name, message, _) = error else {
            return Self::Unavailable(transport_kind(error));
        };
        let kind = message.clone().unwrap_or_else(|| name.as_str().to_owned());
        match name.as_str().rsplit('.').next() {
            // Only the runtime's own errors carry a message worth keeping:
            // it is the short diagnostic kind this crate defined. Everything
            // else on the bus writes free prose — an activation failure will
            // happily quote the path of the executable it could not run — so
            // those get a kind derived from the error *name* alone. That is
            // what keeps the client's errors structured and path-free even
            // when the failure did not come from us.
            Some("Unavailable") => Self::Unavailable(kind),
            Some("Refused") => Self::Refused(kind),
            Some("Rejected") => Self::Rejected(kind),
            Some("Failed") => Self::Failed(kind),
            // The bus answering that nobody serves this name is the ordinary
            // "runtime not started" case, and it is retryable.
            Some("ServiceUnknown" | "NameHasNoOwner" | "NoServer") => {
                Self::Unavailable("unavailable:not_started".to_owned())
            }
            // A timeout is NOT `Unavailable`. D-Bus has no way to retract a
            // request, so the runtime may well be executing it right now:
            // reporting "not reachable, retry freely" would let a client add
            // the same tracks to the queue twice. `Failed` is the category
            // whose contract is "the effect may have run; retrying is a user
            // decision", which is exactly what is true here.
            Some("NoReply" | "Timeout" | "TimedOut") => Self::Failed("failed:no_reply".to_owned()),
            _ => Self::Failed("failed:bus_error".to_owned()),
        }
    }
}

/// A short kind for a failure that never reached the runtime.
fn transport_kind(error: &zbus::Error) -> String {
    match error {
        zbus::Error::Address(_) | zbus::Error::InputOutput(_) => "unavailable:no_bus".to_owned(),
        _ => "unavailable:transport".to_owned(),
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.kind())
    }
}

impl std::error::Error for ClientError {}
