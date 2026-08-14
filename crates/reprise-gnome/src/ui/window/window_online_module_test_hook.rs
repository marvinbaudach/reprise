//! Test-only handle publication for the fully composed online-module seam.

use std::cell::RefCell;
use std::rc::Rc;

pub(super) struct OnlineModuleTestHandles {
    pub(super) preferences: Rc<crate::ui::preferences::PreferencesContext>,
    pub(super) radio: crate::ui::radio::RadioTestHandle,
    pub(super) cover_batch: Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    pub(super) lyrics_batch: Rc<crate::ui::lyrics_batch::LyricsBatch>,
}

thread_local! {
    static HANDLES: RefCell<Option<OnlineModuleTestHandles>> = const { RefCell::new(None) };
}

pub(super) fn publish(
    preferences: &Rc<crate::ui::preferences::PreferencesContext>,
    cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    lyrics_batch: &Rc<crate::ui::lyrics_batch::LyricsBatch>,
) {
    HANDLES.with(|handles| {
        handles.replace(Some(OnlineModuleTestHandles {
            preferences: preferences.clone(),
            radio: crate::ui::radio::test_handle(),
            cover_batch: cover_batch.clone(),
            lyrics_batch: lyrics_batch.clone(),
        }));
    });
}

pub(super) fn take() -> Option<OnlineModuleTestHandles> {
    HANDLES.with(|handles| handles.borrow_mut().take())
}
