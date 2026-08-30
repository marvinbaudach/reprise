use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::Path;

use rusqlite::Connection;

const LOCK_FILE_NAME: &str = "tag-write.lock";

/// The outcome of trying to establish the tag writer's liveness signal.
#[derive(Debug)]
pub enum TagWriteLockAttempt {
    /// Acquired, and the filesystem enforces it. Only this proves no other writer.
    Held(TagWriteLock),
    /// Another live process holds the lock.
    Busy,
    /// The filesystem does not enforce advisory locks.
    ///
    /// Writes may proceed because the database row remains the exclusion
    /// mechanism. Recovery must not treat this as proof that no writer exists.
    Unenforceable,
}

/// What an independent lock-file probe can prove about a tag writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagWriteLiveness {
    Live,
    Absent,
    /// Advisory locking could not be enforced, so a live writer is possible.
    Unknown,
}

/// Keeps the tag writer's advisory lock held for the lifetime of one job.
#[derive(Debug)]
pub struct TagWriteLock {
    file: File,
}

impl TagWriteLock {
    /// Attempts to hold `<db_dir>/tag-write.lock` without waiting.
    pub fn acquire(db_dir: &Path) -> io::Result<TagWriteLockAttempt> {
        let file = open_lock_file(db_dir)?;
        let attempt = file.try_lock();
        attempt_after_try_lock(file, attempt)
    }

    /// Probes liveness through a fresh open-file description.
    ///
    /// Open and lock errors prove nothing, so they conservatively map to
    /// `Unknown`. A successful probe releases its temporary lock immediately.
    #[must_use]
    pub fn probe(db_dir: &Path) -> TagWriteLiveness {
        let file = match open_lock_file(db_dir) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "tag-writer liveness is unknown: its lock file could not be opened",
                );
                return TagWriteLiveness::Unknown;
            }
        };

        match file.try_lock() {
            Ok(()) => {
                if let Err(error) = file.unlock() {
                    tracing::warn!(
                        %error,
                        "tag-writer liveness is unknown: its probe lock could not be released",
                    );
                    TagWriteLiveness::Unknown
                } else {
                    TagWriteLiveness::Absent
                }
            }
            Err(TryLockError::WouldBlock) => TagWriteLiveness::Live,
            Err(TryLockError::Error(error)) => {
                tracing::warn!(
                    %error,
                    "tag-writer liveness is unknown: this filesystem does not enforce advisory locks",
                );
                TagWriteLiveness::Unknown
            }
        }
    }
}

impl Drop for TagWriteLock {
    fn drop(&mut self) {
        if let Err(error) = self.file.unlock() {
            tracing::warn!(%error, "the tag-writer lock could not be released");
        }
    }
}

fn open_lock_file(db_dir: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(db_dir.join(LOCK_FILE_NAME))
}

pub(super) fn attempt_after_try_lock(
    mut file: File,
    attempt: Result<(), TryLockError>,
) -> io::Result<TagWriteLockAttempt> {
    match attempt {
        Ok(()) => {
            file.set_len(0)?;
            writeln!(file, "pid={}", std::process::id())?;
            file.flush()?;
            Ok(TagWriteLockAttempt::Held(TagWriteLock { file }))
        }
        Err(TryLockError::WouldBlock) => Ok(TagWriteLockAttempt::Busy),
        Err(TryLockError::Error(error)) => {
            tracing::warn!(
                %error,
                "tag writing is running unlocked: this filesystem does not enforce advisory locks",
            );
            Ok(TagWriteLockAttempt::Unenforceable)
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("another tag-writing job is already running")]
pub struct TagWriteBusy;

/// Fails when any tag-write job of any kind is prepared or running.
/// Call this inside the same transaction that inserts the new job row.
///
/// A query that cannot be answered is reported as the database error it is,
/// never as `TagWriteBusy`: "wait, someone else is writing" invites a retry,
/// and a caller that retries against a broken database never stops.
pub(crate) fn claim_tag_write_slot<E>(conn: &Connection) -> Result<(), E>
where
    E: From<rusqlite::Error> + From<TagWriteBusy>,
{
    let occupied = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tag_write_jobs \
         WHERE state IN ('prepared', 'running') LIMIT 1)",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if occupied {
        Err(TagWriteBusy.into())
    } else {
        Ok(())
    }
}
