//! Playback snapshots and commands.
//!
//! Field names match what `reprise-mcp` already publishes, so Stage 3's move
//! from the MPRIS-shaped read path to the runtime is a re-wiring, not a
//! rename that breaks every agent at once.

use serde::{Deserialize, Serialize};
use zvariant::{DeserializeDict, SerializeDict, Type};

/// Live playback state. Path-free by construction: a track is an id plus the
/// display strings a client would show anyway.
#[derive(Debug, Clone, PartialEq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct PlaybackSnapshot {
    /// `playing`, `paused`, `stopped`.
    pub status: String,
    /// Absent when the loaded item is not a library track (a radio stream or
    /// a podcast episode), so a client never invents a library id.
    pub track_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub position_ms: i64,
    /// Inclusive 0.0..=1.0.
    pub volume: f64,
    pub shuffle: bool,
    /// `off`, `all`, `one`.
    pub repeat: String,
    /// Why the last automatic start did not happen: `not_playable` when
    /// nothing could be resolved for the id, `backend` when the pipeline
    /// refused it or gave up mid-track. Absent once anything plays again.
    ///
    /// A short kind, never a path and never the backend's own message — the
    /// same allow-list [`crate::jobs::JobSnapshot::error_kind`] follows, for
    /// the same reason (§9.7).
    ///
    /// This is state, not a log entry: "playback is stopped *because* a track
    /// failed" and "playback is stopped because the queue ran out" are
    /// different situations that are otherwise indistinguishable, and §9.5
    /// only lets a facet say what it looks like now — never that an operation
    /// happened.
    pub failure_kind: Option<String>,
    /// The library track the failure was about. Absent when what failed had
    /// no library id, and absent when `failure_kind` is.
    pub failure_track_id: Option<i64>,
}

/// Something to play that is not a library track: a radio stream, a podcast
/// episode, a preview render.
///
/// The runtime owns the pipeline, so it has to be told what to put in it —
/// but it deliberately does not own *finding* it. Resolving a station's
/// current stream URL, or a video's audio track, is network work with its
/// own retries and its own failures; the surface that already does it keeps
/// doing it and hands over the result. What crosses here is the smallest
/// thing that cannot be anywhere else: a location and what to show.
///
/// `location` travels **inward only**. No snapshot carries it back, which is
/// what keeps the path-free rule (§9.7) true even when the thing playing is
/// a local file nobody has a library id for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMedia {
    /// A local path, or a remote URI when `remote` is set.
    pub location: String,
    /// Whether `location` is a URI for the backend's remote entry point.
    /// Explicit rather than sniffed from the string: a local path that
    /// happens to contain `://` would otherwise be opened as a URL.
    pub remote: bool,
    pub title: String,
    /// The show, the station, or empty. Whatever the surface would display
    /// where an artist normally goes.
    pub artist: String,
    /// Zero when unknown — a live stream has no duration, and a podcast's is
    /// sometimes only learned once it plays.
    pub duration_ms: i64,
}

/// A playback command.
///
/// Deliberately not a wire type: D-Bus has no sum type, so encoding this as
/// one payload would mean an `action` string plus a bag of optional fields
/// where exactly one combination is valid — the same positional fragility in
/// a new costume. On the wire each variant is its own typed method. This
/// enum is the in-process command language the runtime's reducers consume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    /// Absolute volume in the inclusive 0.0..=1.0 range; the runtime clamps
    /// and reports the applied value in the next snapshot.
    SetVolume(f64),
    /// Relative seek in milliseconds; negative seeks backwards.
    Seek(i64),
    SetShuffle(bool),
    /// `off`, `all`, `one`.
    SetRepeat(String),
}
