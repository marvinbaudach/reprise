//! Editing the queue.
//!
//! Split out of `transport.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. Editing the queue is a subject of
//! its own: every arm answers one extra question beyond "did it work" —
//! how many entries it actually changed, which is what a surface puts in a
//! toast and cannot recover from a snapshot afterwards.

use reprise_core::playback::PlaybackBackend;
use reprise_core::up_next::UpNextQueue;
use reprise_runtime_protocol::queue::QueueCommand;

use super::{as_index, Source, Transport};
use crate::error::{Rejected, RuntimeError};
use crate::ports::LibraryPort;

impl Transport {
    pub(crate) fn queue_command(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        command: &QueueCommand,
    ) -> Result<u64, RuntimeError> {
        // Every arm answers the same question: how many entries did this
        // actually change. Not how many it was handed — a purge for a track
        // that was never queued changed nothing, and saying otherwise would
        // put a number in a toast that the user can see is wrong.
        let affected = match command {
            // Both ends of the *explicit* queue, not one of each: "play
            // next" jumps the manual line and "add to queue" joins its back.
            // Neither touches the surrounding context, which is what makes
            // them undoable by clearing the queue.
            QueueCommand::AddNext(ids) => {
                self.up_next.prepend(ids);
                ids.len()
            }
            QueueCommand::AddLast(ids) => {
                self.up_next.append(ids);
                ids.len()
            }
            // "Clearing a queue is not a stop command" (protocol): only the
            // explicit queue goes; the current track keeps playing.
            QueueCommand::Clear => {
                let dropped = self.up_next.len();
                self.up_next = UpNextQueue::default();
                dropped
            }
            QueueCommand::Move { from, to, .. } => {
                let (from, to) = (as_index(*from), as_index(*to));
                if !self.up_next.move_item(from, to) {
                    return Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry));
                }
                1
            }
            QueueCommand::RemoveAt { positions, .. } => {
                let positions: Vec<usize> = positions.iter().map(|at| as_index(*at)).collect();
                let removed = self.up_next.remove_positions(&positions);
                if removed == 0 {
                    return Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry));
                }
                removed
            }
            QueueCommand::RemoveContextAt { positions, .. } => {
                let positions: Vec<usize> = positions.iter().map(|at| as_index(*at)).collect();
                let removed = self.queue.remove_order_positions(&positions);
                if removed == 0 {
                    return Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry));
                }
                removed
            }
            QueueCommand::PlayNextAt { position, .. } => {
                let track_id = self
                    .up_next
                    .take_at(as_index(*position))
                    .ok_or(RuntimeError::Rejected(Rejected::NoSuchQueueEntry))?;
                return self
                    .start(backend, library, track_id, Source::PlayNext)
                    .map(|()| 1);
            }
            QueueCommand::PlayContextAt { position, .. } => {
                let track_id = self
                    .queue
                    .play_order_position_now(as_index(*position))
                    .ok_or(RuntimeError::Rejected(Rejected::NoSuchQueueEntry))?;
                return self
                    .start(backend, library, track_id, Source::Context)
                    .map(|()| 1);
            }
            QueueCommand::Purge(ids) => {
                let from_up_next = self.up_next.remove_ids(ids);
                // `_except_current` deliberately: a track that is playing
                // when its file is deleted finishes, because stopping the
                // music is not what deleting a file asked for.
                from_up_next + self.queue.remove_ids_except_current(ids)
            }
        };
        Ok(affected as u64)
    }
}
