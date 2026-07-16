//! The library-scan flow extracted from `ui::window` (Queue-C refactor Task
//! 3): the "Scan folder…" button wiring, the "Rescan library" trigger, the
//! background scan worker (`spawn_scan`/`run_scan`), the headless
//! `REPRISE_SMOKE_RESCAN` hook, and the folder-watcher (re)start logic.
//! `window::build` calls into here via `super::scan_flow::…`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library;
use reprise_core::library::scanner::{ScanError, ScanProgress, ScanReport};
use reprise_core::library::settings;
use reprise_core::library::watcher::{self, WatcherHandle};

use super::scan_progress::{
    EmptyScanIndicator, ScanProgressView, WeakEmptyScanIndicator, WeakScanProgressView,
};
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

type OnScanStateChanged = Rc<dyn Fn(bool)>;

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
    // The main view is retained for the window lifetime. Foreground views are
    // weak so closing Preferences cannot leak its widget tree; current_progress
    // lets a newly opened Preferences window catch up mid-scan.
    button: gtk4::Button,
    primary_progress: ScanProgressView,
    foreground_progress: Rc<RefCell<Vec<WeakScanProgressView>>>,
    current_progress: Rc<RefCell<Option<ScanProgress>>>,
    completion: ScanCompletion,
    cancel_token: Arc<AtomicBool>,
    on_scan_state_changed: Rc<RefCell<Option<OnScanStateChanged>>>,
    /// Weak reference to the indicator embedded in the empty-library status
    /// page. `None` until `set_empty_indicator` is called from `window.rs`
    /// after both `track_list` and `scan_controls` exist.
    empty_indicator: Rc<RefCell<Option<WeakEmptyScanIndicator>>>,
    /// Weak reference to the sidebar toggle button. When set, the tooltip is
    /// updated to reflect scan progress so the user can see the status while
    /// the sidebar is collapsed. `None` until `set_sidebar_toggle` is called.
    sidebar_toggle: Rc<RefCell<Option<glib::WeakRef<gtk4::ToggleButton>>>>,
}

impl ScanControls {
    pub(super) fn new(button: &gtk4::Button, progress: &ScanProgressView) -> Self {
        Self {
            button: button.clone(),
            primary_progress: progress.clone(),
            foreground_progress: Rc::new(RefCell::new(Vec::new())),
            current_progress: Rc::new(RefCell::new(None)),
            completion: ScanCompletion::default(),
            cancel_token: Arc::new(AtomicBool::new(false)),
            on_scan_state_changed: Rc::new(RefCell::new(None)),
            empty_indicator: Rc::new(RefCell::new(None)),
            sidebar_toggle: Rc::new(RefCell::new(None)),
        }
    }

    /// Registers the lightweight scan indicator embedded in the empty-library
    /// status page. Called from `window.rs` after both `track_list` and
    /// `scan_controls` exist. Stored as a `Weak` reference so `ScanControls`
    /// can never keep the indicator's widget tree alive past the window's own
    /// lifetime — same pattern as `foreground_progress`.
    pub(super) fn set_empty_indicator(&self, indicator: &EmptyScanIndicator) {
        *self.empty_indicator.borrow_mut() = Some(indicator.downgrade());
    }

    /// Registers the sidebar toggle button so its tooltip can be updated to
    /// reflect scan progress while the sidebar is collapsed. Called from
    /// `window.rs` after both `scan_controls` and `sidebar_toggle` exist.
    /// Stored as a `Weak` reference so `ScanControls` cannot keep the button
    /// alive past the window's lifetime.
    pub(super) fn set_sidebar_toggle(&self, button: &gtk4::ToggleButton) {
        let weak = glib::WeakRef::new();
        weak.set(Some(button));
        *self.sidebar_toggle.borrow_mut() = Some(weak);
    }

    pub(super) fn is_scanning(&self) -> bool {
        !self.button.is_sensitive()
    }

    pub(super) fn request_cancel(&self) {
        self.cancel_token.store(true, Ordering::Relaxed);
    }

    pub(super) fn reset_cancel(&self) {
        self.cancel_token.store(false, Ordering::Relaxed);
    }

    pub(super) fn is_cancel_requested(&self) -> bool {
        self.cancel_token.load(Ordering::Relaxed)
    }

    pub(super) fn set_on_scan_state_changed(&self, callback: impl Fn(bool) + 'static) {
        *self.on_scan_state_changed.borrow_mut() = Some(Rc::new(callback));
    }

    fn notify_scan_state(&self) {
        let callback = self.on_scan_state_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(self.is_scanning());
        }
    }

    pub(super) fn attach_progress_view(&self, progress: &ScanProgressView) {
        self.foreground_progress
            .borrow_mut()
            .push(progress.downgrade());
        let current = self.current_progress.borrow().clone();
        if let Some(current) = current {
            progress.show(&current);
        }
    }

    fn live_progress_views(&self) -> Vec<ScanProgressView> {
        let foreground = {
            let mut weak_views = self.foreground_progress.borrow_mut();
            let mut live = Vec::with_capacity(weak_views.len());
            weak_views.retain(|weak| match weak.upgrade() {
                Some(view) => {
                    live.push(view);
                    true
                }
                None => false,
            });
            live
        };
        std::iter::once(self.primary_progress.clone())
            .chain(foreground)
            .collect()
    }

    fn show_progress(&self, progress: &ScanProgress) {
        let phase_changed = {
            let current = self.current_progress.borrow();
            !matches!(
                (current.as_ref(), progress),
                (Some(ScanProgress::Discovering), ScanProgress::Discovering)
                    | (
                        Some(ScanProgress::Scanning { .. }),
                        ScanProgress::Scanning { .. }
                    )
                    | (
                        Some(ScanProgress::Fetching { .. }),
                        ScanProgress::Fetching { .. }
                    )
            )
        };
        *self.current_progress.borrow_mut() = Some(progress.clone());
        match progress {
            ScanProgress::Discovering => {
                if phase_changed {
                    tracing::info!("scan progress: discovering");
                }
            }
            ScanProgress::Scanning {
                processed,
                total,
                current_path,
            } => {
                if phase_changed {
                    tracing::info!(
                        processed,
                        total,
                        file = %current_path.display(),
                        "scan progress: scanning"
                    );
                } else {
                    tracing::debug!(
                        processed,
                        total,
                        file = %current_path.display(),
                        "scan progress: scanning"
                    );
                }
            }
            ScanProgress::Fetching { done, total } => {
                if phase_changed {
                    tracing::info!(done, total, "scan progress: fetching");
                } else {
                    tracing::debug!(done, total, "scan progress: fetching");
                }
            }
        }
        let views = self.live_progress_views();
        for view in views {
            view.show(progress);
        }
        if let Some(indicator) = self
            .empty_indicator
            .borrow()
            .as_ref()
            .and_then(super::scan_progress::WeakEmptyScanIndicator::upgrade)
        {
            indicator.show(progress);
        }
        if let Some(button) = self
            .sidebar_toggle
            .borrow()
            .as_ref()
            .and_then(libadwaita::glib::WeakRef::upgrade)
        {
            let tooltip = match progress {
                ScanProgress::Discovering => Some(strings::scan_tooltip_discovering()),
                ScanProgress::Scanning {
                    processed, total, ..
                } => {
                    let pct = if *total > 0 {
                        (*processed as f64 / *total as f64 * 100.0).round() as u32
                    } else {
                        0
                    };
                    Some(strings::scan_tooltip_progress(pct))
                }
                ScanProgress::Fetching { done, total } => {
                    let pct = if *total > 0 {
                        (*done as f64 / *total as f64 * 100.0).round() as u32
                    } else {
                        0
                    };
                    Some(strings::scan_tooltip_progress(pct))
                }
            };
            button.set_tooltip_text(tooltip.as_deref());
        }
    }

    fn finish_progress(&self) {
        self.current_progress.borrow_mut().take();
        let views = self.live_progress_views();
        for view in views {
            view.finish();
        }
        if let Some(indicator) = self
            .empty_indicator
            .borrow()
            .as_ref()
            .and_then(super::scan_progress::WeakEmptyScanIndicator::upgrade)
        {
            indicator.finish();
        }
        if let Some(button) = self
            .sidebar_toggle
            .borrow()
            .as_ref()
            .and_then(libadwaita::glib::WeakRef::upgrade)
        {
            button.set_tooltip_text(Some(&strings::text(strings::SIDEBAR_TOGGLE)));
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

/// Wires the shared "Scan folder…" trigger used by Preferences and initial
/// setup: activation opens a portal-friendly `gtk::FileDialog` folder picker;
/// a chosen folder starts a background scan (see `spawn_scan`). Dismissing the
/// dialog without choosing a folder is a normal, expected outcome (not an
/// error) — logged at debug and otherwise ignored.
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
    if controls.is_scanning() {
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
    controls.reset_cancel();
    controls.button.set_sensitive(false);
    controls.notify_scan_state();
    controls.button.set_label(&strings::text(strings::SCANNING));
    controls
        .button
        .set_tooltip_text(Some(&strings::text(strings::SCANNING)));
    controls.show_progress(&ScanProgress::Discovering);

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

    let progress_controls = controls.clone();
    glib::spawn_future_local(async move {
        while let Ok(progress) = progress_receiver.recv().await {
            progress_controls.show_progress(&progress);
        }
        let _ = drained_sender.try_send(());
    });

    glib::spawn_future_local(async move {
        let outcome = result_receiver.recv().await;
        let _ = drained_receiver.recv().await;
        finish_scan_ui(&controls);

        match outcome {
            Ok(Ok(report)) => {
                if controls.is_cancel_requested() {
                    tracing::info!("scan cancelled by user; keeping already-imported tracks");
                    track_list.reload();
                    sidebar.refresh("scan cancelled");
                } else {
                    tracing::info!(?report, "scan complete");
                    let result = report.to_scan_result();
                    toasts::show(
                        &toast_overlay,
                        &strings::scan_complete_toast(result.new_tracks, result.failed),
                    );
                    track_list.reload();
                    sidebar.refresh("scan completed");
                    start_or_restart_watcher(
                        &watcher_state,
                        &folder,
                        db_path,
                        Rc::downgrade(&track_list),
                        Rc::downgrade(&sidebar),
                    );
                }
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
    controls.finish_progress();
    controls.button.set_sensitive(true);
    controls.notify_scan_state();
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
    analyze_waveforms(db_path);
    Ok(report)
}

/// How many gst-launch subprocesses to run in parallel.
const WAVEFORM_WORKERS: usize = 4;

/// Analyzes waveform peaks for all tracks that don't have them yet.
/// Parallelizes across `WAVEFORM_WORKERS` threads, each with its own DB
/// connection. Uses a shared work queue (atomic index into the track list).
fn analyze_waveforms(db_path: &std::path::Path) {
    let conn = match reprise_core::db::open(Some(db_path)) {
        Ok(c) => c,
        Err(_) => return,
    };
    let tracks: Vec<(i64, String)> = match conn
        .prepare("SELECT id, path FROM tracks WHERE waveform_peaks IS NULL AND missing = 0")
    {
        Ok(mut stmt) => stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok()
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default(),
        Err(_) => return,
    };
    drop(conn);

    if tracks.is_empty() {
        tracing::info!("waveform backfill: all tracks already analyzed");
        return;
    }
    let total = tracks.len();
    tracing::info!(
        total,
        workers = WAVEFORM_WORKERS,
        "waveform backfill: starting"
    );

    let cursor = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..WAVEFORM_WORKERS {
            scope.spawn(|| {
                let Ok(worker_conn) = reprise_core::db::open(Some(db_path)) else {
                    return;
                };
                loop {
                    let idx = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if idx >= total {
                        break;
                    }
                    let (track_id, ref path_str) = tracks[idx];
                    let path = std::path::Path::new(path_str);
                    match crate::ui::waveform_peaks::extract_peaks(
                        path,
                        crate::ui::waveform_peaks::STORED_PEAK_COUNT,
                    ) {
                        Ok(peaks) => {
                            if reprise_core::db::set_waveform_peaks(&worker_conn, track_id, &peaks)
                                .is_ok()
                            {
                                done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            } else {
                                failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        Err(_) => {
                            failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    let progress = done.load(std::sync::atomic::Ordering::Relaxed)
                        + failed.load(std::sync::atomic::Ordering::Relaxed);
                    if progress.is_multiple_of(100) {
                        tracing::info!(
                            done = done.load(std::sync::atomic::Ordering::Relaxed),
                            failed = failed.load(std::sync::atomic::Ordering::Relaxed),
                            total,
                            "waveform backfill progress"
                        );
                    }
                }
            });
        }
    });

    tracing::info!(
        done = done.load(std::sync::atomic::Ordering::Relaxed),
        failed = failed.load(std::sync::atomic::Ordering::Relaxed),
        total,
        "waveform backfill complete"
    );
}

/// Spawns a background thread that analyzes waveform peaks for all tracks
/// without peaks in the DB. Called once at app startup so existing libraries
/// get peaks without requiring a manual rescan.
pub(super) fn spawn_waveform_backfill(db_path: std::path::PathBuf) {
    std::thread::Builder::new()
        .name("reprise-waveform-backfill".to_string())
        .spawn(move || {
            analyze_waveforms(&db_path);
        })
        .ok();
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

    use super::{publish_latest_progress, ScanCompletion, ScanControls};
    use crate::ui::scan_progress::ScanProgressView;

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn foreground_progress_view_replays_and_tracks_the_active_scan() {
        if gtk4::init().is_err() {
            return;
        }
        let button = gtk4::Button::new();
        let main = ScanProgressView::new();
        let controls = ScanControls::new(&button, &main);
        controls.show_progress(&ScanProgress::Scanning {
            processed: 2,
            total: 5,
            current_path: PathBuf::from("song.flac"),
        });
        let foreground = ScanProgressView::new();

        controls.attach_progress_view(&foreground);

        assert!(main.widget().reveals_child());
        assert!(foreground.widget().reveals_child());
        controls.finish_progress();
        assert!(!main.widget().reveals_child());
        assert!(!foreground.widget().reveals_child());

        drop(foreground);
        controls.show_progress(&ScanProgress::Discovering);
        assert!(main.widget().reveals_child());
        assert!(controls.foreground_progress.borrow().is_empty());
    }
}
