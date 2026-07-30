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
