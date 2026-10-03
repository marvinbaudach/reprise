use std::rc::Rc;

use super::radio_view::{RadioView, Shared};

pub(super) fn wire_module_off_action(shared: &Rc<Shared>) {
    let weak = Rc::downgrade(shared);
    shared.module_off_state.connect_add(move || {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let callback = shared.on_open_preferences.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    });
}

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
