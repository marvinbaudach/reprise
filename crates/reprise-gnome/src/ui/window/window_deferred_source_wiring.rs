//! External wiring that must follow deferred source-page materialization.

use std::rc::Rc;

use libadwaita as adw;

use super::super::content_stack::DeferredPage;

#[allow(clippy::too_many_arguments)]
pub(super) fn install(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    toast_overlay: &adw::ToastOverlay,
    stats: &DeferredPage<crate::ui::stats_view::StatsView>,
    concerts: &DeferredPage<crate::ui::concerts::ConcertsView>,
    releases: &Rc<crate::ui::releases::ReleasesView>,
    podcasts: &DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube: &DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio: &DeferredPage<crate::ui::radio::RadioView>,
) {
    super::super::source_connectivity::wire(
        concerts,
        releases,
        podcasts,
        youtube,
        radio,
        preferences,
    );
    releases.set_toast_overlay(toast_overlay);
    super::super::startup_report::mark("source_connectivity::wire");
    super::artwork_permission_wiring::wire(
        preferences,
        cover_batch,
        stats,
        podcasts,
        youtube,
        radio,
    );

    // These callbacks used to be installed immediately after eager
    // construction. Register them with the page so they are present before a
    // synchronous navigation call returns with the page visible.
    for page in [podcasts, youtube] {
        let preferences = Rc::downgrade(preferences);
        page.on_materialized(move |view| {
            view.set_on_open_preferences(move || {
                if let Some(preferences) = preferences.upgrade() {
                    preferences.present_online_sources();
                }
            });
        });
    }
    {
        let preferences = Rc::downgrade(preferences);
        youtube.on_materialized(move |view| {
            view.set_on_open_youtube_preferences(move || {
                if let Some(preferences) = preferences.upgrade() {
                    preferences.present_plugins(&["youtube"]);
                }
            });
        });
    }
    let preferences = Rc::downgrade(preferences);
    concerts.on_materialized(move |view| {
        view.set_on_open_preferences(move || {
            if let Some(preferences) = preferences.upgrade() {
                preferences.present_plugins(&["concerts"]);
            }
        });
    });
}
