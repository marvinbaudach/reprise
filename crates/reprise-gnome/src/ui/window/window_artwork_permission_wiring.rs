//! Artwork permission effects at the window composition boundary.

use std::rc::Rc;

pub(super) fn wire(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    stats: &super::super::content_stack::DeferredPage<crate::ui::stats_view::StatsView>,
    podcasts: &super::super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube: &super::super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio: &super::super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
) {
    let cover_batch = Rc::downgrade(cover_batch);
    let stats = stats.clone();
    let podcasts = podcasts.clone();
    let youtube = youtube.clone();
    let radio = radio.clone();
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
        stats.if_materialized(|view| view.refresh_visible_artwork());
        podcasts.if_materialized(|view| view.refresh_visible_artwork());
        youtube.if_materialized(|view| view.refresh_visible_artwork());
        radio.if_materialized(|view| view.refresh_visible_artwork());
    });
}
