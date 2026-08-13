use gtk4::prelude::*;

#[cfg(test)]
use std::cell::Cell;

use super::RadioView;

#[cfg(test)]
thread_local! {
    static ARTWORK_REFRESH_REQUESTS: Cell<u64> = const { Cell::new(0) };
}

impl RadioView {
    /// Rebinds only visible station artwork, without a model reset or query.
    pub(in crate::ui) fn refresh_visible_artwork(&self) {
        #[cfg(test)]
        ARTWORK_REFRESH_REQUESTS.with(|count| count.set(count.get() + 1));
        if self.shared.root.is_mapped() {
            self.shared.artwork_cells.reapply();
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn artwork_refresh_requests_for_test() -> u64 {
        ARTWORK_REFRESH_REQUESTS.with(Cell::get)
    }
}
