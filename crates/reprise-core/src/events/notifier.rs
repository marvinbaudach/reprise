//! Cross-process wake-up (Beschluss 5): a background thread with its own
//! connection watches the database directory and, after a short debounce,
//! checks `PRAGMA data_version` — which advances only when *another*
//! connection commits (any process, or another connection in this one). On a
//! change it calls `on_wake`; the consumer then reads the change log. When the
//! filesystem watch cannot be armed (network FS, inotify limit) it degrades to
//! plain 2-second polling instead of giving up. Modelled on `library::watcher`:
//! own thread, own connection, failure is never fatal.
//!
//! `PRAGMA data_version` is the truth, not the raw filesystem event: it is
//! unchanged by this connection's own reads and by its own would-be writes,
//! and changes for exactly one reason — a commit on some *other* connection.
//! That makes the wake self-filtering against incidental `-wal`/`-shm` churn
//! and makes a same-process second connection an exact stand-in for a foreign
//! process in tests.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use rusqlite::Connection;

/// Quiet window after the last filesystem event before re-checking
/// `data_version` — coalesces the burst a single commit's `-wal`/`-shm`
/// touches produce into one check.
const WAKE_DEBOUNCE_MS: u64 = 250;
const WAKE_DEBOUNCE: Duration = Duration::from_millis(WAKE_DEBOUNCE_MS);

/// Cadence of the pure-polling fallback (when `notify` cannot be armed), and
/// the armed loop's own safety re-check so a dropped `notify` event still
/// surfaces within this bound.
const POLL_FALLBACK_SECS: u64 = 2;
const POLL_FALLBACK: Duration = Duration::from_secs(POLL_FALLBACK_SECS);

/// How often each loop wakes to test its timers and to observe a dropped
/// `Handle`. Much shorter than [`WAKE_DEBOUNCE`] so armed trigger latency
/// stays close to the nominal 250 ms, and short enough that stopping is prompt.
const TICK_MS: u64 = 50;
const TICK: Duration = Duration::from_millis(TICK_MS);

/// Handle to a running notifier. Dropping it stops the background thread on its
/// next tick (the `stopped` flag) and drops the underlying watcher, which
/// unregisters the OS-level watch — the same teardown shape as
/// `library::watcher::WatcherHandle`.
pub struct Handle {
    stopped: Arc<AtomicBool>,
    _watcher: Option<notify::RecommendedWatcher>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

/// Namespace for the cross-process notifier; [`Notifier::start`] is the only
/// entry point.
pub struct Notifier;

impl Notifier {
    /// Starts watching the database at `db_path` for commits by other
    /// connections/processes, calling `on_wake` (on the notifier's own
    /// background thread) whenever one is observed. Returns `None` — after a
    /// `tracing::warn!`, never a panic — only when its own connection cannot be
    /// opened; the caller keeps the app fully usable, just without live
    /// updates. If the filesystem watch cannot be armed the notifier still
    /// starts, in the 2-second polling fallback.
    pub fn start(db_path: &Path, on_wake: impl Fn() + Send + 'static) -> Option<Handle> {
        let conn = match crate::db::open(Some(db_path)) {
            Ok(conn) => conn,
            Err(error) => {
                tracing::warn!(
                    %error,
                    db = %db_path.display(),
                    "notifier: cannot open a watch connection; continuing without live updates"
                );
                return None;
            }
        };

        match arm_watch(db_path) {
            Some((watcher, rx)) => {
                tracing::info!(db = %db_path.display(), "notifier: armed filesystem watch");
                Some(spawn(conn, Some(rx), Some(watcher), on_wake))
            }
            None => {
                tracing::warn!(
                    db = %db_path.display(),
                    "notifier: could not arm filesystem watch; falling back to {POLL_FALLBACK_SECS}s polling"
                );
                Some(spawn(conn, None, None, on_wake))
            }
        }
    }

    /// Test seam: start in the polling fallback unconditionally, so the
    /// degraded path can be exercised headlessly without forcing a real watch
    /// failure. The wake mechanism (`data_version`) is identical to armed mode.
    #[cfg(test)]
    pub(crate) fn start_polling_for_test(
        db_path: &Path,
        on_wake: impl Fn() + Send + 'static,
    ) -> Option<Handle> {
        let conn = crate::db::open(Some(db_path)).ok()?;
        Some(spawn(conn, None, None, on_wake))
    }
}

/// Arms a non-recursive watch on the *directory* holding the database. The
/// `-wal` file where WAL-mode commits actually land is created and truncated
/// over the database's life, so watching the two files directly would miss
/// events after a checkpoint; watching their directory catches the main DB,
/// `-wal` and `-shm` alike. Returns `None` if the watcher cannot be created or
/// armed (the caller then polls instead).
fn arm_watch(
    db_path: &Path,
) -> Option<(
    notify::RecommendedWatcher,
    std_mpsc::Receiver<notify::Result<notify::Event>>,
)> {
    let watch_dir = db_path.parent()?.to_path_buf();
    let (tx, rx) = std_mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        // The only send failure is a disconnected receiver (the thread already
        // exited); nothing to do but drop the event.
        let _ = tx.send(result);
    })
    .inspect_err(|error| tracing::debug!(%error, "notifier: cannot create watcher"))
    .ok()?;
    watcher
        .watch(&watch_dir, RecursiveMode::NonRecursive)
        .inspect_err(
            |error| tracing::debug!(%error, dir = %watch_dir.display(), "notifier: cannot arm watch"),
        )
        .ok()?;
    Some((watcher, rx))
}

fn spawn(
    conn: Connection,
    rx: Option<std_mpsc::Receiver<notify::Result<notify::Event>>>,
    watcher: Option<notify::RecommendedWatcher>,
    on_wake: impl Fn() + Send + 'static,
) -> Handle {
    // Establish the baseline before returning the handle. If the worker read
    // it after being spawned, a commit in that scheduling gap could become
    // the baseline and remain invisible forever.
    let initial_version = current_data_version(&conn);
    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_thread = stopped.clone();
    std::thread::spawn(move || match rx {
        Some(rx) => run_armed(&conn, &rx, &stopped_thread, initial_version, &on_wake),
        None => run_polling(&conn, &stopped_thread, initial_version, &on_wake),
    });
    Handle {
        stopped,
        _watcher: watcher,
    }
}

/// Armed loop: a filesystem event opens a debounce window; once it has been
/// quiet for [`WAKE_DEBOUNCE`] (or [`POLL_FALLBACK`] has elapsed as a safety
/// re-check) it consults `data_version` and wakes only on a real change.
fn run_armed(
    conn: &Connection,
    rx: &std_mpsc::Receiver<notify::Result<notify::Event>>,
    stopped: &AtomicBool,
    mut last_version: i64,
    on_wake: &(impl Fn() + Send + 'static),
) {
    let mut pending_since: Option<Instant> = None;
    let mut last_check = Instant::now();

    loop {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        match rx.recv_timeout(TICK) {
            Ok(_event) => pending_since = Some(Instant::now()),
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
            Err(std_mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let debounced = pending_since.is_some_and(|since| since.elapsed() >= WAKE_DEBOUNCE);
        let safety_due = last_check.elapsed() >= POLL_FALLBACK;
        if debounced || safety_due {
            pending_since = None;
            last_check = Instant::now();
            wake_if_changed(conn, &mut last_version, on_wake);
        }
    }
}

/// Polling fallback: no filesystem events, just a `data_version` check every
/// [`POLL_FALLBACK`], waking on a real change.
fn run_polling(
    conn: &Connection,
    stopped: &AtomicBool,
    mut last_version: i64,
    on_wake: &(impl Fn() + Send + 'static),
) {
    let mut last_check = Instant::now();

    loop {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        if last_check.elapsed() >= POLL_FALLBACK {
            last_check = Instant::now();
            wake_if_changed(conn, &mut last_version, on_wake);
        }
        std::thread::sleep(TICK);
    }
}

fn wake_if_changed(conn: &Connection, last_version: &mut i64, on_wake: &impl Fn()) {
    match data_version(conn) {
        Ok(current) if current != *last_version => {
            *last_version = current;
            on_wake();
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, "notifier: data_version check failed; will retry");
        }
    }
}

/// Reads `PRAGMA data_version`. A read failure yields `0` for the baseline so a
/// later successful read that differs still triggers exactly one wake rather
/// than silently wedging the loop.
fn current_data_version(conn: &Connection) -> i64 {
    data_version(conn).unwrap_or(0)
}

fn data_version(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
}

#[cfg(test)]
#[path = "notifier_tests.rs"]
mod tests;
