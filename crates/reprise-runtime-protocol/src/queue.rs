//! Queue snapshots and commands.

use serde::{Deserialize, Serialize};
use zvariant::{DeserializeDict, SerializeDict, Type};

/// Bounded live queue state. The id windows are capped; the totals describe
/// the complete sections, so a client can say "and 412 more" without the
/// runtime shipping 412 ids.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct QueueSnapshot {
    /// How many times this facet has changed, counted by the runtime.
    ///
    /// Every position in this snapshot is only meaningful against *this*
    /// revision. A client sends it back with any command that names a row,
    /// and a command carrying an older one is rejected rather than applied
    /// to whichever row is there now (§9.5: a client refreshes, it does not
    /// replay). It counts observable changes to the queue facet, not user
    /// edits — a track ending renumbers the context window just as surely as
    /// an edit does.
    ///
    /// It follows the *whole* queue, not the windows below. It used to
    /// follow only what this snapshot carried, which was defensible while
    /// nothing could name a deeper position — but [`QueuePage`] hands them
    /// out, so a reorder past the two-hundredth row now renumbers rows a
    /// client is holding, and has to move the revision like any other.
    pub revision: u64,
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
    Move {
        from: u64,
        to: u64,
        expected_revision: u64,
    },
    /// Drop explicit-queue entries by position. Positions rather than ids
    /// because the same track may sit in the queue more than once, and a
    /// user removing one row means that row.
    RemoveAt {
        positions: Vec<u64>,
        expected_revision: u64,
    },
    /// Drop entries from the surrounding context, by play-order position.
    RemoveContextAt {
        positions: Vec<u64>,
        expected_revision: u64,
    },
    /// Play the explicit-queue entry at this position now, taking it out of
    /// the queue.
    PlayNextAt {
        position: u64,
        expected_revision: u64,
    },
    /// Let the context entry at this play-order position jump the line and
    /// play now. Everything it passed stays queued, in order, right behind
    /// it — fast-forwarding the playhead onto it instead would drop those
    /// tracks out of the upcoming list, which reads as "my queue vanished".
    PlayContextAt {
        position: u64,
        expected_revision: u64,
    },
    /// Forget these track ids wherever they appear. This is a library
    /// deletion reaching the queue, not a user editing it: a track that is
    /// *currently playing* is left alone and finishes, because stopping the
    /// music is not what deleting a file asked for.
    Purge(Vec<i64>),
}

impl QueueCommand {
    /// The queue revision this command's positions were read from, for the
    /// commands that name a row at all.
    ///
    /// `None` is not "skip the check" but "there is no row to be wrong
    /// about": `AddNext`, `AddLast`, `Purge` name tracks, and `Clear` names
    /// the whole queue. Demanding a revision from them would make "add this
    /// album" fail because something finished playing in the meantime.
    #[must_use]
    pub fn expected_revision(&self) -> Option<u64> {
        match self {
            Self::Move {
                expected_revision, ..
            }
            | Self::RemoveAt {
                expected_revision, ..
            }
            | Self::RemoveContextAt {
                expected_revision, ..
            }
            | Self::PlayNextAt {
                expected_revision, ..
            }
            | Self::PlayContextAt {
                expected_revision, ..
            } => Some(*expected_revision),
            Self::AddNext(_) | Self::AddLast(_) | Self::Clear | Self::Purge(_) => None,
        }
    }
}

/// Which of the two queues a page is read from.
///
/// A string on the wire, like every other closed vocabulary here: an integer
/// would make an off-by-one silently read the other queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueSection {
    PlayNext,
    Context,
}

impl QueueSection {
    /// The wire spelling, or `None` for a word this build does not know.
    #[must_use]
    pub fn from_wire(name: &str) -> Option<Self> {
        match name {
            "play_next" => Some(Self::PlayNext),
            "context" => Some(Self::Context),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::PlayNext => "play_next",
            Self::Context => "context",
        }
    }
}

/// One window of a queue section, for a view scrolled past what the snapshot
/// carries.
///
/// Carries its own `revision` and `offset` rather than leaving the caller to
/// remember what it asked for. A page that arrived while the queue moved
/// underneath describes rows that are no longer at those positions, and a
/// client comparing the revision it holds against this one is how it finds
/// out — the same check a positional command makes before it is applied.
#[derive(Debug, Clone, PartialEq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct QueuePage {
    pub revision: u64,
    /// `play_next` or `context`.
    pub section: String,
    /// Where this window starts within its section.
    pub offset: u64,
    pub track_ids: Vec<i64>,
    /// How long the whole section is, so a view can size its scrollbar
    /// without asking for every page.
    pub total: u64,
}
