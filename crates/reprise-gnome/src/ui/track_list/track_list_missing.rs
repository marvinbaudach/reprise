//! Missing-files integration seams kept out of the main list orchestrator.

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;

use super::reload;
use super::TrackList;

impl TrackList {
    /// Adds the first-scan indicator under the shared empty/status page.
    pub fn set_empty_scan_widget(&self, widget: &impl gtk4::prelude::IsA<gtk4::Widget>) {
        self.shared.empty_page_actions.append(widget);
    }

    pub(in crate::ui) fn set_on_scan_queue_purge_ids(
        &self,
        callback: impl Fn() -> Vec<i64> + 'static,
    ) {
        *self.shared.on_scan_queue_purge_ids.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn notify_library_purged(&self, ids: &[i64]) {
        let callback = self.shared.on_library_mutated.borrow().clone();
        if let Some(callback) = callback {
            callback(ids);
        }
    }

    /// Combines auto-clean removals with freshly non-retained queue ids and
    /// sends one silent notification through the existing hard-purge seam.
    pub(in crate::ui) fn notify_scan_postprocessed(&self, auto_cleaned_ids: &[i64]) {
        let provider = self.shared.on_scan_queue_purge_ids.borrow().clone();
        let mut purge_ids = auto_cleaned_ids.to_vec();
        if let Some(provider) = provider {
            purge_ids.extend(provider());
        }
        purge_ids.sort_unstable();
        purge_ids.dedup();
        if !purge_ids.is_empty() {
            self.notify_library_purged(&purge_ids);
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
