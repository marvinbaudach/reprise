//! The in-process refresh backoff state (`NET-3d`).
//!
//! Split out of `pipeline.rs` so both stay under the 800-line limit the
//! architecture gate enforces.
//!
//! KNOWN HAZARD, deliberately recorded rather than silently carried: the key
//! includes the *address* of the `Connection`, not an identity of it. A
//! connection that is dropped and a new one allocated at the same address
//! inherit each other's backoff. In the app this is harmless — one long-lived
//! connection per process — but in tests, where databases are created and
//! dropped constantly, it can make a fresh database believe it is already in
//! backoff. Fixing it properly means giving `Db` a process-unique id and
//! threading it here, which is a change to the database boundary rather than
//! to this file.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Mutex, MutexGuard, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct RetryKey {
    pub(super) connection: usize,
    pub(super) subscription_id: i64,
}

static REFRESH_RETRIES: OnceLock<Mutex<HashMap<RetryKey, crate::podcasts::refresh::RefreshRetry>>> =
    OnceLock::new();

pub(super) fn retry_states(
) -> MutexGuard<'static, HashMap<RetryKey, crate::podcasts::refresh::RefreshRetry>> {
    REFRESH_RETRIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn pending_retry(key: RetryKey) -> Option<crate::podcasts::refresh::RefreshRetry> {
    retry_states().get(&key).copied()
}

pub(super) fn previous_attempt(key: RetryKey) -> u32 {
    pending_retry(key).map_or(0, crate::podcasts::refresh::RefreshRetry::attempt)
}

pub(super) fn set_retry(key: RetryKey, retry: Option<crate::podcasts::refresh::RefreshRetry>) {
    let mut states = retry_states();
    if let Some(retry) = retry {
        states.insert(key, retry);
    } else {
        states.remove(&key);
    }
}

pub(super) fn clear_retry(key: RetryKey) {
    set_retry(key, None);
}

fn classification_key(db: &crate::db::Db, episode_id: i64) -> RetryKey {
    let connection = db.path().map_or_else(
        || std::ptr::from_ref(db).addr(),
        |path| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            path.hash(&mut hasher);
            hasher.finish() as usize
        },
    );
    RetryKey {
        connection,
        // SQLite episode ids are positive. Their negative namespace cannot
        // collide with the subscription ids held in the same retry store.
        subscription_id: episode_id.saturating_neg(),
    }
}

impl crate::podcasts::EpisodeClassification {
    /// Reserves the next classification attempt under the shared source retry
    /// policy. The frontend calls this before it creates the worker, so a
    /// replay during an outstanding or backed-off attempt creates no thread,
    /// database handle, or yt-dlp process.
    pub fn claim_retry(db: &crate::db::Db, episode_id: i64, now: i64) -> bool {
        if !Self::retry_due(db, episode_id, now) {
            return false;
        }
        Self::defer_retry(db, episode_id, now);
        true
    }

    pub(crate) fn retry_due(db: &crate::db::Db, episode_id: i64, now: i64) -> bool {
        pending_retry(classification_key(db, episode_id)).is_none_or(|retry| retry.is_due(now))
    }

    pub(crate) fn defer_retry(db: &crate::db::Db, episode_id: i64, now: i64) {
        let key = classification_key(db, episode_id);
        // An empty answer has no provider error of its own, but it is still a
        // failed classification attempt. Keep it on the same bounded
        // exponential schedule as transient source failures. Once the shared
        // schedule reaches its cap, repeat its longest delay instead of
        // falling back to every play.
        let attempt = previous_attempt(key).min(crate::source_error::MAX_BACKOFF_ATTEMPTS - 1);
        let retry = crate::podcasts::refresh::next_retry(
            &crate::podcasts::PodcastError::Transport(
                "classification learned no category".to_owned(),
            ),
            attempt,
            now,
        );
        set_retry(key, retry);
    }

    /// Re-anchors a reservation's deadline at the moment the attempt it
    /// covers actually failed, without booking a second attempt.
    ///
    /// The frontend's `claim_retry` bumps and books the attempt optimistically
    /// — before the worker has even connected — so its deadline is measured
    /// from claim time, not from failure time. A slow extraction can outlive
    /// that short first delay and leave the reservation already expired by
    /// the time the worker actually fails. This keeps the same attempt number
    /// the claim reserved, but restarts its delay from `now`, so the backoff
    /// still covers the failure that just happened. A call with no prior
    /// claim (a caller that skipped `claim_retry`) still gets a first
    /// reservation instead of silently retrying forever.
    pub(crate) fn refresh_retry_deadline(db: &crate::db::Db, episode_id: i64, now: i64) {
        let key = classification_key(db, episode_id);
        let Some(reserved) = pending_retry(key) else {
            Self::defer_retry(db, episode_id, now);
            return;
        };
        let retry = crate::podcasts::refresh::next_retry(
            &crate::podcasts::PodcastError::Transport(
                "classification learned no category".to_owned(),
            ),
            reserved.attempt().saturating_sub(1),
            now,
        );
        set_retry(key, retry);
    }

    pub(crate) fn clear_retry(db: &crate::db::Db, episode_id: i64) {
        clear_retry(classification_key(db, episode_id));
    }

    #[cfg(test)]
    pub(crate) fn pending_retry_for_test(
        db: &crate::db::Db,
        episode_id: i64,
    ) -> Option<crate::podcasts::refresh::RefreshRetry> {
        pending_retry(classification_key(db, episode_id))
    }
}
