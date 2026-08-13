//! Artwork permission effects at the window composition boundary.

use std::rc::Rc;

pub(super) fn wire(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
) {
    let cover_batch = Rc::downgrade(cover_batch);
    preferences.set_on_artwork_permission_changed(move |enabled| {
        let Some(cover_batch) = cover_batch.upgrade() else {
            return;
        };
        if !enabled {
            cover_batch.cancel();
            return;
        }

        cover_batch.start_user_triggered();
    });
}
