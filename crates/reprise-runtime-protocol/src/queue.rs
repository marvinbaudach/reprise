//! Queue snapshots and commands.

use serde::{Deserialize, Serialize};
use zvariant::{DeserializeDict, SerializeDict, Type};

/// Bounded live queue state. The id windows are capped; the totals describe
/// the complete sections, so a client can say "and 412 more" without the
/// runtime shipping 412 ids.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct QueueSnapshot {
    pub current_track_id: Option<i64>,
    /// Explicitly queued items, in play order.
    pub play_next_track_ids: Vec<i64>,
    /// The surrounding context the queue was started from, in play order.
    pub context_track_ids: Vec<i64>,
    pub play_next_total: u64,
    pub context_total: u64,
}

/// A queue command. In-process only, for the same reason as
/// [`crate::playback::PlaybackCommand`]: on the wire each variant is its own
/// typed method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueCommand {
    /// Insert directly after the current item, in the given order.
    AddNext(Vec<i64>),
    /// Append to the end of the explicit queue, in the given order.
    AddLast(Vec<i64>),
    /// Drop the explicit queue. The current item keeps playing — clearing a
    /// queue is not a stop command.
    Clear,
}
