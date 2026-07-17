//! Folder-watcher lifecycle and GTK-thread event reconciliation.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use gtk4::glib;
use reprise_core::library::watcher::{self, WatcherHandle};

use super::scan_controls::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;

pub(in crate::ui) fn start_or_restart_watcher(
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
    root: &Path,
    db_path: PathBuf,
    controls: ScanControls,
    track_list: Weak<TrackList>,
    sidebar: Weak<Sidebar>,
) {
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
                auto_cleaned = event.auto_cleaned_ids.len(),
                "watcher: reconciling UI after live library update"
            );
            match (track_list.upgrade(), event.root_unavailable) {
                (Some(track_list), Some(root)) => {
                    controls.set_library_root_unavailable(true);
                    controls.show_root_unavailable(&root);
                    track_list.set_library_root_unavailable(Some(root));
                }
                (Some(track_list), None) => {
                    if controls.library_root_unavailable() {
                        controls.finish_progress();
                        controls.set_library_root_unavailable(false);
                        track_list.set_library_root_unavailable(None);
                    } else {
                        track_list.reload();
                    }
                    track_list.notify_scan_postprocessed(&event.auto_cleaned_ids);
                }
                (None, _) => {
                    tracing::warn!("watcher: track list reload skipped: track list is gone");
                }
            }
            match sidebar.upgrade() {
                Some(sidebar) => sidebar.refresh("watcher reconcile"),
                None => tracing::warn!("watcher: sidebar refresh skipped: sidebar is gone"),
            }
        }
        tracing::debug!("watcher: event receiver closed; exiting UI drain loop");
    });
}
