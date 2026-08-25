//! Serial, cancellable library-wide artist-portrait cache population.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::artist_portrait::{cache, PortraitError, PortraitOutcome};
use crate::db::Db;
use crate::queries::{self, WindowRange};

const REPORT_INTERVAL: Duration = Duration::from_millis(250);
const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(45),
    Duration::from_secs(120),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortraitBackfillState {
    Preparing,
    Running,
    Paused,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortraitBackfillProgress {
    pub run_id: u64,
    pub state: PortraitBackfillState,
    pub done: u32,
    pub failed: u32,
    pub total: u32,
}

impl PortraitBackfillProgress {
    #[must_use]
    pub const fn idle() -> Self {
        Self {
            run_id: 0,
            state: PortraitBackfillState::Complete,
            done: 0,
            failed: 0,
            total: 0,
        }
    }
}

pub type PortraitBackfillFetch =
    dyn Fn(&str, &Path) -> Result<PortraitOutcome, PortraitError> + Send + Sync;
pub type PortraitBackfillListener = dyn Fn(PortraitBackfillProgress) + Send + Sync;

type PrepareWork = dyn FnOnce() -> Result<Vec<String>, String> + Send;
type WaitForRetry = dyn Fn(&Control, Duration) -> bool + Send + Sync;

struct Shared {
    active: bool,
    cancelled: bool,
    progress: PortraitBackfillProgress,
    listener: Option<Arc<PortraitBackfillListener>>,
}

struct Control {
    shared: Mutex<Shared>,
    wake: Condvar,
}

impl Control {
    fn new() -> Self {
        Self {
            shared: Mutex::new(Shared {
                active: false,
                cancelled: false,
                progress: PortraitBackfillProgress::idle(),
                listener: None,
            }),
            wake: Condvar::new(),
        }
    }

    fn cancelled(&self) -> bool {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancelled
    }

    fn wait(&self, duration: Duration) -> bool {
        let shared = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.cancelled {
            return true;
        }
        self.wake
            .wait_timeout_while(shared, duration, |state| !state.cancelled)
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0
            .cancelled
    }
}

/// Owns the sole worker thread and its latest immutable progress snapshot.
pub struct PortraitBackfill {
    control: Arc<Control>,
    next_run_id: AtomicU64,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl PortraitBackfill {
    #[must_use]
    pub fn new() -> Self {
        Self {
            control: Arc::new(Control::new()),
            next_run_id: AtomicU64::new(1),
            worker: Mutex::new(None),
        }
    }

    /// Starts a run whose worklist is read on the worker after `Preparing` is published.
    ///
    /// A call during an active run only replaces the listener. This is how a
    /// recreated Android activity attaches without creating a second worker.
    pub fn start(
        &self,
        database_path: PathBuf,
        cache_dir: PathBuf,
        fetch: Arc<PortraitBackfillFetch>,
        listener: Arc<PortraitBackfillListener>,
    ) -> bool {
        let prepare_cache = cache_dir.clone();
        self.launch(
            Box::new(move || {
                let db = Db::open_ready(&database_path).map_err(|error| error.to_string())?;
                let now = chrono::Utc::now().timestamp();
                pending_artists(&db, &prepare_cache, now).map_err(|error| error.to_string())
            }),
            cache_dir,
            fetch,
            listener,
            Arc::new(Control::wait),
        )
    }

    pub fn cancel(&self) {
        let listener = {
            let mut shared = self
                .control
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !shared.active {
                return;
            }
            shared.cancelled = true;
            shared.progress = PortraitBackfillProgress::idle();
            shared.listener.take()
        };
        self.control.wake.notify_all();
        if let Some(listener) = listener {
            listener(PortraitBackfillProgress::idle());
        }
    }

    #[must_use]
    pub fn progress(&self) -> PortraitBackfillProgress {
        self.control
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .progress
    }

    fn launch(
        &self,
        prepare: Box<PrepareWork>,
        cache_dir: PathBuf,
        fetch: Arc<PortraitBackfillFetch>,
        listener: Arc<PortraitBackfillListener>,
        wait: Arc<WaitForRetry>,
    ) -> bool {
        {
            let mut shared = self
                .control
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if shared.active {
                shared.listener = Some(listener);
                return false;
            }
        }

        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(previous) = worker.take() {
            if previous.is_finished() {
                let _ = previous.join();
            } else {
                *worker = Some(previous);
                return false;
            }
        }

        let run_id = self.next_run_id.fetch_add(1, Ordering::Relaxed);
        let preparing = PortraitBackfillProgress {
            run_id,
            state: PortraitBackfillState::Preparing,
            done: 0,
            failed: 0,
            total: 0,
        };
        {
            let mut shared = self
                .control
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            shared.active = true;
            shared.cancelled = false;
            shared.progress = preparing;
            shared.listener = Some(Arc::clone(&listener));
        }
        listener(preparing);

        let control = Arc::clone(&self.control);
        *worker = Some(std::thread::spawn(move || {
            let artists = match prepare() {
                Ok(artists) => artists,
                Err(error) => {
                    tracing::warn!(%error, "artist portrait backfill could not prepare its worklist");
                    finish(
                        &control,
                        PortraitBackfillProgress {
                            state: PortraitBackfillState::Complete,
                            ..preparing
                        },
                    );
                    return;
                }
            };
            run_worker(
                &control,
                preparing,
                artists,
                &cache_dir,
                fetch.as_ref(),
                wait.as_ref(),
            );
        }));
        true
    }

    #[cfg(test)]
    fn start_prepared(
        &self,
        artists: Vec<String>,
        cache_dir: PathBuf,
        fetch: Arc<PortraitBackfillFetch>,
        listener: Arc<PortraitBackfillListener>,
        wait: Arc<WaitForRetry>,
    ) -> bool {
        self.launch(
            Box::new(move || Ok(artists)),
            cache_dir,
            fetch,
            listener,
            wait,
        )
    }
}

impl Default for PortraitBackfill {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PortraitBackfill {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self
            .worker
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = worker.join();
        }
    }
}

/// Returns each library artist whose positive and negative cache entries are stale or absent.
pub fn pending_artists(
    db: &Db,
    cache_dir: &Path,
    now: i64,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut pending = Vec::new();
    let mut offset = 0_i64;
    loop {
        let window = queries::query_artists(
            db,
            "",
            WindowRange {
                offset,
                limit: i64::MAX,
            },
        )?;
        let returned = window.rows.len();
        pending.extend(window.rows.into_iter().filter_map(|artist| {
            cache::verdict(cache_dir, &artist.artist, now)
                .needs_fetch()
                .then_some(artist.artist)
        }));
        if !window.has_more || returned == 0 {
            return Ok(pending);
        }
        offset = offset.saturating_add(i64::try_from(returned).unwrap_or(i64::MAX));
    }
}

fn run_worker(
    control: &Control,
    preparing: PortraitBackfillProgress,
    artists: Vec<String>,
    cache_dir: &Path,
    fetch: &PortraitBackfillFetch,
    wait: &WaitForRetry,
) {
    let total = u32::try_from(artists.len()).unwrap_or(u32::MAX);
    if artists.is_empty() {
        finish(
            control,
            PortraitBackfillProgress {
                state: PortraitBackfillState::Complete,
                ..preparing
            },
        );
        return;
    }

    let mut queue = VecDeque::from(artists);
    let mut progress = PortraitBackfillProgress {
        state: PortraitBackfillState::Running,
        total,
        ..preparing
    };
    let mut reporter = Reporter::new(preparing);
    if !publish(control, progress, &mut reporter, false) {
        return;
    }
    let mut consecutive_errors = 0_u32;

    while let Some(artist) = queue.pop_front() {
        if control.cancelled() {
            return finish_cancelled(control);
        }
        match fetch(&artist, cache_dir) {
            Ok(PortraitOutcome::Found(_)) => {
                progress.done = progress.done.saturating_add(1);
                consecutive_errors = 0;
                progress.state = PortraitBackfillState::Running;
            }
            Ok(PortraitOutcome::NotFound) => {
                cache::write_negative(cache_dir, &artist);
                progress.failed = progress.failed.saturating_add(1);
                consecutive_errors = 0;
                progress.state = PortraitBackfillState::Running;
            }
            Err(error) => {
                tracing::debug!(artist, %error, "artist portrait backfill request will retry");
                queue.push_front(artist);
                consecutive_errors = consecutive_errors.saturating_add(1);
                if consecutive_errors >= 3 {
                    progress.state = PortraitBackfillState::Paused;
                }
            }
        }

        if !publish(control, progress, &mut reporter, false) {
            return;
        }
        if consecutive_errors >= 3 {
            let index = usize::try_from(consecutive_errors - 3)
                .unwrap_or(usize::MAX)
                .min(RETRY_DELAYS.len() - 1);
            if wait(control, RETRY_DELAYS[index]) {
                return finish_cancelled(control);
            }
        }
    }

    progress.state = PortraitBackfillState::Complete;
    finish(control, progress);
}

struct Reporter {
    last_sent: PortraitBackfillProgress,
    last_sent_at: Instant,
}

impl Reporter {
    fn new(initial: PortraitBackfillProgress) -> Self {
        Self {
            last_sent: initial,
            last_sent_at: Instant::now(),
        }
    }

    fn should_send(&self, progress: PortraitBackfillProgress, terminal: bool) -> bool {
        terminal
            || progress.state != self.last_sent.state
            || self.last_sent_at.elapsed() >= REPORT_INTERVAL
    }

    fn sent(&mut self, progress: PortraitBackfillProgress) {
        self.last_sent = progress;
        self.last_sent_at = Instant::now();
    }
}

fn publish(
    control: &Control,
    progress: PortraitBackfillProgress,
    reporter: &mut Reporter,
    terminal: bool,
) -> bool {
    let (listener, should_send) = {
        let mut shared = control
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.cancelled {
            return false;
        }
        shared.progress = progress;
        let should_send = reporter.should_send(progress, terminal);
        (shared.listener.clone(), should_send)
    };
    if should_send {
        reporter.sent(progress);
        if let Some(listener) = listener {
            listener(progress);
        }
    }
    true
}

fn finish(control: &Control, progress: PortraitBackfillProgress) {
    let listener = {
        let mut shared = control
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.cancelled {
            shared.active = false;
            return;
        }
        shared.progress = progress;
        shared.active = false;
        shared.listener.clone()
    };
    if let Some(listener) = listener {
        listener(progress);
    }
}

fn finish_cancelled(control: &Control) {
    let mut shared = control
        .shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    shared.active = false;
}

#[cfg(test)]
#[path = "backfill_tests.rs"]
mod tests;
