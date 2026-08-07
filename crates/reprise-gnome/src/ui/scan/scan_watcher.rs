//! Folder-watcher lifecycle and GTK-thread event reconciliation.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use gtk4::glib;
use reprise_core::library::watcher::{self, WatcherHandle};

use super::scan_controls::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;

/// Whether a watcher reconcile can have changed which rows the table shows.
///
/// The watcher fires on filesystem activity, not on library change: a scan that
/// found nothing new still arrives here, and every one of those used to cost a
/// full model swap. Measured against a copy of the real library (1,903 tracks),
/// a startup produced two such reconciles at **~360 ms each** — both reporting
/// `added=0 updated=0 moved=0 vanished=0`, both rebuilding the table into
/// exactly what was already on screen.
///
/// `errors` and `skipped_unchanged` deliberately do not count. An unreadable
/// file leaves the catalog untouched and surfaces in the Issues view, which the
/// sidebar refresh below covers; a skipped file is by definition unchanged.
fn reconcile_changes_rows(event: &watcher::WatchEvent) -> bool {
    event.report.added > 0
        || event.report.updated > 0
        || event.report.moved > 0
        || event.report.excluded > 0
        || event.vanished > 0
        || !event.auto_cleaned_ids.is_empty()
}

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
            // Read before the match below moves `root_unavailable` out.
            let changes_rows = reconcile_changes_rows(&event);
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
                    } else if changes_rows {
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

#[cfg(test)]
mod tests {
    use reprise_core::library::watcher::WatchEvent;

    use super::reconcile_changes_rows;

    fn quiet_event() -> WatchEvent {
        WatchEvent {
            report: Default::default(),
            vanished: 0,
            root_unavailable: None,
            auto_cleaned_ids: Vec::new(),
        }
    }

    #[test]
    fn a_reconcile_that_found_nothing_does_not_rebuild_the_table() {
        assert!(!reconcile_changes_rows(&quiet_event()));
    }

    #[test]
    fn errors_and_skips_alone_do_not_rebuild_the_table() {
        let mut event = quiet_event();
        event.report.errors = 7;
        event.report.skipped_unchanged = 1903;
        assert!(!reconcile_changes_rows(&event));
    }

    #[test]
    fn every_kind_of_row_change_rebuilds_the_table() {
        for apply in [
            (|e: &mut WatchEvent| e.report.added = 1) as fn(&mut WatchEvent),
            |e: &mut WatchEvent| e.report.updated = 1,
            |e: &mut WatchEvent| e.report.moved = 1,
            |e: &mut WatchEvent| e.report.excluded = 1,
            |e: &mut WatchEvent| e.vanished = 1,
            |e: &mut WatchEvent| e.auto_cleaned_ids = vec![42],
        ] {
            let mut event = quiet_event();
            apply(&mut event);
            assert!(
                reconcile_changes_rows(&event),
                "a change of this kind must still reach the table"
            );
        }
    }
}
