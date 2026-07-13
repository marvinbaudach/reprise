//! The library-scan flow extracted from `ui::window` (Queue-C refactor Task
//! 3): the "Scan folder…" button wiring, the "Rescan library" trigger, the
//! background scan worker (`spawn_scan`/`run_scan`), the headless
//! `REPRISE_SMOKE_RESCAN` hook, and the folder-watcher (re)start logic.
//! `window::build` calls into here via `super::scan_flow::…`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library;
use reprise_core::library::scanner::{ScanError, ScanProgress, ScanReport};
use reprise_core::library::settings;
use reprise_core::library::watcher::{self, WatcherHandle};

use super::scan_progress::ScanProgressView;
use super::sidebar::Sidebar;
use super::strings;
use super::toasts;
use super::track_list::TrackList;

/// Dev/verification hook (permanent, like the others in this module and in
/// `track_list.rs`): when set to a directory, arms a one-shot idle callback
/// that calls `spawn_scan` directly — the exact function a real "Scan
/// folder…" click hands off to — once the main loop is up, skipping the
/// portal `gtk::FileDialog` folder picker (not headlessly drivable). Added
/// for Stage 3 Task 4 review finding #2's verification: `main.rs`'s
/// `REPRISE_SCAN_DIR` runs its scan *before* the window/sidebar even exist,
/// so it can never appear as its own "sidebar refresh #N (scan completed)"
/// log line — this hook fires after everything is built and wired, so it
/// does, giving headless E2E a real, attributable post-launch scan to grep
/// for.
///
/// Usage: `REPRISE_SCAN_DIR=<fixtures> REPRISE_SMOKE_RESCAN=<dir2>
/// REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_RESCAN_ENV_VAR: &str = "REPRISE_SMOKE_RESCAN";

type OnScanComplete = Rc<dyn Fn()>;

#[derive(Clone, Default)]
struct ScanCompletion(Rc<RefCell<Option<OnScanComplete>>>);

impl ScanCompletion {
    fn set(&self, callback: impl Fn() + 'static) {
        self.0.borrow_mut().replace(Rc::new(callback));
    }

    fn notify(&self) {
        let callback = self.0.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

#[derive(Clone)]
pub(super) struct ScanControls {
    button: gtk4::Button,
    progress: ScanProgressView,
    completion: ScanCompletion,
}

impl ScanControls {
    pub(super) fn new(button: &gtk4::Button, progress: &ScanProgressView) -> Self {
        Self {
            button: button.clone(),
            progress: progress.clone(),
            completion: ScanCompletion::default(),
        }
    }

    pub(super) fn set_on_complete(&self, callback: impl Fn() + 'static) {
        self.completion.set(callback);
    }
}

/// Arms the `REPRISE_SMOKE_RESCAN` hook (see `SMOKE_RESCAN_ENV_VAR`'s doc
/// comment): one idle callback, deferred so it runs once the main loop is up
/// (matching `track_list.rs`'s `arm_smoke_*` hooks), that calls `spawn_scan`
/// with the given directory — exactly what `wire_scan_button`'s click
/// handler does after a folder is chosen, minus the dialog.
pub(super) fn arm_smoke_rescan(
    controls: &ScanControls,
    toast_overlay: &adw::ToastOverlay,
    db_path: PathBuf,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
    watcher_state: Rc<RefCell<Option<WatcherHandle>>>,
) {
    let Ok(dir) = std::env::var(SMOKE_RESCAN_ENV_VAR) else {
        return;
    };
    tracing::info!(dir = %dir, "{SMOKE_RESCAN_ENV_VAR} set: arming headless post-launch rescan");
    let controls = controls.clone();
    let toast_overlay = toast_overlay.clone();
    glib::idle_add_local_once(move || {
        spawn_scan(
            PathBuf::from(dir),
            db_path,
            controls,
            toast_overlay,
            track_list,
            sidebar,
            watcher_state,
        );
    });
}

/// Wires the header's "Scan folder…" button: a click opens a portal-friendly
/// `gtk::FileDialog` folder picker; a chosen folder starts a background scan
/// (see `spawn_scan`). Dismissing the dialog without choosing a folder is a
/// normal, expected outcome (not an error) — logged at debug and otherwise
/// ignored.
pub(super) fn wire_scan_button(
    controls: &ScanControls,
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    db_path: PathBuf,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
    watcher_state: Rc<RefCell<Option<WatcherHandle>>>,
) {
    let window = window.clone();
    let toast_overlay = toast_overlay.clone();
    let controls = controls.clone();

    controls.button.clone().connect_clicked(move |_| {
        // Disable synchronously, before the async dialog even opens: a
        // second click landing while the first dialog is still up must not
        // be able to spawn a second dialog (and thus a second concurrent
        // scan worker against the same DB). Every exit path below that does
        // *not* hand off to `spawn_scan` must re-enable the button; the
        // `spawn_scan` path re-enables it itself once the scan finishes.
        controls.button.set_sensitive(false);

        let dialog = gtk4::FileDialog::builder()
            .title(strings::text(strings::SCAN_DIALOG_TITLE))
            .modal(true)
            .build();

        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        let db_path = db_path.clone();
        let track_list = track_list.clone();
        let sidebar = sidebar.clone();
        let controls = controls.clone();
        let watcher_state = watcher_state.clone();

        glib::spawn_future_local(async move {
            let folder = match dialog.select_folder_future(Some(&window)).await {
                Ok(folder) => folder,
                Err(error) => {
                    // Dismissed (Escape/Cancel) or Cancelled: the user simply
                    // changed their mind — not a failure worth a toast.
                    if error.matches(gtk4::DialogError::Dismissed)
                        || error.matches(gtk4::DialogError::Cancelled)
                    {
                        tracing::debug!("scan folder dialog dismissed");
                    } else {
                        tracing::error!(%error, "scan folder dialog failed");
                    }
                    controls.button.set_sensitive(true);
                    return;
                }
            };
            let Some(path) = folder.path() else {
                tracing::warn!("selected folder has no local filesystem path; cannot scan");
                controls.button.set_sensitive(true);
                return;
            };

            spawn_scan(
                path,
                db_path,
                controls,
                toast_overlay,
                track_list,
                sidebar,
                watcher_state,
            );
        });
    });
}

/// "Rescan library" (Stage 3 Task 8, Missing-source context menu action):
/// re-runs the persisted library root (`library::settings::LIBRARY_ROOT_
/// KEY`) through the exact same `spawn_scan` flow "Scan folder…" uses, minus
/// the folder-picker dialog — mirrors `arm_smoke_rescan`'s reasoning for
/// reusing `spawn_scan` directly. Guards against firing a second concurrent
/// scan while `scan_button` shows one is already running (its `is_
/// sensitive()` is the same flag `spawn_scan` toggles), and surfaces a toast
/// rather than silently doing nothing when no folder has ever been scanned.
pub(super) fn trigger_rescan_of_library_root(
    conn: &Rc<RefCell<Connection>>,
    controls: &ScanControls,
    toast_overlay: &adw::ToastOverlay,
    db_path: PathBuf,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
) {
    if !controls.button.is_sensitive() {
        tracing::debug!("rescan library: a scan is already running; ignoring");
        toasts::show(toast_overlay, &strings::scan_already_running_toast());
        return;
    }

    let root = {
        let conn = conn.borrow();
        settings::get_library_root(&conn)
    };
    let root = match root {
        Ok(Some(root)) => PathBuf::from(root),
        Ok(None) => {
            toasts::show(toast_overlay, &strings::no_library_root_to_rescan_toast());
            return;
        }
        Err(error) => {
            tracing::error!(%error, "rescan library: failed to read persisted library root");
            toasts::show(toast_overlay, &strings::no_library_root_to_rescan_toast());
            return;
        }
    };

    spawn_scan(
        root,
        db_path,
        controls.clone(),
        toast_overlay.clone(),
        track_list,
        sidebar,
        watcher_state.clone(),
    );
}

/// Starts a background scan of `folder`: disables `scan_button`, reveals
/// `scan_progress`, and runs `library::scanner::scan_folder_with_progress` on a
/// `std::thread` against a *separate* `rusqlite::Connection` opened from
/// `db_path` (a `Connection` cannot cross threads), then marshals the result
/// back onto the GTK main thread over a `bounded(1)` progress channel. While
/// scanning, a full channel slot is replaced with the newest progress event,
/// so a fast worker can neither block nor build an unbounded UI backlog. The
/// separate one-shot result future waits until that progress channel has been
/// fully drained before hiding the row. On success: re-enable the button,
/// reload the
/// track list (`TrackList::reload`'s `on_reload` hook keeps the status line
/// in sync too — see its doc comment — so this doesn't refresh it a second
/// time itself), and refresh the sidebar (trigger #1 from `Sidebar::
/// refresh`'s doc comment — a scan can add tracks/playlists and clear
/// import-error/missing counts, none of which the narrowed `on_reload` hook
/// covers any more). On failure: re-enable the button, log at `error!`, and
/// surface an `adw::Toast` — the app stays fully usable either way (fault
/// tolerance: a scan failure must never wedge the UI or crash the app).
fn spawn_scan(
    folder: PathBuf,
    db_path: PathBuf,
    controls: ScanControls,
    toast_overlay: adw::ToastOverlay,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
    watcher_state: Rc<RefCell<Option<WatcherHandle>>>,
) {
    controls.button.set_sensitive(false);
    controls.button.set_label(&strings::text(strings::SCANNING));
    controls
        .button
        .set_tooltip_text(Some(&strings::text(strings::SCANNING)));
    controls.progress.show(&ScanProgress::Discovering);

    let (progress_sender, progress_receiver) = async_channel::bounded::<ScanProgress>(1);
    let (result_sender, result_receiver) = async_channel::bounded(1);
    let (drained_sender, drained_receiver) = async_channel::bounded(1);

    // Cloned (not moved) here: the worker thread below consumes its own
    // copies, while the `glib::spawn_future_local` block further down still
    // needs `folder`/`db_path` afterward to (re)arm the watcher on exactly
    // the folder that was just scanned.
    let thread_folder = folder.clone();
    let thread_db_path = db_path.clone();
    let stale_receiver = progress_receiver.clone();
    std::thread::spawn(move || {
        let result = run_scan(&thread_db_path, &thread_folder, |progress| {
            publish_latest_progress(&progress_sender, &stale_receiver, progress);
        });
        drop(progress_sender);
        drop(stale_receiver);
        if let Err(error) = result_sender.send_blocking(result) {
            tracing::warn!(%error, "scan result dropped: UI receiver is gone");
        }
    });

    let progress_view = controls.progress.clone();
    glib::spawn_future_local(async move {
        while let Ok(progress) = progress_receiver.recv().await {
            progress_view.show(&progress);
        }
        let _ = drained_sender.try_send(());
    });

    glib::spawn_future_local(async move {
        let outcome = result_receiver.recv().await;
        let _ = drained_receiver.recv().await;
        finish_scan_ui(&controls);

        match outcome {
            Ok(Ok(report)) => {
                tracing::info!(?report, "scan complete");
                track_list.reload();
                sidebar.refresh("scan completed");
                start_or_restart_watcher(
                    &watcher_state,
                    &folder,
                    db_path,
                    Rc::downgrade(&track_list),
                    Rc::downgrade(&sidebar),
                );
                controls.completion.notify();
            }
            Ok(Err(error)) => {
                tracing::error!(%error, "scan failed");
                toasts::show(
                    &toast_overlay,
                    &format!("{}{error}", &strings::text(strings::SCAN_FAILED_PREFIX)),
                );
            }
            Err(error) => {
                tracing::error!(%error, "scan worker channel closed unexpectedly");
                toasts::show(
                    &toast_overlay,
                    &format!("{}{error}", &strings::text(strings::SCAN_FAILED_PREFIX)),
                );
            }
        }
    });
}

fn publish_latest_progress(
    sender: &async_channel::Sender<ScanProgress>,
    receiver: &async_channel::Receiver<ScanProgress>,
    progress: ScanProgress,
) {
    match sender.try_send(progress) {
        Ok(()) => {}
        Err(async_channel::TrySendError::Full(progress)) => {
            let _ = receiver.try_recv();
            if let Err(error) = sender.try_send(progress) {
                tracing::warn!(%error, "scan progress dropped: UI receiver is gone");
            }
        }
        Err(async_channel::TrySendError::Closed(_)) => {
            tracing::warn!("scan progress dropped: UI receiver is gone");
        }
    }
}

fn finish_scan_ui(controls: &ScanControls) {
    controls.progress.finish();
    controls.button.set_sensitive(true);
    controls
        .button
        .set_label(&strings::text(strings::SCAN_FOLDER));
    controls.button.set_tooltip_text(None);
}

/// Runs on the scan worker thread: opens and migrates its own `Connection`
/// over `db_path` (never the UI's `Rc<RefCell<Connection>>` — see the
/// module doc comment on `spawn_scan`), scans `folder` through it, then
/// persists `folder` as the library root (`library::settings::LIBRARY_ROOT_
/// KEY`) so the watcher knows what to watch on the next launch even before
/// this scan's result reaches the UI thread. A persistence failure is logged
/// but does not fail the scan itself — the scan's own result is what matters
/// most; the watcher simply won't auto-start next launch if this write
/// didn't stick.
fn run_scan(
    db_path: &std::path::Path,
    folder: &std::path::Path,
    on_progress: impl FnMut(ScanProgress),
) -> Result<ScanReport, ScanError> {
    let mut worker_conn = reprise_core::db::open(Some(db_path))?;
    reprise_core::db::migrate(&worker_conn)?;
    let report =
        library::scanner::scan_folder_with_progress(&mut worker_conn, folder, on_progress)?;
    if let Err(error) = settings::set_library_root(&worker_conn, &folder.to_string_lossy()) {
        tracing::error!(%error, "failed to persist library root after scan");
    }
    Ok(report)
}

/// (Re)starts the folder watcher on `root` (Stage 3 Task 8): builds a fresh
/// `async_channel`, starts `library::watcher::start` with a sender closure as
/// its `on_event` callback (called on the watcher's own background thread —
/// see that function's doc comment), stores the resulting handle in
/// `watcher_state` (dropping — and thereby stopping — any previous watcher,
/// via plain assignment; see `watcher_state`'s own doc comment in `build`),
/// and spawns a long-lived local future that drains the receiver for the
/// rest of the app's lifetime, reloading the track list and refreshing the
/// sidebar on every reconcile. `track_list`/`sidebar` are `Weak` (not strong
/// `Rc`s): this drain loop runs indefinitely and must never be the thing
/// keeping either alive past the window's own lifetime — the same reasoning
/// as every other long-lived cross-widget callback in this module (e.g.
/// `player.set_track_list_reload`).
pub(super) fn start_or_restart_watcher(
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
    root: &Path,
    db_path: PathBuf,
    track_list: std::rc::Weak<TrackList>,
    sidebar: std::rc::Weak<Sidebar>,
) {
    // Task 9 review fold-in: drop any previous watcher *before* starting the
    // new one, not after. The old code let a fresh `watcher::start` (which
    // arms its own OS-level watch) run first and only replaced `*watcher_
    // state.borrow_mut()` with the new handle afterward — the assignment's
    // right-hand side (the new handle) is fully constructed before the old
    // value is dropped, so for the brief window between `watcher::start`
    // returning and the assignment landing, two watchers were both alive and
    // watching (the old one not yet stopped, the new one already started).
    // `.take()` here drops (and thereby stops — see `WatcherHandle`'s `Drop`
    // impl) the previous watcher immediately, before `watcher::start` for the
    // new one is even called, so at most one watcher is ever alive at a time.
    watcher_state.borrow_mut().take();

    let (sender, receiver) = async_channel::unbounded::<watcher::WatchEvent>();

    let handle = watcher::start(root, db_path, move |event| {
        if let Err(error) = sender.send_blocking(event) {
            tracing::warn!(%error, "watcher event dropped: UI receiver is gone");
        }
    });
    match &handle {
        Some(_) => tracing::info!(root = %root.display(), "watcher started"),
        None => tracing::warn!(
            root = %root.display(),
            "watcher unavailable; continuing without live updates"
        ),
    }
    *watcher_state.borrow_mut() = handle;

    glib::spawn_future_local(async move {
        while let Ok(event) = receiver.recv().await {
            tracing::info!(
                added = event.report.added,
                updated = event.report.updated,
                moved = event.report.moved,
                errors = event.report.errors,
                vanished = event.vanished,
                "watcher: reconciling UI after live library update"
            );
            match track_list.upgrade() {
                Some(track_list) => track_list.reload(),
                None => tracing::warn!("watcher: track list reload skipped: track list is gone"),
            }
            match sidebar.upgrade() {
                Some(sidebar) => sidebar.refresh("watcher reconcile"),
                None => tracing::warn!("watcher: sidebar refresh skipped: sidebar is gone"),
            }
        }
        tracing::debug!("watcher: event receiver closed; exiting UI drain loop");
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::rc::Rc;

    use reprise_core::library::scanner::ScanProgress;

    use super::{publish_latest_progress, ScanCompletion};

    #[test]
    fn scan_completion_callback_runs_without_holding_its_refcell_borrow() {
        let completion = ScanCompletion::default();
        let calls = Rc::new(Cell::new(0));
        let calls_for_callback = calls.clone();
        let reentrant_completion = completion.clone();
        completion.set(move || {
            calls_for_callback.set(calls_for_callback.get() + 1);
            reentrant_completion.set(|| {});
        });

        completion.notify();

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn progress_channel_keeps_only_the_latest_pending_update() {
        let (sender, receiver) = async_channel::bounded(1);
        publish_latest_progress(&sender, &receiver, ScanProgress::Discovering);
        publish_latest_progress(
            &sender,
            &receiver,
            ScanProgress::Scanning {
                processed: 2,
                total: 9,
                current_path: PathBuf::from("second.flac"),
            },
        );

        let progress = receiver.try_recv().expect("latest progress event");
        let ScanProgress::Scanning {
            processed,
            total,
            current_path,
        } = progress
        else {
            panic!("expected the newest scanning event");
        };
        assert_eq!(processed, 2);
        assert_eq!(total, 9);
        assert_eq!(current_path, PathBuf::from("second.flac"));
        assert!(receiver.is_empty());
    }
}
