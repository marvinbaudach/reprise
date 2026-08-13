use gtk4::prelude::*;

use super::RadioView;

impl RadioView {
    /// Rebinds only visible station artwork, without a model reset or query.
    pub(in crate::ui) fn refresh_visible_artwork(&self) {
        if self.shared.root.is_mapped() {
            self.shared.artwork_cells.reapply();
        }
    }
}
