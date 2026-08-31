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
//! So the session owns one writer thread that uses `MusicLibrary`'s coordinated
//! writer handle, and the playback thread only ever hands over an id and the
//! moment it happened. A scan and play counting can no longer bypass one
//! another through independent SQLite connections.
//!
//! ## Losing to the scanner
//!
//! `MusicLibrary::scan` wraps its entire folder walk in **one** transaction,
//! and an SAF walk over a large tree easily runs longer than the five-second
//! `busy_timeout` every `Db` connection carries — which is exactly the moment a
//! user is most likely to press play. A single attempt would then fail with
//! `SQLITE_BUSY` and the play would be gone. So a play that lost to another
//! writer is offered again a bounded number of times ([`retry_after`]). When
//! that round gives up, it says so by track and sequence and leaves the play in
//! the journal for a later drain.
//!
//! ## Surviving the process
//!
//! The writer appends every play to a bounded file in Android's private app
//! directory before it asks SQLite to count it, and follows the append through
//! to the platter — so the promise reaches past process death to a flat battery.
//! Each entry has a monotonic sequence; Core increments the count and advances
//! one applied high-water mark in the same transaction. The journal entry is
//! removed only after that commit, so a kill on either side of the commit is
//! safe: an unapplied entry is replayed, while an already-applied one is
//! recognized without being counted twice.
//!
//! What the journal does with a line it did not expect is [its own
//! decision](crate::play_journal) — damage is discarded, an unknown format
//! version is refused rather than rewritten away, and a colliding sequence is
//! renumbered, because it describes a play that happened.
//!
//! ## When there is no journal to survive in
//!
//! A journal that will not open — an unenforceable lock's `Unsupported`, a
//! format from a newer build, a directory that has gone read-only — takes the
//! promise away, and nothing else. The writer then counts plays the way it did
//! before the journal existed: straight to the library, losing only what a kill
//! catches between the play and SQLite's commit.
//!
//! It is worth being blunt about why, because the first version of this got it
//! backwards and the whole feature went dark on a device. Holding a play back
//! because its *durability* mechanism is broken loses that play with certainty;
//! writing it without the mechanism loses it only if the process dies in the
//! next few milliseconds. A durability mechanism that can make the outcome
//! worse than its own absence is not one, so its failure is a downgrade and
//! says so in the log — never a shutdown.
//!
//! One boundary is left standing on purpose: an entry whose *write* keeps
//! failing for a reason retrying cannot fix does hold up the ones behind it.
//! Applying a later entry would move the applied mark past this one and make
//! the play it describes unrecognisable as pending, so stopping is the only way
//! to keep it replayable. A user would see a play count that stops rising while
//! the log carries `kept an Android play count in its journal` for the same
//! track over and over.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use reprise_core::db::Db;

use crate::play_journal::{JournalEntry, PlayJournal};
use crate::play_recorder_retry::{with_busy_retries, GaveUp};
use crate::play_recorder_writer::{with_shared_writer_retries, SharedWriteError};

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
    /// Set before the join so an in-flight retry stops waiting for a database
    /// that is about to outlive this process. Without it, teardown would
    /// inherit the whole retry budget and the service's `onDestroy` could sit
    /// for twenty seconds per queued play.
    shutting_down: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PlayRecorder {
    /// Starts the writer over the library's one coordinated writer handle.
    pub(crate) fn spawn(
        database_path: PathBuf,
        writer: Arc<Mutex<Db>>,
        applied_sequence: i64,
    ) -> Self {
        let (plays, queued) = mpsc::channel();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&shutting_down);
        let worker = std::thread::Builder::new()
            .name("reprise-android-plays".to_owned())
            .spawn(move || {
                write_queued_plays(
                    &database_path,
                    &writer,
                    applied_sequence,
                    queued,
                    &worker_flag,
                );
            });
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
    /// that cannot be queued leaves a warning rather than disappearing. The
    /// channel is deliberately the only state this object has access to; the
    /// private-file journal is opened and appended only by the writer thread.
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
        // journaled everything already queued. Joining is what turns "the
        // service is being destroyed" into a bounded handoff instead of losing
        // an event that had reached this process but not its private file.
        //
        // The flag goes up first so a drain currently backing off from a busy
        // database stops retrying. Its entry is already durable and remains for
        // the next session rather than extending `onDestroy` by the full retry
        // budget.
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
    writer: &Mutex<Db>,
    applied_sequence: i64,
    queued: Receiver<RecordedPlay>,
    shutting_down: &AtomicBool,
) {
    let journal = match PlayJournal::open(database_path, applied_sequence) {
        Ok(journal) => Some(journal),
        Err(error) => {
            tracing::warn!(
                %error,
                "Android plays will be counted without a journal: it could not be opened",
            );
            None
        }
    };
    let Some(mut journal) = journal else {
        for play in queued {
            let Ok(database) = writer.lock() else {
                tracing::warn!(
                    track_id = play.track_id,
                    "dropped an Android play count: the shared writer was poisoned",
                );
                continue;
            };
            record_unjournaled_play(&database, play, shutting_down);
        }
        return;
    };
    drain_shared_journal(writer, &mut journal, shutting_down);
    for play in queued {
        if append_or_warn(&mut journal, play) {
            drain_shared_journal(writer, &mut journal, shutting_down);
        }
    }
}

fn drain_shared_journal(writer: &Mutex<Db>, journal: &mut PlayJournal, shutting_down: &AtomicBool) {
    drain_journal(writer, journal, shutting_down);
}

fn append_or_warn(journal: &mut PlayJournal, play: RecordedPlay) -> bool {
    match journal.append(play) {
        Ok(accepted) => accepted,
        Err(error) => {
            tracing::warn!(
                %error,
                track_id = play.track_id,
                "dropped an Android play count: it could not be journaled",
            );
            false
        }
    }
}

fn drain_journal(writer: &Mutex<Db>, journal: &mut PlayJournal, shutting_down: &AtomicBool) {
    while let Some(entry) = journal.front() {
        if !record_play_with_retries(writer, entry, shutting_down) {
            return;
        }
        if let Err(error) = journal.remove_front() {
            // Not a reason to stop. The entry is counted and its sequence is at
            // the applied mark, so the line it leaves behind can only be
            // recognised, never counted again — whereas stopping would park
            // every later play behind it for as long as the file stays
            // unwritable.
            tracing::warn!(
                %error,
                track_id = entry.play.track_id,
                sequence = entry.sequence,
                "an Android play committed but could not be removed from its journal",
            );
        }
    }
}

/// Writes one journal entry, offering it again while the only thing in the way
/// is another writer. A retry round that gives up leaves a warning naming the
/// track and sequence; the caller keeps the entry durable for a later drain.
fn record_play_with_retries(
    writer: &Mutex<Db>,
    entry: JournalEntry,
    shutting_down: &AtomicBool,
) -> bool {
    let written = with_shared_writer_retries(
        writer,
        shutting_down,
        entry.play.track_id,
        reprise_core::library::stats::is_database_busy,
        |database| {
            reprise_core::library::stats::record_journaled_play(
                database,
                entry.sequence,
                entry.play.track_id,
                entry.play.at_unix,
            )
            .map(|_| ())
        },
    );
    match written {
        Ok(()) => true,
        Err(GaveUp {
            attempts,
            error: SharedWriteError::Database(error),
        }) => {
            tracing::warn!(
                %error,
                track_id = entry.play.track_id,
                sequence = entry.sequence,
                attempts,
                "kept an Android play count in its journal after a write failure",
            );
            false
        }
        Err(GaveUp {
            attempts,
            error: SharedWriteError::WriterPoisoned,
        }) => {
            tracing::warn!(
                track_id = entry.play.track_id,
                sequence = entry.sequence,
                attempts,
                "kept an Android play count in its journal: the shared writer was poisoned",
            );
            false
        }
    }
}

/// Counts one play with no journal behind it, because there is no journal to
/// have.
///
/// This is what the writer did before M8, and it is deliberately what it falls
/// back to: a play written straight to the library is lost only if the process
/// dies in the seconds before SQLite commits, while a play held back because
/// the durability mechanism is broken is lost every single time. The weaker
/// promise is named in the log so nobody reads a rising count as the strong
/// one.
fn record_unjournaled_play(db: &Db, play: RecordedPlay, shutting_down: &AtomicBool) {
    let written = with_busy_retries(
        shutting_down,
        play.track_id,
        reprise_core::library::stats::is_database_busy,
        || reprise_core::library::stats::record_play(db, play.track_id, play.at_unix),
    );
    match written {
        Ok(()) => tracing::debug!(
            track_id = play.track_id,
            "counted an Android play without a journal: it would not survive a kill",
        ),
        Err(GaveUp { attempts, error }) => tracing::warn!(
            %error,
            track_id = play.track_id,
            attempts,
            "dropped an Android play count: no journal was open to keep it",
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};

    use reprise_core::db::Db;

    use super::{record_play_with_retries, RecordedPlay};
    use crate::log_capture::CapturedLogs;
    use crate::play_journal::{
        JournalEntry, PlayJournal, FILE_NAME as JOURNAL_FILE_NAME,
        TEMP_FILE_NAME as JOURNAL_TEMP_FILE_NAME,
    };
    use crate::play_recorder_retry::{retry_after, BUSY_ATTEMPTS};

    fn seeded_tracks(directory: &Path, count: i64) -> (PathBuf, Vec<i64>) {
        let music = directory.join("music");
        std::fs::create_dir(&music).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        for index in 0..count {
            std::fs::copy(&source, music.join(format!("sine-{index}.flac"))).unwrap();
        }
        let database_path = directory.join("reprise.db");
        let database = Db::open_migrated(Some(&database_path)).unwrap();
        reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
        let tracks: Vec<_> = reprise_core::queries::query_library_text_search(
            &database,
            "",
            reprise_core::queries::WindowRange {
                offset: 0,
                limit: count,
            },
        )
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.id)
        .collect();
        assert_eq!(
            tracks.len(),
            count as usize,
            "the fixture must seed {count} tracks"
        );
        (database_path, tracks)
    }

    fn seeded_database(directory: &Path) -> (PathBuf, i64) {
        let (database_path, tracks) = seeded_tracks(directory, 1);
        (database_path, tracks[0])
    }

    fn play_count(database_path: &Path, track_id: i64) -> i64 {
        let database = Db::open_ready(database_path).unwrap();
        reprise_core::queries::query_present_track_by_id(&database, track_id)
            .unwrap()
            .unwrap()
            .play_count
    }

    fn shared_writer(database_path: &Path) -> Arc<Mutex<Db>> {
        Arc::new(Mutex::new(Db::open_ready(database_path).unwrap()))
    }

    #[test]
    fn an_unapplied_journal_entry_is_counted_on_the_next_open() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, track_id) = seeded_database(directory.path());
        std::fs::write(
            directory.path().join(JOURNAL_FILE_NAME),
            format!("v1\t1\t{track_id}\t1700000000\n"),
        )
        .unwrap();

        let recorder =
            super::PlayRecorder::spawn(database_path.clone(), shared_writer(&database_path), 0);
        drop(recorder);

        assert_eq!(play_count(&database_path, track_id), 1);
        assert_eq!(
            std::fs::read(directory.path().join(JOURNAL_FILE_NAME)).unwrap(),
            b"",
            "the committed entry must be removed from the journal",
        );
    }

    #[test]
    fn an_applied_entry_left_in_the_journal_is_not_counted_twice() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, track_id) = seeded_database(directory.path());
        let database = Db::open_ready(&database_path).unwrap();
        assert!(reprise_core::library::stats::record_journaled_play(
            &database,
            1,
            track_id,
            1_700_000_000,
        )
        .unwrap());
        drop(database);
        std::fs::write(
            directory.path().join(JOURNAL_FILE_NAME),
            format!("v1\t1\t{track_id}\t1700000000\n"),
        )
        .unwrap();

        let recorder =
            super::PlayRecorder::spawn(database_path.clone(), shared_writer(&database_path), 1);
        drop(recorder);

        assert_eq!(play_count(&database_path, track_id), 1);
        assert_eq!(
            std::fs::read(directory.path().join(JOURNAL_FILE_NAME)).unwrap(),
            b"",
        );
    }

    /// Two writers that overlapped read the same applied mark and numbered
    /// their plays alike. Both lines describe a play that happened, so the next
    /// session counts both: the colliding entry is data, not damage.
    #[test]
    fn both_plays_journaled_under_one_sequence_are_counted() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, tracks) = seeded_tracks(directory.path(), 2);
        std::fs::write(
            directory.path().join(JOURNAL_FILE_NAME),
            format!(
                "v1\t1\t{}\t1700000000\nv1\t1\t{}\t1700000001\n",
                tracks[0], tracks[1],
            ),
        )
        .unwrap();

        let recorder =
            super::PlayRecorder::spawn(database_path.clone(), shared_writer(&database_path), 0);
        drop(recorder);

        assert_eq!(play_count(&database_path, tracks[0]), 1);
        assert_eq!(
            play_count(&database_path, tracks[1]),
            1,
            "the colliding entry described a play that happened",
        );
        assert_eq!(
            std::fs::read(directory.path().join(JOURNAL_FILE_NAME)).unwrap(),
            b"",
            "both entries must leave the journal once they are counted",
        );
    }

    /// A journal whose file cannot be rewritten still has to let later plays
    /// through: the head is already counted, and leaving it in the file cannot
    /// double-count it — its sequence is at the applied mark.
    #[test]
    fn a_play_behind_one_that_cannot_leave_the_journal_is_still_counted() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, tracks) = seeded_tracks(directory.path(), 2);
        std::fs::write(
            directory.path().join(JOURNAL_FILE_NAME),
            format!(
                "v1\t1\t{}\t1700000000\nv1\t2\t{}\t1700000001\n",
                tracks[0], tracks[1],
            ),
        )
        .unwrap();
        let db = Db::open_ready(&database_path).unwrap();
        let mut journal = PlayJournal::open(&database_path, 0).unwrap();
        std::fs::create_dir(directory.path().join(JOURNAL_TEMP_FILE_NAME)).unwrap();

        let logs = CapturedLogs::default();
        logs.capture(|| {
            super::drain_journal(&Mutex::new(db), &mut journal, &AtomicBool::new(false));
        });

        assert_eq!(play_count(&database_path, tracks[0]), 1);
        assert_eq!(
            play_count(&database_path, tracks[1]),
            1,
            "a play must not be parked behind one that is already counted",
        );
        let logged = logs.joined();
        assert!(
            logged.contains("could not be removed from its journal"),
            "the unremovable entry must still be named, got {logged}",
        );
    }

    /// The failure mode the M8 device pass found, from the outside: the
    /// journal does not open, and every play is thrown away with it. It must
    /// downgrade instead — count the play now, without the promise that it
    /// outlives a kill.
    ///
    /// The refusal here is a format version from a newer build, which is the
    /// only way this suite can make `open` fail with the very `ErrorKind` a
    /// device's unenforceable lock produced (`Unsupported`). What made the
    /// journal unopenable is deliberately not what this test is about: the
    /// writer must not care, because on a device it was a reason no test had
    /// on its list.
    #[test]
    fn an_unopenable_journal_still_counts_plays() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, track_id) = seeded_database(directory.path());
        let foreign = format!("v2\t1\t{track_id}\t1700000000\n");
        std::fs::write(directory.path().join(JOURNAL_FILE_NAME), &foreign).unwrap();

        let (plays, queued) = mpsc::channel();
        plays
            .send(RecordedPlay {
                track_id,
                at_unix: 1_700_000_000,
            })
            .unwrap();
        drop(plays);

        let logs = CapturedLogs::default();
        logs.capture(|| {
            let writer = shared_writer(&database_path);
            super::write_queued_plays(
                &database_path,
                writer.as_ref(),
                0,
                queued,
                &AtomicBool::new(false),
            );
        });

        assert_eq!(
            play_count(&database_path, track_id),
            1,
            "a broken durability mechanism must not stop the counting it was insuring",
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join(JOURNAL_FILE_NAME)).unwrap(),
            foreign,
            "the journal this build cannot read must still survive it untouched",
        );
        let logged = logs.joined();
        assert!(
            logged.contains("counted without a journal"),
            "the weaker promise must be on the record, got {logged}",
        );
        assert!(
            logged.contains("will be counted without a journal"),
            "the downgrade must name itself where a device log can be read for it, got {logged}",
        );
    }

    /// A lock that works is not allowed to cost more than a lock that does not.
    /// The refused writer loses the promise that its plays outlive a kill; it
    /// does not lose the plays.
    #[test]
    fn a_writer_refused_the_journal_still_counts_its_plays() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, track_id) = seeded_database(directory.path());
        let held = PlayJournal::open(&database_path, 0)
            .expect("the fixture needs the first writer to hold the lock");
        let (plays, queued) = mpsc::channel();
        plays
            .send(RecordedPlay {
                track_id,
                at_unix: 1_700_000_000,
            })
            .unwrap();
        drop(plays);

        let writer = shared_writer(&database_path);
        super::write_queued_plays(
            &database_path,
            writer.as_ref(),
            0,
            queued,
            &AtomicBool::new(false),
        );

        assert_eq!(
            play_count(&database_path, track_id),
            1,
            "the writer the lock turned away must still count what it was given",
        );
        drop(held);
    }

    #[test]
    fn corrupt_lines_and_a_truncated_tail_do_not_block_valid_replay() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, track_id) = seeded_database(directory.path());
        std::fs::write(
            directory.path().join(JOURNAL_FILE_NAME),
            format!("v1\t1\t{track_id}\t1700000000\nnot-an-entry\nv1\t2\t{track_id}"),
        )
        .unwrap();

        let recorder =
            super::PlayRecorder::spawn(database_path.clone(), shared_writer(&database_path), 0);
        drop(recorder);

        assert_eq!(play_count(&database_path, track_id), 1);
        assert_eq!(
            std::fs::read(directory.path().join(JOURNAL_FILE_NAME)).unwrap(),
            b"",
            "repair must discard malformed records and the incomplete last line",
        );
    }

    #[test]
    fn recording_returns_while_the_writer_is_held_before_the_journal_append() {
        let directory = tempfile::tempdir().unwrap();
        let (database_path, track_id) = seeded_database(directory.path());
        let (plays, queued) = mpsc::channel();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&shutting_down);
        let (entered, wait_until_entered) = mpsc::channel();
        let (release, wait_for_release) = mpsc::channel();
        let worker_path = database_path.clone();
        let worker_writer = shared_writer(&database_path);
        let worker = std::thread::Builder::new()
            .name("reprise-android-plays-test".to_owned())
            .spawn(move || {
                entered.send(()).unwrap();
                wait_for_release.recv().unwrap();
                super::write_queued_plays(
                    &worker_path,
                    worker_writer.as_ref(),
                    0,
                    queued,
                    &worker_flag,
                );
            })
            .unwrap();
        let recorder = super::PlayRecorder {
            plays: Some(plays),
            shutting_down,
            worker: Some(worker),
        };
        wait_until_entered.recv().unwrap();

        let started = Instant::now();
        recorder.record(RecordedPlay {
            track_id,
            at_unix: 1_700_000_000,
        });
        let elapsed = started.elapsed();
        let ordinary_operation = !recorder.shutting_down.load(Ordering::Relaxed);
        release.send(()).unwrap();
        drop(recorder);

        assert!(
            elapsed < Duration::from_millis(100),
            "the playback thread waited for writer-thread work",
        );
        assert!(
            ordinary_operation,
            "the fixture must still describe ordinary operation",
        );
        assert_eq!(play_count(&database_path, track_id), 1);
    }

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
        let read_only = Mutex::new(Db::open_ready_read_only(&path).unwrap());

        let logs = CapturedLogs::default();
        logs.capture(|| {
            record_play_with_retries(
                &read_only,
                JournalEntry {
                    sequence: 1,
                    play: RecordedPlay::now(830),
                },
                &AtomicBool::new(false),
            );
        });

        let logged = logs.joined();
        assert!(logged.contains("WARN"), "expected a warning, got {logged}");
        assert!(
            logged.contains("kept an Android play count in its journal"),
            "expected the loss to be named, got {logged}",
        );
        assert!(
            logged.contains("track_id=830"),
            "expected the affected track to be named, got {logged}",
        );
    }
}
