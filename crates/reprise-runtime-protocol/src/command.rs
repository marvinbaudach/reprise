//! What a command did, as opposed to what the state looks like afterwards.
//!
//! Snapshots answer "what is true now". They cannot answer "what did the
//! thing I just asked for actually do", and a surface needs that to say "4
//! removed" or to offer an undo with a number in it. Counting the difference
//! between two snapshots is not the same answer: between them another client
//! may have changed the queue too, and the difference would credit this
//! command with somebody else's work.

use zvariant::{DeserializeDict, SerializeDict, Type};

/// The result of one command, for the client that sent it.
///
/// A dictionary rather than a tuple so a later field — a job id, an
/// identifier for something that was created — can be added without every
/// client having to be rebuilt to keep decoding the ones before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct CommandOutcome {
    /// The queue revision *after* this command.
    ///
    /// Shipped back so a surface can issue a follow-up positional command
    /// straight away — dragging a second row before the first drag's delta
    /// has come round — instead of being unable to speak until it hears its
    /// own echo. See [`crate::queue::QueueSnapshot::revision`].
    pub queue_revision: u64,
    /// How many queue entries this command added, removed or moved.
    ///
    /// Zero means it edited none, which is the truth about setting the
    /// volume; it never means "unknown". It counts entries that actually
    /// changed, not the ids or positions the command was handed — a purge
    /// for a track that was not queued affected nothing, and reporting
    /// otherwise would credit work that did not happen.
    pub affected: u64,
}
