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
//!
//! ## Losing to the scanner
//!
//! `MusicLibrary::scan` wraps its entire folder walk in **one** transaction,
//! and an SAF walk over a large tree easily runs longer than the five-second
//! `busy_timeout` every `Db` connection carries — which is exactly the moment a
//! user is most likely to press play. A single attempt would then fail with
//! `SQLITE_BUSY` and the play would be gone. So a play that lost to another
//! writer is offered again a bounded number of times ([`retry_after`]) before
//! it is given up, and giving up says so by name.
//!
//! What this deliberately is **not** is a persistent queue: nothing here
//! survives the process. A play still in hand when Android kills the service is
//! lost, silently, because there is nobody left to tell.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reprise_core::db::Db;

/// How many times one play is offered to a database another writer is holding.
///
/// Each attempt already blocks inside SQLite for up to `busy_timeout`
/// (5 000 ms), so four attempts is roughly twenty seconds of patience — long
/// enough to outlast an ordinary scan transaction, short enough that a
/// permanently wedged database does not keep a thread forever.
const BUSY_ATTEMPTS: u32 = 4;

/// The pause before the second attempt; each later pause doubles it. Small on
/// purpose: the waiting that matters happens inside SQLite's own `busy_timeout`
/// and this only keeps the retries from arriving as a burst.
const FIRST_BUSY_BACKOFF: Duration = Duration::from_millis(250);

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

/// How long to wait before offering a failed write again, or `None` when the
/// play should be given up.
///
/// Pure, so the policy — retry only what another writer caused, and only a
/// bounded number of times — is decidable without a database.
fn retry_after(busy: bool, attempt: u32) -> Option<Duration> {
    if !busy || attempt >= BUSY_ATTEMPTS {
        return None;
    }
    Some(FIRST_BUSY_BACKOFF * 2u32.pow(attempt - 1))
}

/// Sends counted plays to a writer thread and waits for it on teardown.
pub(crate) struct PlayRecorder {
    /// `None` only while dropping: the sender has to go before the join, or
    /// the worker never sees the end of its channel.
    plays: Option<Sender<RecordedPlay>>,
    /// Set before the join so an in-flight retry stops waiting for a database
    /// that is about to outlive this process. Without it, teardown would
    /// inherit the whole retry budget and the service's `onDestroy` could sit
    /// for twenty seconds per queued play.
    shutting_down: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PlayRecorder {
    /// Starts the writer over `database_path`, which the caller has already
    /// opened and migrated.
    pub(crate) fn spawn(database_path: PathBuf) -> Self {
        let (plays, queued) = mpsc::channel();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&shutting_down);
        let worker = std::thread::Builder::new()
            .name("reprise-android-plays".to_owned())
            .spawn(move || write_queued_plays(&database_path, queued, &worker_flag));
        match worker {
            Ok(worker) => Self {
                plays: Some(plays),
                shutting_down,
                worker: Some(worker),
            },
            Err(error) => {
                // A device that cannot spawn one thread will not play music
                // either; refusing to construct the session over it would be a
                // worse answer than playing without counting.
                tracing::warn!(%error, "no Android play counting: writer thread did not start");
                Self {
                    plays: None,
                    shutting_down,
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
        //
        // The flag goes up first so that bound stays a bound: a drain that is
        // currently backing off from a busy database stops offering the write
        // again and settles for saying it was lost.
        self.shutting_down.store(true, Ordering::Relaxed);
        self.plays = None;
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("the Android play-count writer thread panicked");
            }
        }
    }
}

fn write_queued_plays(
    database_path: &Path,
    queued: Receiver<RecordedPlay>,
    shutting_down: &AtomicBool,
) {
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
        record_play_with_retries(&db, play, shutting_down);
    }
}

/// Writes one play, offering it again while the only thing in the way is
/// another writer. A play that is finally given up leaves a warning naming the
/// track, so the loss is a line in `logcat` rather than nothing at all.
fn record_play_with_retries(db: &Db, play: RecordedPlay, shutting_down: &AtomicBool) {
    let mut attempt = 1;
    loop {
        let error = match reprise_core::library::stats::record_play(db, play.track_id, play.at_unix)
        {
            Ok(()) => return,
            Err(error) => error,
        };
        let busy = reprise_core::library::stats::is_database_busy(&error);
        let wait = retry_after(busy, attempt).filter(|_| !shutting_down.load(Ordering::Relaxed));
        let Some(wait) = wait else {
            tracing::warn!(
                %error,
                track_id = play.track_id,
                attempts = attempt,
                "dropped an Android play count",
            );
            return;
        };
        tracing::debug!(
            track_id = play.track_id,
            attempt,
            "the library is busy; offering an Android play count again",
        );
        std::thread::sleep(wait);
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use reprise_core::db::Db;

    use super::{record_play_with_retries, retry_after, RecordedPlay, BUSY_ATTEMPTS};
    use crate::log_capture::{CapturedLogs, LogCapture};

    /// The whole point of the retry: a write that lost to the scanner's
    /// transaction is offered again, with a growing pause, up to a bound. A
    /// failure the scanner did not cause is not worth a single retry.
    #[test]
    fn only_a_write_that_lost_to_another_writer_is_offered_again_and_only_so_often() {
        let waits: Vec<_> = (1..=BUSY_ATTEMPTS)
            .map(|attempt| retry_after(true, attempt))
            .collect();

        assert_eq!(
            waits,
            vec![
                Some(std::time::Duration::from_millis(250)),
                Some(std::time::Duration::from_millis(500)),
                Some(std::time::Duration::from_millis(1_000)),
                None,
            ],
        );
        assert_eq!(
            retry_after(false, 1),
            None,
            "a busy database is the only reason to try again"
        );
    }

    /// A play that cannot be written must not vanish quietly. The failure here
    /// is a read-only handle rather than a busy one, which is also the shortest
    /// way to prove the give-up path does not retry what retrying cannot fix.
    #[test]
    fn a_play_that_cannot_be_written_names_its_track_on_the_way_out() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reprise.db");
        drop(Db::open_migrated(Some(&path)).unwrap());
        let read_only = Db::open_ready_read_only(&path).unwrap();

        let logs = CapturedLogs::default();
        tracing::subscriber::with_default(LogCapture(logs.clone()), || {
            record_play_with_retries(&read_only, RecordedPlay::now(830), &AtomicBool::new(false));
        });

        let logged = logs.joined();
        assert!(logged.contains("WARN"), "expected a warning, got {logged}");
        assert!(
            logged.contains("dropped an Android play count"),
            "expected the loss to be named, got {logged}",
        );
        assert!(
            logged.contains("track_id=830"),
            "expected the affected track to be named, got {logged}",
        );
    }
}
