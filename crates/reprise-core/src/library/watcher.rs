//! Folder watcher (Stage 3 Task 8): real-time library updates via `notify`'s
//! recommended (inotify-backed on Linux) recursive watcher, running on its
//! own OS thread against its own `rusqlite::Connection` — never the UI's
//! `Rc<RefCell<Connection>>` (not `Send`, and sharing one connection across
//! threads invites lock contention with the UI's own reads/writes anyway).
//! This mirrors `ui::window`'s scan-worker thread, which opens its own
//! connection over the same `db_path` for the identical reason (see that
//! module's `run_scan`).
//!
//! ## Debounce: collect for `DEBOUNCE` of quiescence, then one reconcile
//!
//! A single file drop — especially a large batch copy — fires many raw
//! filesystem events in quick succession (create, several writes, a final
//! rename-into-place for many editors/downloaders). Reacting to each one
//! individually would run a storm of redundant scans. Instead, every
//! observed event resets a "last event at" timestamp on the watcher thread;
//! the thread wakes on a short poll interval ([`POLL_INTERVAL`]) and runs
//! exactly one reconcile the moment [`DEBOUNCE`] has elapsed since the last
//! event, then goes back to waiting for the next batch. The pure decision
//! ("has it been long enough?") is [`should_trigger_after`], factored out so
//! it's unit-testable without a live filesystem/thread.
//!
//! ## Reconcile order: scan THEN mark-vanished, never the reverse
//!
//! Each reconcile runs `scanner::scan_folder(root)` first, then `scanner::
//! mark_vanished_under_root(root)` — see that function's doc comment for why
//! this order matters: a file moved/renamed within `root` must be
//! reconciled by the scan's move detection (which updates its row's `path`
//! in place) before this watcher decides which of the *remaining*
//! not-found paths are genuinely gone. Running mark-vanished first would
//! transiently flag a moved-but-not-yet-rescanned file as missing.
//!
//! ## Failure is never fatal
//!
//! [`start`] returns `None` (after a `tracing::warn!`) if the underlying
//! `notify` watcher can't be created or armed on `root` (e.g. the inotify
//! watch-descriptor limit, `ENOSPC`) — the caller (`ui::window`) keeps the
//! app fully usable without live updates rather than treating this as
//! fatal, exactly like every other subsystem-unavailable degrade in this
//! codebase (GStreamer, MPRIS, D-Bus registration).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

use crate::library::scanner::{self, ScanReport};

/// How long the watcher waits for filesystem-event quiet before running one
/// reconciling scan — see the module doc's `## Debounce` section.
const DEBOUNCE: Duration = Duration::from_secs(2);

/// How often the watcher thread wakes to check whether `DEBOUNCE` has
/// elapsed since the last event — deliberately much shorter than `DEBOUNCE`
/// so the actual trigger latency stays close to the nominal 2s rather than
/// up to `DEBOUNCE + POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Combined result of one watcher-triggered reconciliation: an incremental
/// `scan_folder` report plus `mark_vanished_under_root`'s newly-marked
/// count. `ui::window` uses this to log a summary and reload the track list
/// + sidebar badges.
#[derive(Debug)]
pub struct WatchEvent {
    pub report: ScanReport,
    pub vanished: u32,
}

/// Handle to a running watcher. Dropping it stops the background thread on
/// its next `POLL_INTERVAL` wake (the `stopped` flag) and drops the
/// underlying `notify::RecommendedWatcher`, which unregisters the OS-level
/// watch. `ui::window` holds at most one of these at a time (in a `RefCell`)
/// — starting a new watcher after a fresh "Scan folder…" replaces it, whose
/// `Drop` tears down the old watch before the new one is armed.
pub struct WatcherHandle {
    _watcher: notify::RecommendedWatcher,
    stopped: Arc<AtomicBool>,
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

/// Pure debounce decision, extracted for unit testing without a live
/// filesystem/thread: given how long it has been since the last observed
/// event, is it time to run the reconciling scan?
fn should_trigger_after(quiet_duration: Duration) -> bool {
    quiet_duration >= DEBOUNCE
}

/// Starts watching `root` recursively. Opens its own connection over
/// `db_path` on every reconcile (never the caller's UI connection — see the
/// module doc). Returns `None` after a `tracing::warn!` if the underlying
/// `notify` watcher can't be created or armed — see the module doc's `##
/// Failure is never fatal` section. `on_event` runs on this watcher's own
/// background thread, not the GTK main thread: `ui::window`'s wiring sends
/// each `WatchEvent` through an `async_channel` back to the main thread,
/// mirroring the bridge `spawn_scan` already uses for the one-shot scan
/// result.
pub fn start(
    root: &Path,
    db_path: PathBuf,
    on_event: impl Fn(WatchEvent) + Send + 'static,
) -> Option<WatcherHandle> {
    let (tx, rx) = std_mpsc::channel::<notify::Result<notify::Event>>();

    let mut watcher = match notify::recommended_watcher(move |result| {
        // The only failure mode of `Sender::send` is a disconnected
        // receiver (the watcher thread has already exited) — nothing to do
        // but drop the event; the watcher is shutting down anyway.
        let _ = tx.send(result);
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            tracing::warn!(
                %error,
                root = %root.display(),
                "watcher: failed to create filesystem watcher; continuing without live updates"
            );
            return None;
        }
    };

    if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
        tracing::warn!(
            %error,
            root = %root.display(),
            "watcher: failed to arm watch on library root; continuing without live updates"
        );
        return None;
    }

    tracing::info!(root = %root.display(), "watcher: armed on library root");

    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_thread = stopped.clone();
    let root_owned = root.to_path_buf();

    std::thread::spawn(move || {
        run_watch_loop(&rx, &stopped_thread, &root_owned, &db_path, &on_event);
    });

    Some(WatcherHandle {
        _watcher: watcher,
        stopped,
    })
}

/// The watcher thread's main loop: drains `rx` for up to `POLL_INTERVAL` at a
/// time, tracks the last relevant-event timestamp, and reconciles once
/// `should_trigger_after` says the quiet period has elapsed. Exits when
/// `stopped` is set (the `WatcherHandle` was dropped) or the event channel
/// disconnects (the `notify::RecommendedWatcher` itself was dropped, which
/// can't happen while `WatcherHandle` is alive, but is handled defensively
/// rather than looping forever on a dead channel).
fn run_watch_loop(
    rx: &std_mpsc::Receiver<notify::Result<notify::Event>>,
    stopped: &AtomicBool,
    root: &Path,
    db_path: &Path,
    on_event: &(impl Fn(WatchEvent) + Send + 'static),
) {
    let mut last_event_at: Option<Instant> = None;
    let mut pending = false;

    loop {
        if stopped.load(Ordering::SeqCst) {
            tracing::debug!("watcher: stop requested; exiting thread");
            return;
        }

        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(Ok(event)) => {
                if event_is_relevant(&event) {
                    last_event_at = Some(Instant::now());
                    pending = true;
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "watcher: filesystem event error; continuing");
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                tracing::warn!("watcher: event channel disconnected; exiting thread");
                return;
            }
        }

        if !pending {
            continue;
        }
        let quiet = last_event_at.map_or(Duration::ZERO, |at| at.elapsed());
        if should_trigger_after(quiet) {
            pending = false;
            reconcile(root, db_path, on_event);
        }
    }
}

/// Whether a raw `notify::Event` should reset the debounce timer.
///
/// ## `Access` events must NEVER count — the self-triggering feedback loop
///
/// `notify`'s Linux (inotify) backend registers `IN_OPEN`/`IN_CLOSE_WRITE`/
/// `IN_CLOSE_NOWRITE` alongside `IN_CREATE`/`IN_MODIFY`/`IN_DELETE`/…, and
/// surfaces the former as `EventKind::Access(_)`. Every reconcile's own
/// `scanner::scan_folder` call opens each audio file read-only (`lofty`'s tag
/// read) — which is itself a filesystem access on a watched path, so it
/// generates a fresh `Access` event. Treating that as "relevant" would mean
/// every reconcile schedules its own successor once the debounce window
/// elapses, forever, even with zero real user activity — an actual bug
/// caught in this task's own headless E2E verification (repeated `added=0
/// vanish-marked=0` reconciles firing every ~2s with nothing on disk having
/// changed). `Access` events carry no create/modify/remove/rename
/// information at all, so excluding them entirely is always correct, not
/// just a workaround for this specific loop.
///
/// Beyond that: every path an event *does* touch must not be under an active
/// [`ignore_path`] window (Stage 4 prep — see that function's doc comment).
/// An event with no paths at all (some platforms emit these for rescan/
/// overflow notifications) is treated conservatively as relevant, since
/// there's nothing to check it against.
fn event_is_relevant(event: &notify::Event) -> bool {
    if matches!(event.kind, notify::EventKind::Access(_)) {
        return false;
    }
    if event.paths.is_empty() {
        return true;
    }
    event.paths.iter().any(|path| !is_ignored(path))
}

/// Runs one reconcile pass: opens+migrates a fresh connection over
/// `db_path`, runs an incremental `scanner::scan_folder(root)`, then
/// `scanner::mark_vanished_under_root(root)` (see the module doc's `##
/// Reconcile order` section), and hands the combined result to `on_event`.
/// Every fallible step degrades to a logged error and an early return rather
/// than panicking — a single bad reconcile must never take down the
/// watcher thread (it simply tries again on the next debounced batch).
fn reconcile(root: &Path, db_path: &Path, on_event: &impl Fn(WatchEvent)) {
    let mut conn = match crate::db::open(Some(db_path)) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(%error, "watcher: failed to open database connection; skipping reconcile");
            return;
        }
    };
    if let Err(error) = crate::db::migrate(&conn) {
        tracing::error!(%error, "watcher: failed to migrate database; skipping reconcile");
        return;
    }

    let report = match scanner::scan_folder(&mut conn, root) {
        Ok(report) => report,
        Err(error) => {
            tracing::error!(%error, "watcher: incremental scan failed; skipping reconcile");
            return;
        }
    };

    let vanished = match scanner::mark_vanished_under_root(&conn, root) {
        Ok(count) => count,
        Err(error) => {
            tracing::error!(%error, "watcher: mark_vanished_under_root failed");
            0
        }
    };

    tracing::info!(
        added = report.added,
        updated = report.updated,
        moved = report.moved,
        errors = report.errors,
        vanished,
        "watcher: scan added={} updated={} moved={} errors={} vanish-marked={}",
        report.added,
        report.updated,
        report.moved,
        report.errors,
        vanished,
    );

    on_event(WatchEvent { report, vanished });
}

/// Process-wide self-write ignore list (Stage 4 prep): a future tag editor
/// writing a tag directly to a file this watcher is watching would otherwise
/// trigger the same "file changed" reconcile cycle as any external change.
/// A global registry rather than state owned by one `WatcherHandle`: a
/// future caller (the tag editor) has no natural handle to whichever watcher
/// happens to be running to reach through, and this app only ever runs one
/// watcher at a time. No consumer calls [`ignore_path`] yet — this task
/// builds the API and proves it works (see the test module below); the
/// watcher's own event loop already consults [`is_ignored`] per changed path
/// (`event_is_relevant`), so wiring in a real caller later needs no further
/// change here.
static IGNORE_LIST: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();

fn ignore_list() -> &'static Mutex<HashMap<PathBuf, Instant>> {
    IGNORE_LIST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Marks `path` to be ignored by the watcher for `duration` from now: an
/// event touching exactly this path arriving before the deadline elapses
/// does not reset the debounce timer (see `event_is_relevant`). Exact path
/// match only — no symlink/canonicalization resolution, consistent with
/// every other path comparison in this module.
///
/// `#[allow(dead_code)]`: no consumer calls this outside its own tests yet
/// (Stage 4's tag editor adds the first real one) — see this function's doc
/// comment and the module doc's `## Failure is never fatal`-adjacent
/// `IGNORE_LIST` section for why the mechanism is still built and tested now
/// rather than deferred.
#[allow(dead_code)]
pub fn ignore_path(path: &Path, duration: Duration) {
    let deadline = Instant::now() + duration;
    ignore_list()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path.to_path_buf(), deadline);
}

/// Whether `path` is currently within an active [`ignore_path`] window.
/// Lazily prunes `path`'s own entry once its deadline has passed rather than
/// running a background sweep — the registry only ever holds a handful of
/// recently-written paths at a time, so a stale entry sitting unpruned until
/// its own path is next checked is not a meaningful leak.
pub fn is_ignored(path: &Path) -> bool {
    let mut list = ignore_list().lock().unwrap_or_else(PoisonError::into_inner);
    match list.get(path) {
        Some(deadline) if *deadline > Instant::now() => true,
        Some(_) => {
            list.remove(path);
            false
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_trigger_after_requires_the_full_debounce_window() {
        assert!(!should_trigger_after(Duration::from_millis(500)));
        assert!(!should_trigger_after(DEBOUNCE - Duration::from_millis(1)));
    }

    #[test]
    fn should_trigger_after_fires_once_the_window_elapses() {
        assert!(should_trigger_after(DEBOUNCE));
        assert!(should_trigger_after(DEBOUNCE + Duration::from_secs(10)));
    }

    #[test]
    fn ignore_path_marks_a_path_ignored_until_it_expires() {
        let path = PathBuf::from("/tmp/reprise-watcher-test-ignore-marks-until-expired.flac");
        assert!(!is_ignored(&path), "not ignored before ignore_path");

        ignore_path(&path, Duration::from_millis(50));
        assert!(is_ignored(&path), "ignored immediately after ignore_path");

        std::thread::sleep(Duration::from_millis(80));
        assert!(
            !is_ignored(&path),
            "no longer ignored once the window elapses"
        );
    }

    #[test]
    fn ignore_path_does_not_affect_an_unrelated_path() {
        let ignored = PathBuf::from("/tmp/reprise-watcher-test-ignore-unrelated-a.flac");
        let other = PathBuf::from("/tmp/reprise-watcher-test-ignore-unrelated-b.flac");
        ignore_path(&ignored, Duration::from_secs(5));
        assert!(is_ignored(&ignored));
        assert!(!is_ignored(&other));
    }

    #[test]
    fn event_is_relevant_true_for_an_event_with_no_paths() {
        let event = notify::Event::new(notify::EventKind::Other);
        assert!(event_is_relevant(&event));
    }

    #[test]
    fn event_is_relevant_false_when_every_path_is_ignored() {
        let path = PathBuf::from("/tmp/reprise-watcher-test-event-relevance.flac");
        ignore_path(&path, Duration::from_secs(5));
        let event = notify::Event::new(notify::EventKind::Any).add_path(path);
        assert!(!event_is_relevant(&event));
    }

    #[test]
    fn event_is_relevant_true_when_a_path_is_not_ignored() {
        let path = PathBuf::from("/tmp/reprise-watcher-test-event-relevance-not-ignored.flac");
        let event = notify::Event::new(notify::EventKind::Any).add_path(path);
        assert!(event_is_relevant(&event));
    }

    /// The self-triggering feedback loop this task's own headless E2E run
    /// caught (see `event_is_relevant`'s doc comment): a reconcile's own
    /// `scan_folder` read of a watched file generates an `Access` event for
    /// that same path, which must NEVER count as relevant — otherwise every
    /// reconcile would forever schedule its own successor.
    #[test]
    fn event_is_relevant_false_for_an_access_event_even_with_an_unignored_path() {
        let path = PathBuf::from("/tmp/reprise-watcher-test-event-relevance-access.flac");
        let event = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Open(
            notify::event::AccessMode::Any,
        )))
        .add_path(path);
        assert!(!event_is_relevant(&event));
    }
}
