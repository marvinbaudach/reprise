//! The Android session's play-count writer.
//!
//! Play counting is decided on Media3's application thread: Kotlin calls
//! [`crate::playback::PlaybackEventBridge::emit`] from it, and on this app that
//! thread is the main thread — `ExoPlayer` is built in
//! `ReprisePlaybackService.onCreate`. Both events that can cross the
//! half-played mark are ones that must not stall: a 500 ms position tick, and
//! `TrackFinished`, whose handler goes straight on to `play_uri` for the next
//! track. A blocking `Db::open_ready` (itself a `PRAGMA user_version` query)
//! plus an `UPDATE` sat on both.
//!
//! So the session owns one writer thread with one long-lived handle over the
//! same database file — the arrangement `Db`'s own documentation prescribes for
//! background work — and the playback thread only ever hands over an id and the
//! moment it happened.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use reprise_core::db::Db;

/// One counted play, timestamped where it happened rather than where it lands,
/// so a queued write cannot back-date `last_played_at` to the drain.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RecordedPlay {
    pub(crate) track_id: i64,
    pub(crate) at_unix: i64,
}

impl RecordedPlay {
    pub(crate) fn now(track_id: i64) -> Self {
        Self {
            track_id,
            at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }
}

/// Sends counted plays to a writer thread and waits for it on teardown.
pub(crate) struct PlayRecorder {
    /// `None` only while dropping: the sender has to go before the join, or
    /// the worker never sees the end of its channel.
    plays: Option<Sender<RecordedPlay>>,
    worker: Option<JoinHandle<()>>,
}

impl PlayRecorder {
    /// Starts the writer over `database_path`, which the caller has already
    /// opened and migrated.
    pub(crate) fn spawn(database_path: PathBuf) -> Self {
        let (plays, queued) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("reprise-android-plays".to_owned())
            .spawn(move || write_queued_plays(&database_path, queued));
        match worker {
            Ok(worker) => Self {
                plays: Some(plays),
                worker: Some(worker),
            },
            Err(error) => {
                // A device that cannot spawn one thread will not play music
                // either; refusing to construct the session over it would be a
                // worse answer than playing without counting.
                tracing::warn!(%error, "no Android play counting: writer thread did not start");
                Self {
                    plays: None,
                    worker: None,
                }
            }
        }
    }

    /// Queues one play. Never blocks on the database and never panics — a play
    /// that cannot be queued leaves a warning rather than disappearing.
    pub(crate) fn record(&self, play: RecordedPlay) {
        let Some(plays) = self.plays.as_ref() else {
            tracing::warn!(
                track_id = play.track_id,
                "dropped an Android play count: no writer thread",
            );
            return;
        };
        if let Err(error) = plays.send(play) {
            tracing::warn!(
                %error,
                track_id = play.track_id,
                "dropped an Android play count: the writer thread is gone",
            );
        }
    }
}

impl Drop for PlayRecorder {
    fn drop(&mut self) {
        // Dropping the sender ends the worker's loop, but only after it has
        // drained everything already queued. Joining is what turns "the service
        // is being destroyed" into a bounded wait instead of a lost play count:
        // the outstanding work is at most a handful of single-row updates.
        self.plays = None;
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("the Android play-count writer thread panicked");
            }
        }
    }
}

fn write_queued_plays(database_path: &Path, queued: Receiver<RecordedPlay>) {
    let db = match Db::open_ready(database_path) {
        Ok(db) => db,
        Err(error) => {
            // Drain rather than return, so every play that arrives afterwards
            // still says out loud that it went nowhere.
            tracing::warn!(%error, "no Android play counting: could not open the library");
            for play in queued {
                tracing::warn!(
                    track_id = play.track_id,
                    "dropped an Android play count: the library never opened",
                );
            }
            return;
        }
    };
    for play in queued {
        if let Err(error) =
            reprise_core::library::stats::record_play(&db, play.track_id, play.at_unix)
        {
            tracing::warn!(%error, track_id = play.track_id, "could not record an Android play");
        }
    }
}
