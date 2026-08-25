//! Test-only handle publication for the fully composed online-module seam.

use std::cell::RefCell;
use std::rc::Rc;

pub(super) struct OnlineModuleTestHandles {
    pub(super) preferences: Rc<crate::ui::preferences::PreferencesContext>,
    pub(super) cover_batch: Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    pub(super) lyrics_batch: Rc<crate::ui::lyrics_batch::LyricsBatch>,
    stats: super::content_stack::DeferredPage<crate::ui::stats_view::StatsView>,
    podcasts: super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube: super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio: super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
}

impl OnlineModuleTestHandles {
    pub(super) fn radio(&self) -> crate::ui::radio::RadioTestHandle {
        crate::ui::radio::RadioTestHandle::new(&self.radio.materialize())
    }

    pub(super) fn materialize_artwork_surfaces(&self) {
        self.stats.materialize();
        self.podcasts.materialize();
        self.youtube.materialize();
        self.radio.materialize();
    }
}

thread_local! {
    static HANDLES: RefCell<Option<OnlineModuleTestHandles>> = const { RefCell::new(None) };
}

pub(super) fn publish(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    lyrics_batch: &Rc<crate::ui::lyrics_batch::LyricsBatch>,
    stats: &super::content_stack::DeferredPage<crate::ui::stats_view::StatsView>,
    podcasts: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    youtube: &super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    radio: &super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
) {
    HANDLES.with(|handles| {
        handles.replace(Some(OnlineModuleTestHandles {
            preferences: preferences.clone(),
            cover_batch: cover_batch.clone(),
            lyrics_batch: lyrics_batch.clone(),
            stats: stats.clone(),
            podcasts: podcasts.clone(),
            youtube: youtube.clone(),
            radio: radio.clone(),
        }));
    });
}

pub(super) fn take() -> Option<OnlineModuleTestHandles> {
    HANDLES.with(|handles| handles.borrow_mut().take())
}
