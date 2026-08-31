//! Lock boundary for retrying Android play-count writes.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use reprise_core::db::Db;

use crate::play_recorder_retry::{with_busy_retries, GaveUp};

pub(crate) enum SharedWriteError<E> {
    Database(E),
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
            SharedWriteError::WriterPoisoned => false,
        },
        || {
            let database = writer
                .lock()
                .map_err(|_| SharedWriteError::WriterPoisoned)?;
            write(&database).map_err(SharedWriteError::Database)
        },
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc, Mutex};

    use reprise_core::db::Db;

    use super::with_shared_writer_retries;

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
