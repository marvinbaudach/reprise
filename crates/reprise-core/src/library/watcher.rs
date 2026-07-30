//! Folder watcher (Stage 3 Task 8): real-time library updates via `notify`'s
//! recommended (inotify-backed on Linux) recursive watcher, running on its
//! own OS thread against its own [`crate::db::Db`] — never the UI's handle
//! (`rusqlite::Connection` is not `Sync`, and sharing one connection across
//! threads invites lock contention with the UI's own reads/writes anyway).
//! This mirrors `ui::window`'s scan-worker thread, which opens its own handle
//! over the same `db_path` for the identical reason (see that module's
//! `run_scan`).
//!
//! ## Debounce: collect for `DEBOUNCE` of quiescence, then one reconcile
//!
//! A single file drop — especially a large batch copy — fires many raw
//! filesystem events in quick succession (create, several writes, a final
//! rename-into-place for many editors/downloaders). Reacting to each one
//! individually would run a storm of redundant scans. Instead, every
//! observed event resets a "last event at" timestamp on the watcher thread;
//! the thread wakes on a short poll interval (`POLL_INTERVAL`) and runs
//! exactly one reconcile the moment `DEBOUNCE` has elapsed since the last
//! event, then goes back to waiting for the next batch. The pure decision
//! ("has it been long enough?") is `should_trigger_after`, factored out so
//! it's unit-testable without a live filesystem/thread.
//!
//! ## Reconcile is just `scan_folder` — Task 1.5 folded mark-vanished in
//!
//! Each reconcile now runs a single `scanner::scan_folder(root)` call.
//! Through Stage 3 this was two calls — `scan_folder` then a separate
//! `mark_vanished_under_root` — and getting their ORDER right (scan first,
//! always) was this module's job: a file moved/renamed within `root` had to
//! be reconciled by the scan's own move detection (which updates its row's
//! `path` in place) before mark-vanished decided which of the *remaining*
//! not-found paths were genuinely gone, or a moved-but-not-yet-rescanned
//! file would transiently get flagged missing. `scan_folder` now runs its
//! own mark phase internally, inside the same transaction as its walk (see
//! that function's doc comment in `library::scanner`), so there is nothing
//! left for this module to sequence — a scan's [`scanner::ScanOutcome`]
//! already reflects both the walk and the reconcile.
//!
//! `scan_folder` can also report [`scanner::ScanOutcome::RootUnavailable`]
//! instead of completing: `root` itself couldn't be seen this pass (e.g. a
//! removable/network mount not up yet). `reconcile` logs that case and still
//! calls `on_event` with an all-zero [`WatchEvent`] (never a silent skip —
//! the caller's own reload/refresh still runs), rather than pretending
//! nothing happened, but with no track ever marked missing on that basis
//! alone — see `scanner::scan_folder_inner`'s `## Root guard` doc section
//! for why marking nothing beats marking every track "unmounted".
//!
//! ## Failure is never fatal
//!
//! [`start`] returns `None` (after a `tracing::warn!`) if the underlying
//! `notify` watcher can't be created or armed on `root` (e.g. the inotify
//! watch-descriptor limit, `ENOSPC`) — the caller (`ui::window`) keeps the
//! app fully usable without live updates rather than treating this as
//! fatal, exactly like every other subsystem-unavailable degrade in this
//! codebase (GStreamer, MPRIS, D-Bus registration).
//!
//! ## Startup reconcile closes the stopped-app gap
//!
//! The OS watcher is armed before its worker thread starts. That thread runs
//! one reconcile immediately, then enters the debounced event loop. Files
//! created while Reprise was closed are therefore discovered at startup,
//! while changes racing with the initial scan are still covered by the
//! already-active recursive watch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

use crate::library::scanner::{self, ScanOutcome, ScanReport};

/// How long the watcher waits for filesystem-event quiet before running one
/// reconciling scan — see the module doc's `## Debounce` section.
const DEBOUNCE: Duration = Duration::from_secs(2);

/// How often the watcher thread wakes to check whether `DEBOUNCE` has
/// elapsed since the last event — deliberately much shorter than `DEBOUNCE`
/// so the actual trigger latency stays close to the nominal 2s rather than
/// up to `DEBOUNCE + POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Combined result of one watcher-triggered reconciliation: an incremental
/// `scan_folder` report, plus its own `vanished` count surfaced again at the
/// top level (Task 1.5 folded the separate `mark_vanished_under_root` call
/// into `scan_folder` itself, so `report.vanished` and this field are always
/// equal now — kept as two fields rather than one to avoid rippling every
/// `WatchEvent` field access elsewhere). `ui::window` uses this to log a
/// summary and reload the track list + sidebar badges.
#[derive(Debug)]
pub struct WatchEvent {
    pub report: ScanReport,
    pub vanished: u32,
    pub root_unavailable: Option<PathBuf>,
    pub auto_cleaned_ids: Vec<i64>,
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

/// Starts watching `root` recursively. Opens its own database handle over
/// `db_path` on every reconcile (never the caller's UI handle — see the
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
        if !stopped_thread.load(Ordering::SeqCst) {
            reconcile(&root_owned, &db_path, &on_event);
        }
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

/// Runs one reconcile pass: opens+migrates a fresh database handle over
/// `db_path`, runs a single incremental `scanner::scan_folder(root)` (see
/// the module doc's `## Reconcile is just scan_folder` section — Task 1.5
/// folded the walk and the mark-vanished phase into one call), and hands the
/// combined result to `on_event`. Every fallible step degrades to a logged
/// error and an early return rather than panicking — a single bad reconcile
/// must never take down the watcher thread (it simply tries again on the
/// next debounced batch). The one exception is `ScanOutcome::RootUnavailable`
/// itself, which is NOT an early return — see below.
fn reconcile(root: &Path, db_path: &Path, on_event: &impl Fn(WatchEvent)) {
    let db = match crate::db::Db::open_migrated(Some(db_path)) {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(%error, "watcher: failed to open database handle; skipping reconcile");
            return;
        }
    };

    let (report, root_unavailable, auto_cleaned_ids) = match scanner::scan_folder(&db, root) {
        Ok(ScanOutcome::Completed(report)) => {
            let auto_cleaned_ids =
                match scanner::finalize_completed_scan(&db, &report, watcher_now_unix()) {
                    Ok(ids) => ids,
                    Err(error) => {
                        tracing::error!(%error, "watcher: scan postprocessing failed");
                        Vec::new()
                    }
                };
            (report, None, auto_cleaned_ids)
        }
        Ok(ScanOutcome::RootUnavailable { root }) => {
            // Deliberately not an early return: unlike a scan/DB error (the
            // arm above), `RootUnavailable` is a normal, expected outcome
            // (a NAS not mounted yet at startup) that `on_event` still needs
            // to hear about, with an honest all-zero report — see the
            // module doc's `## Reconcile is just scan_folder` section and
            // `scanner::scan_folder_inner`'s `## Root guard` doc section.
            tracing::warn!(
                root = %root.display(),
                "watcher: scan root unavailable; reporting an empty reconcile rather than \
                 marking anything missing"
            );
            (ScanReport::default(), Some(root), Vec::new())
        }
        Err(error) => {
            tracing::error!(%error, "watcher: incremental scan failed; skipping reconcile");
            return;
        }
    };
    let vanished = report.vanished;

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

    on_event(WatchEvent {
        report,
        vanished,
        root_unavailable,
        auto_cleaned_ids,
    });
}

fn watcher_now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

/// Process-wide self-write ignore list: the tag editor writing a tag
/// directly to a file this watcher is watching would otherwise trigger the
/// same "file changed" reconcile cycle as any external change. A global
/// registry rather than state owned by one `WatcherHandle`: the tag editor
/// has no natural handle to whichever watcher happens to be running to reach
/// through, and this app only ever runs one watcher at a time. The watcher's
/// own event loop consults [`is_ignored`] per changed path
/// (`event_is_relevant`); `library::tag_edit_write::write_one_track` and
/// `library::tag_edit`'s legacy `*_ignored` batch functions are the real
/// callers of [`ignore_path`].
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
/// Called by `library::tag_edit_write::write_one_track` and
/// `library::tag_edit`'s legacy `*_ignored` batch functions, immediately
/// before each file's own write — never upfront for a whole batch, so an
/// early file's window can't expire while later files are still being
/// written.
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

    #[test]
    fn file_created_after_watcher_start_is_scanned_and_reported() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("music");
        std::fs::create_dir(&root).unwrap();
        let db_path = temp.path().join("reprise.db");
        {
            let _conn = crate::db::Db::open_migrated(Some(&db_path)).unwrap();
        }

        let (sender, receiver) = std_mpsc::sync_channel(1);
        let handle = start(&root, db_path.clone(), move |event| {
            let _ = sender.send(event);
        })
        .expect("temporary directory should be watchable");

        let startup = receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("watcher should complete its initial reconcile");
        assert_eq!(startup.report.added, 0);
        assert_eq!(startup.report.errors, 0);
        assert_eq!(startup.vanished, 0);
        assert!(startup.root_unavailable.is_none());

        let added = root.join("added-after-start.flac");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(fixture, &added).unwrap();

        let event = receiver
            .recv_timeout(DEBOUNCE + Duration::from_secs(6))
            .expect("watcher should reconcile a file created after it was armed");
        assert_eq!(event.report.added, 1);
        assert_eq!(event.report.errors, 0);
        assert_eq!(event.vanished, 0);
        assert!(event.root_unavailable.is_none());

        let conn = crate::db::Db::open_ready(&db_path).unwrap();
        let stored = crate::queries::track_id_for_path(&conn, &added.to_string_lossy()).unwrap();
        assert!(
            stored.is_some(),
            "new file should be present without a rescan"
        );
        drop(handle);
    }

    #[test]
    fn file_created_while_stopped_is_scanned_when_watcher_starts() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("music");
        std::fs::create_dir(&root).unwrap();
        let db_path = temp.path().join("reprise.db");
        {
            let _conn = crate::db::Db::open_migrated(Some(&db_path)).unwrap();
        }

        let added = root.join("added-before-start.flac");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(fixture, &added).unwrap();

        let (sender, receiver) = std_mpsc::sync_channel(1);
        let handle = start(&root, db_path.clone(), move |event| {
            let _ = sender.send(event);
        })
        .expect("temporary directory should be watchable");

        let event = receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("watcher startup should reconcile files created while it was stopped");
        assert_eq!(event.report.added, 1);
        assert_eq!(event.report.errors, 0);
        assert_eq!(event.vanished, 0);
        assert!(event.root_unavailable.is_none());

        let conn = crate::db::Db::open_ready(&db_path).unwrap();
        let stored = crate::queries::track_id_for_path(&conn, &added.to_string_lossy()).unwrap();
        assert!(
            stored.is_some(),
            "startup reconcile should persist the file"
        );
        drop(handle);
    }

    /// Fix-pass regression: `reconcile`'s `ScanOutcome::RootUnavailable` arm
    /// (see this module's `## Reconcile is just scan_folder` doc section)
    /// must still invoke `on_event` with an honest all-zero `WatchEvent`
    /// rather than silently skipping — the caller's own reload/refresh has
    /// to run either way. Calls the private `reconcile` directly (Root-Guard
    /// case (a): a root that doesn't exist at all) rather than going through
    /// `start`/a live filesystem watch, since the guard itself is exercised
    /// end-to-end by `scanner_vanished_tests.rs`; this test only needs to pin
    /// that this module's own mapping from `RootUnavailable` to a delivered,
    /// zeroed `WatchEvent` actually happens.
    #[test]
    fn reconcile_reports_root_unavailable_as_a_zeroed_event_rather_than_skipping() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("does-not-exist");
        let db_path = temp.path().join("reprise.db");
        {
            let _conn = crate::db::Db::open_migrated(Some(&db_path)).unwrap();
        }

        let (sender, receiver) = std_mpsc::sync_channel(1);
        reconcile(&root, &db_path, &move |event| {
            let _ = sender.send(event);
        });

        let event = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("RootUnavailable must still call on_event, never a silent skip");
        assert_eq!(event.report.added, 0);
        assert_eq!(event.report.errors, 0);
        assert_eq!(
            event.vanished, 0,
            "RootUnavailable must never mark anything missing"
        );
        assert_eq!(event.root_unavailable, Some(root));
        assert!(event.auto_cleaned_ids.is_empty());
    }

    #[test]
    fn completed_watcher_reconcile_runs_scan_postprocessing_and_reports_purged_ids() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("music");
        std::fs::create_dir(&root).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(fixture, root.join("present.flac")).unwrap();
        let db_path = temp.path().join("reprise.db");
        {
            let conn = crate::db::Db::open_migrated(Some(&db_path)).unwrap();
            conn.conn()
                .execute(
                    "INSERT INTO tracks \
                 (id,path,title,artist,added_at,missing_since,missing_reason) \
                 VALUES (99,'/gone.flac','Gone','Artist',0,0,'deleted')",
                    [],
                )
                .unwrap();
            crate::library::settings::set_missing_auto_clean(
                &conn,
                crate::library::settings::AutoCleanSetting::Days(0),
            )
            .unwrap();
            crate::library::settings::set_auto_clean_armed_at(&conn, 0).unwrap();
        }

        let (sender, receiver) = std_mpsc::sync_channel(1);
        reconcile(&root, &db_path, &move |event| {
            let _ = sender.send(event);
        });

        let event = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(event.auto_cleaned_ids, vec![99]);
        assert!(event.root_unavailable.is_none());
        let conn = crate::db::Db::open_migrated(Some(&db_path)).unwrap();
        assert_eq!(
            crate::library::settings::get_last_scan_relinked(&conn).unwrap(),
            Some(event.report.moved)
        );
    }
}
