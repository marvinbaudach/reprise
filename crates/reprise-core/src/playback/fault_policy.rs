//! Platform-neutral response to a fault of the currently playing track.

/// The one user-facing notice a playback fault is allowed to produce.
/// Frontends translate these semantic variants at their presentation edge;
/// keeping the cardinality in [`PlaybackFaultPolicy`] makes FB-6's "one
/// toast" rule a core policy rather than an accident of one frontend branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackFaultNotice {
    /// The file vanished while it was playing: mark it missing, skip, and
    /// explain that availability—not decoding—caused the skip.
    TrackUnavailableSkipped,
    /// The file still exists but the backend could not play it.
    CouldNotPlaySkipped,
}

/// Complete effect policy for one fault of the currently playing track.
/// Background watcher events never construct this value and therefore stay
/// silent; only the player fault path consumes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackFaultPolicy {
    pub mark_missing: bool,
    pub skip: bool,
    /// Exactly one notice by construction. An array is deliberate: a future
    /// edit cannot silently add a second toast without changing this API and
    /// its FB-6 acceptance test.
    pub notices: [PlaybackFaultNotice; 1],
}

/// Resolves a player backend fault from the strongest evidence available at
/// that moment: whether the track's path still names a file.
pub fn playback_fault_policy(file_exists: bool) -> PlaybackFaultPolicy {
    if file_exists {
        PlaybackFaultPolicy {
            mark_missing: false,
            skip: true,
            notices: [PlaybackFaultNotice::CouldNotPlaySkipped],
        }
    } else {
        PlaybackFaultPolicy {
            mark_missing: true,
            skip: true,
            notices: [PlaybackFaultNotice::TrackUnavailableSkipped],
        }
    }
}
