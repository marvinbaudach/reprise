//! Test-only access to the composed Add Station dialog.

use std::rc::Rc;

use super::radio_view::{RadioView, Shared};

pub(in crate::ui) struct RadioTestHandle {
    shared: Rc<Shared>,
}

impl RadioTestHandle {
    pub(in crate::ui) fn new(view: &RadioView) -> Self {
        Self {
            shared: view.shared.clone(),
        }
    }

    pub(in crate::ui) fn open_near_you_location_preferences_for_test(&self) {
        let dialog = self
            .shared
            .add_dialog
            .borrow()
            .clone()
            .expect("Radio always owns its Add Station dialog");
        dialog.open_near_you_location_preferences_for_test(&self.shared.root);
    }

    pub(in crate::ui) fn add_dialog_is_visible_for_test(&self) -> bool {
        self.shared
            .add_dialog
            .borrow()
            .as_ref()
            .is_some_and(|dialog| dialog.is_visible_for_test())
    }

    pub(in crate::ui) fn add_dialog_needs_location_for_test(&self) -> bool {
        self.shared
            .add_dialog
            .borrow()
            .as_ref()
            .is_some_and(|dialog| dialog.needs_location_for_test())
    }

    pub(in crate::ui) fn add_dialog_is_searching_for_test(&self) -> bool {
        self.shared
            .add_dialog
            .borrow()
            .as_ref()
            .is_some_and(|dialog| dialog.is_searching_for_test())
    }
}
