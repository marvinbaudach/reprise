//! Lock boundary for retrying Android play-count writes.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use reprise_core::db::Db;

use crate::play_recorder::RecordedPlay;
use crate::play_recorder_retry::{with_busy_retries, GaveUp};
use crate::writer_backoff::try_lock_writer;

pub(crate) enum SharedWriteError<E> {
    Database(E),
    WriterBusy,
    WriterPoisoned,
}

pub(crate) fn with_shared_writer_retries<E>(
    writer: &Mutex<Db>,
    shutting_down: &AtomicBool,
    track_id: i64,
    is_busy: impl Fn(&E) -> bool,
    mut write: impl FnMut(&Db) -> Result<(), E>,
) -> Result<(), GaveUp<SharedWriteError<E>>> {
    with_busy_retries(
        shutting_down,
        track_id,
        |error| match error {
            SharedWriteError::Database(error) => is_busy(error),
            SharedWriteError::WriterBusy => true,
            SharedWriteError::WriterPoisoned => false,
        },
        || {
            let database = try_lock_writer(writer)
                .map_err(|_| SharedWriteError::WriterPoisoned)?
                .ok_or(SharedWriteError::WriterBusy)?;
            write(&database).map_err(SharedWriteError::Database)
        },
    )
}

/// Counts one play directly when no durable journal could be opened.
///
/// This degraded path accepts bounded loss rather than letting Android service
/// shutdown wait indefinitely behind the coordinated writer.
pub(crate) fn record_unjournaled_play(
    writer: &Mutex<Db>,
    play: RecordedPlay,
    shutting_down: &AtomicBool,
) {
    let written = with_shared_writer_retries(
        writer,
        shutting_down,
        play.track_id,
        reprise_core::library::stats::is_database_busy,
        |database| reprise_core::library::stats::record_play(database, play.track_id, play.at_unix),
    );
    match written {
        Ok(()) => tracing::debug!(
            track_id = play.track_id,
            "counted an Android play without a journal: it would not survive a kill",
        ),
        Err(GaveUp {
            attempts,
            error: SharedWriteError::Database(error),
        }) => tracing::warn!(
            %error,
            track_id = play.track_id,
            attempts,
            "dropped an Android play count: no journal was open and the write failed",
        ),
        Err(GaveUp {
            attempts,
            error: SharedWriteError::WriterBusy,
        }) => tracing::warn!(
            track_id = play.track_id,
            attempts,
            "dropped an Android play count: no journal was open and the library writer stayed busy",
        ),
        Err(GaveUp {
            attempts,
            error: SharedWriteError::WriterPoisoned,
        }) => tracing::warn!(
            track_id = play.track_id,
            attempts,
            "dropped an Android play count: the shared writer was poisoned",
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    use reprise_core::db::Db;

    use super::{with_shared_writer_retries, SharedWriteError};
    use crate::play_recorder_retry::GaveUp;

    #[test]
    fn held_shared_writer_is_reported_as_busy_without_blocking() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let writer = Arc::new(Mutex::new(Db::open_migrated(Some(&database_path)).unwrap()));
        let held = writer.lock().unwrap();
        let worker_writer = Arc::clone(&writer);
        let (returned, wait_for_return) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = with_shared_writer_retries(
                worker_writer.as_ref(),
                &AtomicBool::new(true),
                830,
                |_| false,
                |_| Ok::<_, ()>(()),
            );
            returned.send(result).unwrap();
        });

        let result = wait_for_return.recv_timeout(Duration::from_millis(500));
        drop(held);
        worker.join().unwrap();

        assert!(matches!(
            result,
            Ok(Err(GaveUp {
                attempts: 1,
                error: SharedWriteError::WriterBusy,
            })),
        ));
    }

    #[test]
    fn shared_writer_is_released_before_a_busy_retry_waits() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let writer = Arc::new(Mutex::new(Db::open_migrated(Some(&database_path)).unwrap()));
        let probe_writer = Arc::clone(&writer);
        let (retrying, observe_retry) = mpsc::channel();
        let probe = std::thread::spawn(move || {
            observe_retry.recv().unwrap();
            probe_writer.try_lock().is_ok()
        });
        let mut attempts = 0;

        let result = with_shared_writer_retries(
            writer.as_ref(),
            &AtomicBool::new(false),
            830,
            |_| {
                retrying.send(()).unwrap();
                true
            },
            |_| {
                attempts += 1;
                if attempts == 1 {
                    Err("busy")
                } else {
                    Ok(())
                }
            },
        );

        assert!(result.is_ok());
        assert_eq!(attempts, 2);
        assert!(
            probe.join().unwrap(),
            "the app-wide writer stayed locked during SQLite busy backoff",
        );
    }
}
