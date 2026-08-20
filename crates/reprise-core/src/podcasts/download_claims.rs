//! One in-flight download per episode, whatever asks for it.
//!
//! Three callers can start the same episode's download — the download button,
//! the background fill-up, and playback — and MCP's `music_manage_episodes` is a
//! fourth. Two concurrent runs write the same `downloads::partial_path`, so
//! they corrupt each other's `.part` file.
//!
//! The guard lives here rather than in any one caller: a caller-side guard
//! (the podcasts view's `download_states` map) can only see its own dispatches,
//! and a second such map would be the same mistake twice.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

fn in_flight() -> &'static Mutex<BTreeSet<i64>> {
    static IN_FLIGHT: OnceLock<Mutex<BTreeSet<i64>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// A held claim on one episode's download. Releasing happens on `Drop`, so an
/// early return or a panic inside the download cannot leak it.
#[derive(Debug)]
pub(crate) struct DownloadClaim {
    episode_id: i64,
}

impl Drop for DownloadClaim {
    fn drop(&mut self) {
        // A poisoned lock means some other holder panicked *while mutating the
        // set*. Recovering is correct here: the set is a plain id collection
        // with no invariant a panic could have broken half-way, and refusing to
        // release would strand this episode for the process's lifetime.
        let mut guard = in_flight()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.remove(&self.episode_id);
    }
}

/// Claims `episode_id` for a download, or returns `None` if one is already in
/// flight.
pub(crate) fn claim(episode_id: i64) -> Option<DownloadClaim> {
    let mut guard = in_flight()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.insert(episode_id) {
        Some(DownloadClaim { episode_id })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_for_the_same_episode_is_refused() {
        let first = claim(4242).expect("first claim");
        assert!(claim(4242).is_none());
        drop(first);
        assert!(claim(4242).is_some(), "the claim is released on drop");
    }

    #[test]
    fn claims_for_different_episodes_coexist() {
        let _one = claim(4243).expect("first claim");
        let _two = claim(4244).expect("second claim");
    }

    #[test]
    fn a_panicking_holder_still_releases_the_claim() {
        // The claim must not survive a panicking download: `Drop` runs while
        // unwinding, a manual `release()` call would not.
        let _ = std::panic::catch_unwind(|| {
            let _held = claim(4245).expect("claim");
            panic!("download exploded");
        });
        assert!(claim(4245).is_some());
    }
}
