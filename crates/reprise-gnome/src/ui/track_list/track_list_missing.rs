//! Missing-files integration seams kept out of the main list orchestrator.

use std::path::PathBuf;

use gtk4::prelude::*;

use super::reload;
use super::TrackList;

impl TrackList {
    /// Adds the first-scan indicator under the shared empty/status page.
    pub fn set_empty_scan_widget(&self, widget: &impl gtk4::prelude::IsA<gtk4::Widget>) {
        self.shared.empty_page_actions.append(widget);
    }

    pub(in crate::ui) fn notify_library_purged(&self, ids: &[i64]) {
        let callback = self.shared.on_library_mutated.borrow().clone();
        if let Some(callback) = callback {
            callback(ids);
        }
    }

    pub(in crate::ui) fn set_library_root_unavailable(&self, root: Option<PathBuf>) {
        self.shared.library_root_unavailable.set(root.is_some());
        *self.shared.unavailable_library_root.borrow_mut() = root;
        reload(&self.shared);
    }

    pub(in crate::ui) fn set_missing_relink_db_path(&self, db_path: PathBuf) {
        self.shared.missing_files_view.set_db_path(db_path);
    }

    pub(in crate::ui) fn missing_relink_progress_widget(&self) -> &gtk4::Revealer {
        self.shared.missing_files_view.relink_progress_widget()
    }

    pub(in crate::ui) fn set_on_missing_relink_progress_activate(
        &self,
        callback: impl Fn(reprise_core::view_source::ViewSource) + 'static,
    ) {
        self.shared
            .missing_files_view
            .set_on_relink_progress_activate(callback);
    }
}
