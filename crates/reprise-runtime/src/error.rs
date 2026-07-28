//! The four outcome categories every client interaction ends in.
//!
//! `docs/plans/multi-frontend-core.md` §9.7 fixes them for interfaces *and*
//! for agents, with two binding properties that this module enforces by
//! construction rather than by review:
//!
//! * **Structured, never free text.** Every variant is an enum; the only
//!   payloads are numbers and a [`Capability`]. There is no `String` to put
//!   a backend's message into, so nobody can.
//! * **Path-free.** A local filesystem path cannot appear in a value that
//!   holds no strings. This is why a failed device transfer reports the
//!   *kind* of failure and the affected track ids (in the snapshot), not the
//!   file it could not read.

use reprise_runtime_protocol::ProtocolVersion;

/// A permission a client must hold to mutate a resource, named exactly as in
/// the capability matrix (§9.8) and in `reprise-mcp`'s settings keys, so the
/// two never drift into different vocabularies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Transport, queue and targeted play.
    PlaybackControl,
    /// Device configuration and runs.
    DeviceSync,
    /// Creating and tending instrumental jobs.
    AiCreate,
}

impl Capability {
    /// The matrix's name for this capability.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlaybackControl => "playback:control",
            Self::DeviceSync => "device:sync",
            Self::AiCreate => "ai:create",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why the runtime could not be reached at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// The command carried a client id the runtime does not know — it was
    /// never connected, or it disconnected. §9.5 is explicit that such a
    /// command is *not* buffered for later: executing a stale intention
    /// after a reconnect is the more dangerous failure.
    NotConnected,
}

/// Why a connection attempt was turned away for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The client speaks a different protocol major version. Only the major
    /// decides (see [`ProtocolVersion::is_compatible_with`]); both versions
    /// travel with the refusal so the client can name the mismatch.
    ProtocolMajor {
        runtime: ProtocolVersion,
        client: ProtocolVersion,
    },
}

/// Why a formally valid command was not admissible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    /// The client does not hold the capability this mutation hangs on.
    MissingCapability(Capability),
    /// A run for this device is already in flight. Two clients starting the
    /// same device is an ordinary race, not a reason to start twice.
    DeviceAlreadyRunning,
    /// Cancel arrived when nothing was running.
    NoRunToCancel,
    /// Transport was asked to play with nothing loaded and nothing queued.
    NothingToPlay,
    /// A repeat mode outside `off`, `all`, `one`.
    UnknownRepeatMode,
    /// The job id does not exist, or is already terminal.
    UnknownJob,
    /// The command is part of the protocol but not yet served here. Saving
    /// and discarding a staged render need the staging store and land with
    /// Task 3.5; rejecting them loudly beats pretending they worked.
    UnsupportedCommand,
}

/// Why an effect that actually ran did not succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    /// The audio backend refused the location (missing codec, unreadable
    /// file, bad URI). The backend's own message stays in the log, where
    /// paths are allowed; it does not travel to a client.
    PlaybackBackend,
    /// The library does not resolve this track id to anything playable.
    TrackNotPlayable,
    /// A write through the runtime's database connection failed. The
    /// underlying `rusqlite` error is logged, where its detail belongs; a
    /// client gets the category and nothing to parse.
    Database,
}

/// The single error type every client-facing entry point returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeError {
    Unavailable(Unavailable),
    Refused(Refused),
    Rejected(Rejected),
    Failed(Failed),
}

impl RuntimeError {
    /// The category, as the short kind a client or an agent branches on.
    #[must_use]
    pub fn category(self) -> &'static str {
        match self {
            Self::Unavailable(_) => "unavailable",
            Self::Refused(_) => "refused",
            Self::Rejected(_) => "rejected",
            Self::Failed(_) => "failed",
        }
    }

    /// The full diagnostic kind, `category:reason`. Stable enough to match
    /// on and short enough to log; carries no prose and no path.
    #[must_use]
    pub fn kind(self) -> String {
        let reason = match self {
            Self::Unavailable(Unavailable::NotConnected) => "not_connected",
            Self::Refused(Refused::ProtocolMajor { .. }) => "protocol_major",
            Self::Rejected(Rejected::MissingCapability(capability)) => {
                return format!("rejected:missing_capability:{capability}");
            }
            Self::Rejected(Rejected::DeviceAlreadyRunning) => "device_already_running",
            Self::Rejected(Rejected::NoRunToCancel) => "no_run_to_cancel",
            Self::Rejected(Rejected::NothingToPlay) => "nothing_to_play",
            Self::Rejected(Rejected::UnknownRepeatMode) => "unknown_repeat_mode",
            Self::Rejected(Rejected::UnknownJob) => "unknown_job",
            Self::Rejected(Rejected::UnsupportedCommand) => "unsupported_command",
            Self::Failed(Failed::PlaybackBackend) => "playback_backend",
            Self::Failed(Failed::TrackNotPlayable) => "track_not_playable",
            Self::Failed(Failed::Database) => "database",
        };
        format!("{}:{reason}", self.category())
    }

    /// Whether repeating the identical command could plausibly help. §9.7
    /// gives the answer per category: only `Unavailable` is worth retrying
    /// on the client's own initiative — `Failed` is a user decision, and the
    /// other two will not change by being asked again.
    #[must_use]
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Unavailable(_))
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.kind())
    }
}

impl std::error::Error for RuntimeError {}

/// Turns a database error into the client-facing category, logging the part
/// that must not travel. Every database call in this crate goes through here
/// so no `rusqlite::Error` can reach a client by accident.
pub(crate) fn failed_database(error: &rusqlite::Error) -> RuntimeError {
    tracing::error!(%error, "runtime database operation failed");
    RuntimeError::Failed(Failed::Database)
}
