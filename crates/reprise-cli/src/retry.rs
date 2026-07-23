//! Bounded write-retry with jittered backoff.
//!
//! The database is opened with SQLite's own `busy_timeout` (5 s), which blocks
//! a writer while another connection holds the write lock. This layer sits on
//! top for the rarer case where a *long* foreign write (e.g. a full rescan by
//! the running app) outlasts that timeout and the facade still returns
//! `SQLITE_BUSY`/`SQLITE_LOCKED`: rather than fail the whole command, the CLI
//! retries a few times with a short, jittered backoff. Reads never need this —
//! WAL readers do not block on a writer — so only mutating facades are wrapped.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How many times a wrapped write is attempted in total (initial try plus
/// retries).
pub const MAX_WRITE_ATTEMPTS: u32 = 5;
/// Base backoff before the first retry; doubles each subsequent retry.
pub const BASE_BACKOFF_MS: u64 = 20;
/// Upper bound on a single backoff sleep, before jitter.
pub const MAX_BACKOFF_MS: u64 = 200;
/// Maximum extra jitter added to each backoff, spreading out contending
/// writers so they do not wake in lockstep.
pub const MAX_JITTER_MS: u64 = 20;

/// Runs `op`, retrying while `is_busy` reports a transient lock/busy failure,
/// up to [`MAX_WRITE_ATTEMPTS`]. Any error `is_busy` rejects is returned
/// immediately; the last busy error is returned once attempts are exhausted.
pub fn with_retry<T, E>(
    mut op: impl FnMut() -> Result<T, E>,
    is_busy: impl Fn(&E) -> bool,
) -> Result<T, E> {
    let mut attempt: u32 = 0;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(error) => {
                attempt += 1;
                if attempt >= MAX_WRITE_ATTEMPTS || !is_busy(&error) {
                    return Err(error);
                }
                std::thread::sleep(backoff(attempt));
            }
        }
    }
}

/// Backoff before the `attempt`-th retry (`attempt` is 1-based): exponential,
/// capped at [`MAX_BACKOFF_MS`], plus up to [`MAX_JITTER_MS`] of jitter.
fn backoff(attempt: u32) -> Duration {
    let exponential = BASE_BACKOFF_MS.saturating_mul(1u64 << (attempt.min(16) - 1));
    let capped = exponential.min(MAX_BACKOFF_MS);
    Duration::from_millis(capped + jitter_ms())
}

/// A cheap jitter source without pulling in an RNG crate: the sub-millisecond
/// wall-clock noise at call time, folded into `0..=MAX_JITTER_MS`.
fn jitter_ms() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()));
    nanos % (MAX_JITTER_MS + 1)
}

/// Whether a `rusqlite::Error` is a transient busy/locked failure worth
/// retrying.
pub fn rusqlite_is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::DatabaseBusy
                || inner.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// Whether a scan failure is a transient busy/locked failure worth retrying —
/// `ScanError` wraps the underlying `rusqlite`/`DbError` cause.
pub fn scan_is_busy(error: &reprise_core::library::scanner::ScanError) -> bool {
    use reprise_core::db::DbError;
    use reprise_core::library::scanner::ScanError;
    match error {
        ScanError::Sqlite(inner) => rusqlite_is_busy(inner),
        ScanError::Db(DbError::Sqlite(inner)) => rusqlite_is_busy(inner),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, PartialEq, Eq)]
    enum FakeError {
        Busy,
        Fatal,
    }

    fn fake_is_busy(error: &FakeError) -> bool {
        matches!(error, FakeError::Busy)
    }

    #[test]
    fn succeeds_without_retry_when_op_is_ok() {
        let calls = Cell::new(0);
        let result: Result<i32, FakeError> = with_retry(
            || {
                calls.set(calls.get() + 1);
                Ok(7)
            },
            fake_is_busy,
        );
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.get(), 1, "a successful op runs exactly once");
    }

    #[test]
    fn retries_busy_then_succeeds() {
        let calls = Cell::new(0);
        let result: Result<&str, FakeError> = with_retry(
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Err(FakeError::Busy)
                } else {
                    Ok("done")
                }
            },
            fake_is_busy,
        );
        assert_eq!(result.unwrap(), "done");
        assert_eq!(
            calls.get(),
            3,
            "two busy failures, then success on the third"
        );
    }

    #[test]
    fn gives_up_after_max_attempts_when_always_busy() {
        let calls = Cell::new(0);
        let result: Result<(), FakeError> = with_retry(
            || {
                calls.set(calls.get() + 1);
                Err(FakeError::Busy)
            },
            fake_is_busy,
        );
        assert_eq!(result.unwrap_err(), FakeError::Busy);
        assert_eq!(
            calls.get(),
            MAX_WRITE_ATTEMPTS as i32,
            "a permanently busy op is tried exactly MAX_WRITE_ATTEMPTS times"
        );
    }

    #[test]
    fn does_not_retry_a_non_busy_error() {
        let calls = Cell::new(0);
        let result: Result<(), FakeError> = with_retry(
            || {
                calls.set(calls.get() + 1);
                Err(FakeError::Fatal)
            },
            fake_is_busy,
        );
        assert_eq!(result.unwrap_err(), FakeError::Fatal);
        assert_eq!(calls.get(), 1, "a fatal error is returned on the first try");
    }

    #[test]
    fn rusqlite_busy_and_locked_are_retryable() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        let locked = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
            None,
        );
        assert!(rusqlite_is_busy(&busy));
        assert!(rusqlite_is_busy(&locked));
    }

    #[test]
    fn rusqlite_other_failures_are_not_retryable() {
        let readonly = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_READONLY),
            None,
        );
        assert!(!rusqlite_is_busy(&readonly));
        assert!(!rusqlite_is_busy(&rusqlite::Error::QueryReturnedNoRows));
    }
}
