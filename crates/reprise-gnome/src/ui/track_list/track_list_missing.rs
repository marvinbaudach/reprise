//! Missing-files integration seams kept out of the main list orchestrator.

use std::path::PathBuf;

use super::TrackList;

impl TrackList {
    pub(in crate::ui) fn set_missing_relink_db_path(&self, db_path: PathBuf) {
        self.shared.missing_files_view.set_db_path(db_path);
    }

    pub(in crate::ui) fn missing_relink_progress_widget(&self) -> &gtk4::Revealer {
        self.shared.missing_files_view.relink_progress_widget()
    }

    pub(in crate::ui) fn set_on_missing_relink_progress_activate(
        &self,
        callback: impl Fn() + 'static,
    ) {
        self.shared
            .missing_files_view
            .set_on_relink_progress_activate(callback);
    }
}
