use super::*;

impl TrackListModel {
    pub(in crate::ui) fn metadata_generation(&self) -> u64 {
        self.imp().metadata_generation.get()
    }

    /// Drops cached SQL windows covering a metadata-only row range without
    /// announcing a structural `GListModel` change.
    pub(in crate::ui) fn invalidate_cached_metadata(&self, position: u32, len: u32) {
        let end = position.saturating_add(len);
        self.imp()
            .state
            .borrow_mut()
            .cache
            .retain(|start, _| start.saturating_add(WINDOW_SIZE) <= position || *start >= end);
        self.imp()
            .metadata_generation
            .set(self.metadata_generation().wrapping_add(1));
    }
}
