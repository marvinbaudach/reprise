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
