//! Putting back a queue a surface saved when it last closed.
//!
//! Split out of `transport.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. Restoring is its own subject and
//! the only one that fills the queue *without* starting it — every other
//! path here plays what it loads.

use reprise_core::queue::{Queue, Repeat};
use reprise_core::up_next::UpNextQueue;
use reprise_runtime_protocol::session::RestoredQueue;

use super::{as_index, Transport};
use crate::error::{Rejected, RuntimeError};

impl Transport {
    /// Puts back a saved queue without starting it.
    ///
    /// Validated whole before anything is applied: the core's own
    /// `restore_snapshot` checks that the play order is a permutation of the
    /// ids and that the cursor points inside them. A session file that got
    /// corrupted rejects rather than half-loading, because the alternative
    /// is a running player replaced by a broken one.
    ///
    /// Nothing is loaded and nothing plays. What the cursor points at is
    /// what `Play` will start, which is how a user carries on where they
    /// left off rather than from the top.
    pub(crate) fn restore_session(
        &mut self,
        context: &RestoredQueue,
        play_next: &[i64],
    ) -> Result<(), RuntimeError> {
        let stored = reprise_core::queue::QueueSnapshot {
            ids: context.track_ids.clone(),
            order: context.play_order.iter().map(|&at| as_index(at)).collect(),
            position: context.position.map(as_index),
            repeat: repeat_from(&context.repeat)?,
            shuffled: context.shuffled,
        };
        let mut restored = Queue::new();
        restored.restore_snapshot(stored).map_err(|error| {
            tracing::warn!(%error, "stored session queue could not be read back");
            RuntimeError::Rejected(Rejected::UnusableSession)
        })?;
        self.queue = restored;
        self.up_next = UpNextQueue::default();
        self.up_next.append(play_next);
        Ok(())
    }
}

/// The runtime's repeat vocabulary, in from the wire. The same three words
/// `SetRepeat` accepts, rejected the same way: a stored session naming a
/// mode this build does not know is a session that cannot be read back.
fn repeat_from(mode: &str) -> Result<Repeat, RuntimeError> {
    match mode {
        "off" => Ok(Repeat::Off),
        "all" => Ok(Repeat::All),
        "one" => Ok(Repeat::One),
        _ => Err(RuntimeError::Rejected(Rejected::UnknownRepeatMode)),
    }
}
