//! Background-job snapshots and commands.
//!
//! The strict metadata allow-list from the MCP job surface carries over
//! unchanged: opaque ids, state, progress and timestamps. Never a source
//! path, a render path or a staging location — not even for a failed job,
//! where a path is exactly what one is tempted to include.

use serde::{Deserialize, Serialize};
use zvariant::{DeserializeDict, SerializeDict, Type};

/// One background job.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct JobSnapshot {
    pub job_id: i64,
    /// What kind of work this is, as a short kind: `scan`, `download`,
    /// `instrumental`.
    pub kind: String,
    /// `queued`, `running`, `staged`, `saved`, `failed`, `cancelled`.
    pub state: String,
    /// Progress in permille, so a client never divides by a zero total.
    pub progress_permille: u16,
    /// Groups jobs that were created together.
    pub batch_id: Option<String>,
    pub source_track_id: Option<i64>,
    /// The saved library track once a render was promoted; absent while
    /// queued, running, or staged-but-unsaved.
    pub result_track_id: Option<i64>,
    pub cancel_requested: bool,
    /// A short diagnostic kind for a failed job — never a path, never the
    /// underlying error's message.
    pub error_kind: Option<String>,
}

/// Aggregate progress over a batch.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct BatchProgress {
    pub batch_id: String,
    pub total: u64,
    pub finished: u64,
    pub failed: u64,
    pub progress_permille: u16,
}

/// A job command. In-process only, for the same reason as
/// [`crate::playback::PlaybackCommand`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobCommand {
    /// Ask the runtime to stop a job. Cancellation is a request, never an
    /// assertion: the snapshot's `cancel_requested` reports that it was
    /// asked, `state` reports what actually happened.
    Cancel(i64),
    /// Promote a staged render to a permanent library track.
    Save(i64),
    /// Drop a staged render without saving it.
    Discard(i64),
}
