//! Everything the runtime needs from the outside world, as traits.
//!
//! The runtime itself is a synchronous reducer over owned state: a command
//! comes in, state changes, events go out. Anything that touches audio
//! hardware, a device's filesystem or the wall clock sits behind one of the
//! traits here, which is what lets the whole runtime run in a unit test with
//! no display, no audio and no media files (Task 3.1).
//!
//! Two of the ports are *asynchronous in production and synchronous in the
//! type system*: [`DeviceEffects`] starts work and returns immediately, and
//! the answer comes back through [`crate::Runtime::on_device_event`]. That is
//! the same shape the GTK driver already uses for device runs, so wiring the
//! real Linux effects to it in Task 3.2 is a re-parenting, not a rewrite.

use reprise_core::device_sync::machine::Effect;
use reprise_core::playback::PlaybackBackend;

/// Where a track's audio actually is. The runtime resolves this internally
/// and hands it to the audio backend; it never travels to a client, which is
/// what keeps §9.7's path-freedom mechanical rather than aspirational.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackLocation {
    /// A local file, for [`PlaybackBackend::play`].
    Path(String),
    /// A remote stream, for [`PlaybackBackend::play_uri`].
    Uri(String),
}

/// What the runtime needs in order to play a library track and describe what
/// is playing. Deliberately the smallest set that fills a
/// [`reprise_runtime_protocol::playback::PlaybackSnapshot`] plus the location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayableTrack {
    pub track_id: i64,
    pub location: TrackLocation,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
}

/// Resolves library ids to something playable.
///
/// This is a port rather than a direct query through the runtime's own
/// connection because the resolution rules (missing files, instrumental
/// substitutions, podcast episodes) live where the library does, and because
/// a fake map makes every transport test independent of database fixtures.
pub trait LibraryPort {
    /// `None` when the id is unknown or has nothing playable behind it.
    fn resolve(&self, track_id: i64) -> Option<PlayableTrack>;
}

/// Performs the side effects a [`reprise_core::device_sync::machine::
/// DeviceSyncMachine`] asks for.
///
/// Every method returns immediately. Results arrive later through
/// [`crate::Runtime::on_device_plan`] and [`crate::Runtime::on_device_event`],
/// which is why the runtime stays single-threaded and testable: the test
/// *is* the port, and it answers exactly when it wants to.
pub trait DeviceEffects {
    /// Begins computing what a run would change. Answered with
    /// [`crate::Runtime::on_device_plan`].
    fn plan(&self, device: &str);

    /// Performs one effect. Answered with the matching
    /// [`reprise_core::device_sync::machine::Event`] through
    /// [`crate::Runtime::on_device_event`].
    fn perform(&self, device: &str, effect: Effect);
}

/// Time, injected so leases, timestamps and transfer rates are testable
/// without sleeping — the same reason `reprise-core`'s job facades take
/// `now: i64` instead of reading the clock themselves.
pub trait Clock {
    /// Seconds since the Unix epoch, UTC. This is what gets stored, so it
    /// has to be the same notion of "now" the rest of the database uses.
    fn now_unix(&self) -> i64;

    /// Milliseconds from an arbitrary but monotonic origin. Separate from
    /// [`Self::now_unix`] because measuring a transfer rate against a clock
    /// that an NTP step can move backwards produces nonsense readings, and
    /// because seconds are too coarse to divide by.
    fn now_monotonic_ms(&self) -> u64;
}

/// The complete set of outside-world dependencies, assembled once at start.
///
/// A struct rather than four constructor arguments so adding a fifth port
/// later is a field, not a signature change at every call site.
pub struct Ports {
    pub playback: Box<dyn PlaybackBackend>,
    pub library: Box<dyn LibraryPort>,
    pub devices: Box<dyn DeviceEffects>,
    pub clock: Box<dyn Clock>,
}
