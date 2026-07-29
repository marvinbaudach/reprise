//! Putting back a queue a surface saved when it last closed.
//!
//! Restoring is deliberately its own command rather than a flag on
//! `PlayTracks`: every other way of filling the queue also starts it, and
//! opening the application is not a request to play.

use serde::{Deserialize, Serialize};
use zvariant::{DeserializeDict, SerializeDict, Type};

/// A stored context queue, in the shape a surface persisted it.
///
/// Carries the play order rather than only the ids. Restoring the ids and
/// reshuffling would change what comes next behind the back of a user who
/// left mid-session — the order is part of what was saved, not a detail the
/// runtime is free to regenerate.
///
/// Path-free like everything else here: a queue is ids, and what those ids
/// resolve to is the runtime's business.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct RestoredQueue {
    pub track_ids: Vec<i64>,
    /// The play order, as indices into `track_ids`. Must be a permutation of
    /// them; anything else is a stored session that cannot be read back.
    pub play_order: Vec<u64>,
    /// Where the playhead stood, as an index into `play_order`. Absent for a
    /// queue that was never started.
    pub position: Option<u64>,
    /// `off`, `all`, `one`.
    pub repeat: String,
    pub shuffled: bool,
}

/// A restore command. In-process only, for the same reason as
/// [`crate::playback::PlaybackCommand`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreSession {
    pub context: RestoredQueue,
    /// What the user had queued by hand. Ids only: the explicit queue has no
    /// order of its own beyond the one it is written in.
    pub play_next: Vec<i64>,
}
