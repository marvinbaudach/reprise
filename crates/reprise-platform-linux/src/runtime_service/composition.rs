//! The real ports, assembled.
//!
//! [`crate::runtime_service::service`] serves whatever [`Runtime`] it is
//! given; this is where the Linux one is built. Keeping the two apart is
//! what lets a test serve a runtime made of fakes over a private bus name
//! and still exercise the identical serving loop.

use std::path::Path;

use reprise_core::playback::PlayerEvent;
use reprise_core::queries;
use reprise_runtime::{
    Clock, DeviceEffects, LibraryPort, PlayableTrack, Ports, Runtime, TrackLocation,
};
use rusqlite::Connection;

use super::service::Request;

/// Resolves library ids through `reprise-core`'s own facade.
///
/// A second connection, not the writer: resolving a track is a read on the
/// hot path of starting playback, and WAL carries any number of readers. The
/// writer stays free for the effects that actually need it.
struct DatabaseLibrary {
    conn: Connection,
}

impl LibraryPort for DatabaseLibrary {
    fn resolve(&self, track_id: i64) -> Option<PlayableTrack> {
        let summary = queries::query_track_summary(&self.conn, track_id)
            .inspect_err(|error| tracing::warn!(track_id, %error, "track lookup failed"))
            .ok()
            .flatten()?;
        Some(PlayableTrack {
            track_id,
            // Every row in `tracks` is a local file; remote sources (radio,
            // podcast episodes) are separate entities and reach the runtime
            // by their own route, not through a library id.
            location: TrackLocation::Path(summary.path),
            title: summary.title,
            artist: summary.artist,
            album: summary.album,
            duration_ms: summary.duration_ms,
        })
    }
}

/// The system clock.
struct SystemClock {
    started: std::time::Instant,
}

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
            })
    }

    fn now_monotonic_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Device effects, routed back into the serving loop.
///
/// The Linux effect driver — clean partials, transcode, copy, write playlist
/// — still lives in the GTK crate, where Task 2.2 left it, and moves here in
/// Task 3.3's device slice. Until then a run started over the bus reports a
/// structured planning failure rather than appearing to start and then doing
/// nothing, which is the failure mode that would waste a user's time.
struct UnmigratedDeviceEffects {
    requests: async_channel::Sender<Request>,
}

impl DeviceEffects for UnmigratedDeviceEffects {
    fn plan(&self, device: &str) {
        tracing::warn!(
            device,
            "device runs are not served by the runtime yet (Task 3.3)"
        );
        let _ = self.requests.send_blocking(Request::DevicePlan {
            device: device.to_owned(),
            plan: None,
        });
    }

    fn perform(&self, device: &str, effect: reprise_core::device_sync::machine::Effect) {
        // Unreachable while `plan` never produces a machine; kept honest
        // rather than `unreachable!()`, because a panic here would take the
        // whole runtime down over an effect nobody is waiting for.
        tracing::error!(device, ?effect, "device effect requested with no driver");
    }
}

/// Everything a Linux runtime needs, plus the channel its audio backend
/// reports on.
pub struct Composition {
    pub runtime: Runtime,
    pub player_events: async_channel::Receiver<PlayerEvent>,
}

/// Builds the Linux runtime over an already-migrated database.
///
/// `requests` is the serving loop's own inbox: ports that finish work
/// asynchronously post their results back through it, which is what keeps
/// every mutation on the one thread that owns the runtime.
pub fn compose(
    database: &Path,
    requests: async_channel::Sender<Request>,
) -> Result<Composition, ComposeError> {
    let writer = reprise_core::db::open_migrated(Some(database)).map_err(ComposeError::Database)?;
    let reader = reprise_core::db::open_migrated(Some(database)).map_err(ComposeError::Database)?;

    let (events, player_events) = async_channel::unbounded::<PlayerEvent>();
    let player = crate::player::Player::new(Box::new(move |event| {
        // The backend reports from GStreamer's own threads; the channel is
        // the only place those threads and the runtime thread meet.
        let _ = events.send_blocking(event);
    }))
    .map_err(ComposeError::Playback)?;

    let ports = Ports {
        playback: Box::new(player),
        library: Box::new(DatabaseLibrary { conn: reader }),
        devices: Box::new(UnmigratedDeviceEffects { requests }),
        clock: Box::new(SystemClock {
            started: std::time::Instant::now(),
        }),
    };
    Ok(Composition {
        runtime: Runtime::new(writer, ports),
        player_events,
    })
}

/// Why the runtime could not be built at all.
#[derive(Debug)]
pub enum ComposeError {
    Database(reprise_core::db::DbError),
    Playback(reprise_core::playback::PlaybackError),
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database: {error}"),
            Self::Playback(error) => write!(formatter, "audio backend: {error}"),
        }
    }
}

impl std::error::Error for ComposeError {}
