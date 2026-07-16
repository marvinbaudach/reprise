//! Folder selection, explicit rescan, and smoke orchestration for library scans.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::settings;
use reprise_core::library::watcher::WatcherHandle;
use rusqlite::Connection;

pub(in crate::ui) use super::scan_controls::ScanControls;
#[cfg(test)]
pub(in crate::ui) use super::scan_controls::{ScanCancellation, ScanCompletion};
pub(in crate::ui) use super::scan_watcher::start_or_restart_watcher;
pub(in crate::ui) use super::scan_waveform_analysis::spawn_waveform_backfill;
#[cfg(test)]
pub(in crate::ui) use super::scan_worker::publish_latest_progress;
use super::scan_worker::spawn_scan;
use super::sidebar::Sidebar;
use super::strings;
use super::toasts;
use super::track_list::TrackList;

const SMOKE_RESCAN_ENV_VAR: &str = "REPRISE_SMOKE_RESCAN";

pub(in crate::ui) fn arm_smoke_rescan(
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

pub(in crate::ui) fn wire_scan_button(
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

pub(in crate::ui) fn trigger_rescan_of_library_root(
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

    let root = settings::get_library_root(&conn.borrow());
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

#[cfg(test)]
#[path = "scan_flow_tests.rs"]
mod tests;
