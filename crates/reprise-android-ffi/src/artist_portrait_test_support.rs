//! Helpers the artist-portrait backfill tests share.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::ArtistPortraitProgressUpdate;

const ATTEMPT_WINDOW: Duration = Duration::from_millis(300);
const SETTLE_DEADLINE: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Starts a run and waits for an update `settled` accepts, failing the test if none comes.
///
/// A start that races the previous run's worker thread exiting is answered "busy" and
/// dropped, which a fixed sleep before the start cannot rule out. So the start is repeated
/// until one is accepted: an observed settled run proves it was, and a start that never is
/// fails at the deadline instead of passing vacuously.
pub(super) fn start_until_settled(
    start: impl Fn(),
    updates: &Mutex<Vec<ArtistPortraitProgressUpdate>>,
    from: usize,
    settled: impl Fn(ArtistPortraitProgressUpdate) -> bool,
) {
    let deadline = Instant::now() + SETTLE_DEADLINE;
    while Instant::now() < deadline {
        start();
        let attempt_ends = Instant::now() + ATTEMPT_WINDOW;
        while Instant::now() < attempt_ends {
            let seen = updates
                .lock()
                .unwrap()
                .get(from..)
                .is_some_and(|updates| updates.iter().copied().any(&settled));
            if seen {
                return;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
    panic!(
        "no started run settled within {SETTLE_DEADLINE:?}: {:?}",
        updates.lock().unwrap()
    );
}
