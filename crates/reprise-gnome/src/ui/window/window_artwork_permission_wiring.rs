//! Artwork permission effects at the window composition boundary.

use std::rc::Rc;

pub(super) fn wire(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    stats: &crate::ui::stats_view::StatsView,
    podcasts: &Rc<crate::ui::podcasts::PodcastsView>,
    youtube: &Rc<crate::ui::podcasts::PodcastsView>,
    radio: &Rc<crate::ui::radio::RadioView>,
) {
    let cover_batch = Rc::downgrade(cover_batch);
    let stats = stats.clone();
    let podcasts = Rc::downgrade(podcasts);
    let youtube = Rc::downgrade(youtube);
    let radio = Rc::downgrade(radio);
    preferences.set_on_artwork_permission_changed(move |enabled| {
        if !enabled {
            if let Some(cover_batch) = cover_batch.upgrade() {
                cover_batch.cancel();
            }
            return;
        }

        if let Some(cover_batch) = cover_batch.upgrade() {
            cover_batch.start_user_triggered();
        }
        stats.refresh_visible_artwork();
        if let Some(view) = podcasts.upgrade() {
            view.refresh_visible_artwork();
        }
        if let Some(view) = youtube.upgrade() {
            view.refresh_visible_artwork();
        }
        if let Some(view) = radio.upgrade() {
            view.refresh_visible_artwork();
        }
    });
}
