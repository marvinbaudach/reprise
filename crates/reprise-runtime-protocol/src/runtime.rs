//! The complete runtime-bound state, as one dictionary.
//!
//! This is what a client receives when it connects and whenever it has to
//! start over (§9.5). It carries only state the runtime *owns*: the library,
//! playlists and settings are read straight from SQLite by every surface and
//! would only go stale in transit.
//!
//! The protocol version travels inside the snapshot rather than beside it so
//! a client cannot end up holding a payload without knowing which contract
//! produced it.

use zvariant::{DeserializeDict, SerializeDict, Type};

use crate::device_run::DeviceRunSnapshot;
use crate::jobs::JobSnapshot;
use crate::playback::PlaybackSnapshot;
use crate::queue::QueueSnapshot;

/// Everything the runtime owns, at one instant.
#[derive(Debug, Clone, PartialEq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct RuntimeSnapshot {
    /// The version the *runtime* speaks, which a client compares against its
    /// own to know whether it is talking to an older peer.
    pub protocol_major: u32,
    pub protocol_minor: u32,
    /// The position in the runtime's single event order that this snapshot
    /// describes. Every delta the client subsequently receives carries a
    /// strictly greater value, which is what makes "snapshot, then deltas"
    /// gap-free without a replay log.
    pub sequence: u64,
    /// The id this runtime gave *this* client. Every event carries the id of
    /// whoever provoked it, so a surface compares the two to tell its own
    /// change from somebody else's — which is what RUN-5 turns on, and what
    /// the quit policy reads off `PlaybackSnapshot::initiated_by`.
    pub client_id: u64,
    pub playback: PlaybackSnapshot,
    pub queue: QueueSnapshot,
    /// Only devices this runtime has actually run. An absent device is
    /// absent, not a row of zeros.
    pub device_runs: Vec<DeviceRunSnapshot>,
    pub jobs: Vec<JobSnapshot>,
}
