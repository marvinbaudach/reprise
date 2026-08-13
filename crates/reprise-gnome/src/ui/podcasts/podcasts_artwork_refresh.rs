//! Targeted source-artwork refresh for retained Podcast and YouTube rows.

#[cfg(test)]
use std::cell::Cell;

use gtk4::prelude::*;

use super::PodcastsView;

#[cfg(test)]
thread_local! {
    static ARTWORK_REFRESH_REQUESTS: Cell<u64> = const { Cell::new(0) };
}

impl PodcastsView {
    /// Rebinds the rows already held by a visible source page so their image
    /// requests see a newly opened Artwork gate. Hidden pages stay cold.
    pub(in crate::ui) fn refresh_visible_artwork(&self) {
        #[cfg(test)]
        ARTWORK_REFRESH_REQUESTS.with(|count| count.set(count.get() + 1));
        if !self.root.is_mapped() {
            return;
        }
        let images_allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::ARTWORK_MODULE,
        )
        .unwrap_or(false);
        if self.youtube_detail.is_active() {
            self.youtube_detail.refresh_visible_artwork(images_allowed);
            return;
        }
        let rebinds = self.artwork_rebinds.borrow().clone();
        for rebind in rebinds {
            rebind(images_allowed);
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn artwork_refresh_requests_for_test() -> u64 {
        ARTWORK_REFRESH_REQUESTS.with(Cell::get)
    }
}
