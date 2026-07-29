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
    /// Move one explicit-queue entry, by position.
    Move { from: u64, to: u64 },
    /// Drop explicit-queue entries by position. Positions rather than ids
    /// because the same track may sit in the queue more than once, and a
    /// user removing one row means that row.
    RemoveAt(Vec<u64>),
    /// Drop entries from the surrounding context, by play-order position.
    RemoveContextAt(Vec<u64>),
    /// Play the explicit-queue entry at this position now, taking it out of
    /// the queue.
    PlayNextAt(u64),
    /// Let the context entry at this play-order position jump the line and
    /// play now. Everything it passed stays queued, in order, right behind
    /// it — fast-forwarding the playhead onto it instead would drop those
    /// tracks out of the upcoming list, which reads as "my queue vanished".
    PlayContextAt(u64),
    /// Forget these track ids wherever they appear. This is a library
    /// deletion reaching the queue, not a user editing it: a track that is
    /// *currently playing* is left alone and finishes, because stopping the
    /// music is not what deleting a file asked for.
    Purge(Vec<i64>),
}
