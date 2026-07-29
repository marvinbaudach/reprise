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
    /// Which client started what is loaded, as the id that client was handed
    /// at connect. Absent when nothing is loaded.
    ///
    /// A surface needs this to answer "is this mine?" — the question the quit
    /// policy turns on: a window that closes stops the playback it started
    /// and leaves alone the playback an agent started. Comparing titles or
    /// track ids cannot answer it, because two clients may well have asked
    /// for the same track.
    ///
    /// An id is not reused after a disconnect, so a reconnected client
    /// correctly stops recognising playback its previous session began.
    pub initiated_by: Option<u64>,
    /// What the surface calls the non-library item that is loaded — the same
    /// opaque string it passed in, echoed back. Absent for a library track and
    /// when nothing is loaded.
    ///
    /// The runtime never parses it. It is an identity, and the only question
    /// it answers is "is this still the same thing?" — which nothing else can
    /// answer: two episodes of a show share a title, and `track_id` is absent
    /// for exactly the items this is about. The MPRIS mirror needs it to keep
    /// a position across a rebuild instead of resetting it to zero, and to
    /// give the item a stable object path.
    pub external_ref: Option<String>,
    /// Whether what is loaded has no end to seek within — a radio stream.
    ///
    /// Its own field rather than `duration_ms == 0`, which is not a safe
    /// stand-in: a podcast episode whose duration is not yet known reports
    /// zero too, and it is perfectly seekable. Deriving it would disable the
    /// seek bar on every episode until the first duration arrives.
    pub live: bool,
    /// Why playback is stopped, when the runtime knows something the status
    /// alone does not say. Today the only value is `finished`: what was
    /// loaded played to its end, rather than being stopped by anyone.
    ///
    /// A client must ignore a value it does not know — that is what makes a
    /// new one an additive change.
    ///
    /// The distinction is not cosmetic. A podcast episode that finished is
    /// marked played and hands the show on to the next unplayed episode; one
    /// the user stopped halfway is neither. A surface reading only `stopped`
    /// cannot tell those apart, and guessing either way is wrong in one of
    /// them.
    ///
    /// Absent when a failure stopped playback: [`Self::failure_kind`] already
    /// says so, and saying it twice invites the two to disagree.
    pub stopped_reason: Option<String>,
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
///
/// Crosses the bus as one `a{sv}` argument rather than as seven positional
/// ones. Positionally this would be three consecutive `String`s and two
/// consecutive `bool`s — swap `title` with `artist`, or `remote` with `live`,
/// and it compiles, passes the signature check, and is wrong at runtime.
/// That is the exact failure this crate was created to end, and it does not
/// stop being that failure because the arguments are on a method instead of
/// in a tuple.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
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
    /// What the surface calls this item, echoed back in every snapshot as
    /// [`PlaybackSnapshot::external_ref`]. Opaque to the runtime.
    ///
    /// The surface picks the scheme; GTK uses `podcast/{id}` and
    /// `radio/{id}`. Empty means the surface offered no identity, and the
    /// snapshot then reports none rather than an empty string, so "no
    /// identity" and "the identity is nothing" stay one case instead of two.
    pub external_ref: String,
    /// Whether this has no end — a radio stream. See
    /// [`PlaybackSnapshot::live`] for why it is not derived from
    /// `duration_ms`.
    pub live: bool,
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
