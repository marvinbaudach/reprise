//! What a client learns: one full snapshot on connect, deltas after it.
//!
//! §9.5 fixes the shape and, more importantly, what it is *not*: there is no
//! replay of missed events. A delta says "this facet now looks like this",
//! never "this operation happened". A client that fell behind or reconnected
//! throws its runtime-bound state away and takes a fresh snapshot — the same
//! reasoning `change_log` uses for foreign database writes (§2.2).

use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::effects::EffectsSnapshot;
use reprise_runtime_protocol::jobs::JobSnapshot;
use reprise_runtime_protocol::playback::PlaybackSnapshot;
use reprise_runtime_protocol::queue::QueueSnapshot;
use reprise_runtime_protocol::ProtocolVersion;

/// One facet of runtime state, in its entirety, after a change.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    PlaybackChanged(PlaybackSnapshot),
    QueueChanged(QueueSnapshot),
    EffectsChanged(EffectsSnapshot),
    DeviceRunChanged(DeviceRunSnapshot),
    JobChanged(JobSnapshot),
}

/// An event together with its position in the runtime's single, global
/// order. Sequence numbers are strictly increasing and shared by all
/// clients, so "did A happen before B" has one answer for everyone.
#[derive(Debug, Clone, PartialEq)]
pub struct SequencedEvent {
    pub sequence: u64,
    /// Whose command caused this, if a command did.
    ///
    /// §9.7 asks every mutation to be attributable to what provoked it. It
    /// is also what lets a surface tell its own change from somebody else's:
    /// RUN-5 says an external change is followed quietly, which is only
    /// decidable if "external" is decidable.
    ///
    /// `None` is not "unknown" but "nobody asked": a position tick, a track
    /// ending, an idle deadline. Attributing those to whichever client
    /// happened to command last would make the attribution worse than
    /// absent, because it would look reliable.
    pub initiator: Option<crate::client::ClientId>,
    pub event: RuntimeEvent,
}

/// Everything a freshly connected client needs to render the runtime-bound
/// half of the interface. The database-bound half (library, playlists,
/// settings) is not here on purpose: clients read that directly through
/// `reprise-core` (§9.1) and it would only go stale in transit.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshot {
    /// The version the runtime serves, so a client can knowingly skip a
    /// feature an older peer lacks instead of discovering an absent field.
    pub protocol: ProtocolVersion,
    /// The sequence this snapshot was taken at. Every event the client
    /// receives afterwards carries a strictly greater one, which is what
    /// makes "snapshot, then deltas" gap-free without a replay log.
    pub sequence: u64,
    pub playback: PlaybackSnapshot,
    pub queue: QueueSnapshot,
    pub effects: EffectsSnapshot,
    pub device_runs: Vec<DeviceRunSnapshot>,
    pub jobs: Vec<JobSnapshot>,
}

/// What one drain hands a client.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub events: Vec<SequencedEvent>,
    /// Set when the client's mailbox overflowed and older events were
    /// dropped. The client must take a fresh snapshot; applying the
    /// remaining deltas on top of a state that missed some would be exactly
    /// the silent divergence the sequence numbers exist to prevent.
    pub resynchronize: bool,
}
