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

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use super::download_state::DownloadState;

const ABANDONED_DOWNLOAD_MESSAGE: &str = "podcast download ended without a terminal state";

#[derive(Debug, Default)]
struct SharedState {
    events: Vec<DownloadState>,
    terminal: Option<DownloadState>,
}

#[derive(Debug, Default)]
struct InFlightDownload {
    state: Mutex<SharedState>,
    changed: Condvar,
}

impl InFlightDownload {
    fn report(&self, state: DownloadState) {
        let mut shared = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.terminal.is_some() {
            return;
        }
        if is_terminal(&state) {
            shared.terminal = Some(state.clone());
        }
        shared.events.push(state);
        self.changed.notify_all();
    }

    fn abandon_if_running(&self) {
        self.report(DownloadState::Failed {
            message: ABANDONED_DOWNLOAD_MESSAGE.to_owned(),
        });
    }
}

fn in_flight() -> &'static Mutex<BTreeMap<i64, Arc<InFlightDownload>>> {
    static IN_FLIGHT: OnceLock<Mutex<BTreeMap<i64, Arc<InFlightDownload>>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn is_terminal(state: &DownloadState) -> bool {
    matches!(
        state,
        DownloadState::Downloaded { .. } | DownloadState::Failed { .. }
    )
}

/// A held claim on one episode's download. Releasing happens on `Drop`, so an
/// early return or a panic inside the download cannot leak it.
#[derive(Debug)]
pub(crate) struct DownloadClaim {
    episode_id: i64,
    download: Arc<InFlightDownload>,
}

impl DownloadClaim {
    pub(crate) fn report(&self, state: DownloadState) {
        self.download.report(state);
    }
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
        if guard
            .get(&self.episode_id)
            .is_some_and(|download| Arc::ptr_eq(download, &self.download))
        {
            guard.remove(&self.episode_id);
        }
        drop(guard);
        self.download.abandon_if_running();
    }
}

#[derive(Debug)]
pub(crate) struct DownloadWaiter {
    download: Arc<InFlightDownload>,
}

impl DownloadWaiter {
    /// Blocks the current thread until the in-flight download reports a
    /// terminal state. Callers must run this only on a worker thread.
    pub(crate) fn wait(self, on_progress: &mut dyn FnMut(DownloadState)) -> DownloadState {
        let mut next_event = 0;
        loop {
            let (events, terminal) = {
                let mut shared = self
                    .download
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                while next_event == shared.events.len() && shared.terminal.is_none() {
                    shared = self
                        .download
                        .changed
                        .wait(shared)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                let events = shared.events[next_event..].to_vec();
                next_event = shared.events.len();
                (events, shared.terminal.clone())
            };
            for state in events {
                on_progress(state);
            }
            if let Some(terminal) = terminal {
                return terminal;
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum ClaimOutcome {
    Acquired(DownloadClaim),
    Running(DownloadWaiter),
}

/// Claims `episode_id` for a download or returns a waiter for the active run.
pub(crate) fn claim(episode_id: i64) -> ClaimOutcome {
    let mut guard = in_flight()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(download) = guard.get(&episode_id) {
        return ClaimOutcome::Running(DownloadWaiter {
            download: Arc::clone(download),
        });
    }
    let download = Arc::new(InFlightDownload::default());
    guard.insert(episode_id, Arc::clone(&download));
    ClaimOutcome::Acquired(DownloadClaim {
        episode_id,
        download,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_for_the_same_episode_is_refused() {
        let ClaimOutcome::Acquired(first) = claim(4242) else {
            panic!("first claim must be acquired");
        };
        assert!(matches!(claim(4242), ClaimOutcome::Running(_)));
        drop(first);
        assert!(matches!(claim(4242), ClaimOutcome::Acquired(_)));
    }

    #[test]
    fn claims_for_different_episodes_coexist() {
        let ClaimOutcome::Acquired(_one) = claim(4243) else {
            panic!("first claim must be acquired");
        };
        let ClaimOutcome::Acquired(_two) = claim(4244) else {
            panic!("second claim must be acquired");
        };
    }

    #[test]
    fn dropping_a_holder_wakes_its_waiter_with_a_failure() {
        let ClaimOutcome::Acquired(held) = claim(4245) else {
            panic!("first claim must be acquired");
        };
        let ClaimOutcome::Running(waiter) = claim(4245) else {
            panic!("second claim must wait");
        };

        drop(held);

        let state = waiter.wait(&mut |_| {});
        assert_eq!(
            state,
            DownloadState::Failed {
                message: ABANDONED_DOWNLOAD_MESSAGE.to_owned()
            }
        );
    }

    #[test]
    fn a_panicking_holder_still_releases_the_claim() {
        // The claim must not survive a panicking download: `Drop` runs while
        // unwinding, a manual `release()` call would not.
        let ClaimOutcome::Acquired(held) = claim(4246) else {
            panic!("claim must be acquired");
        };
        let ClaimOutcome::Running(waiter) = claim(4246) else {
            panic!("second claim must wait");
        };
        let _ = std::panic::catch_unwind(move || {
            let _held = held;
            panic!("download exploded");
        });
        assert!(matches!(
            waiter.wait(&mut |_| {}),
            DownloadState::Failed { .. }
        ));
        assert!(matches!(claim(4246), ClaimOutcome::Acquired(_)));
    }
}
