//! Bounded retry policy for Android play-count writes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How many times one play is offered to a database another writer is holding.
pub(crate) const BUSY_ATTEMPTS: u32 = 4;

const FIRST_BUSY_BACKOFF: Duration = Duration::from_millis(250);

pub(crate) fn retry_after(busy: bool, attempt: u32) -> Option<Duration> {
    if !busy || attempt >= BUSY_ATTEMPTS {
        return None;
    }
    Some(FIRST_BUSY_BACKOFF * 2u32.pow(attempt - 1))
}

pub(crate) struct GaveUp<E> {
    pub(crate) attempts: u32,
    pub(crate) error: E,
}

/// Offers `write` again while the only thing in the way is another writer.
pub(crate) fn with_busy_retries<E>(
    shutting_down: &AtomicBool,
    track_id: i64,
    is_busy: impl Fn(&E) -> bool,
    mut write: impl FnMut() -> Result<(), E>,
) -> Result<(), GaveUp<E>> {
    let mut attempt = 1;
    loop {
        let error = match write() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        let wait = retry_after(is_busy(&error), attempt);
        let Some(wait) = wait.filter(|_| !shutting_down.load(Ordering::Relaxed)) else {
            return Err(GaveUp {
                attempts: attempt,
                error,
            });
        };
        tracing::debug!(
            track_id,
            attempt,
            "the library is busy; offering an Android play count again",
        );
        std::thread::park_timeout(wait);
        // Teardown can free a short-lived competing writer before it wakes us.
        // Preserve one immediate probe, but never inherit a longer backoff.
        if attempt > 1 && shutting_down.load(Ordering::Relaxed) {
            return Err(GaveUp {
                attempts: attempt,
                error,
            });
        }
        attempt += 1;
    }
}
