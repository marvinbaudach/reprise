use std::rc::Rc;

use super::radio_view::RadioView;

impl RadioView {
    /// `RAD-5`: forwards to the Add Station dialog's "Near you" hand-off.
    pub(in crate::ui) fn set_on_location_settings(&self, callback: impl Fn() + 'static) {
        if let Some(dialog) = self.shared.add_dialog.borrow().as_ref() {
            dialog.set_on_location_settings(callback);
        }
    }

    pub(in crate::ui) fn set_on_open_preferences(&self, callback: impl Fn() + 'static) {
        self.shared
            .on_open_preferences
            .replace(Some(Rc::new(callback)));
    }
}
