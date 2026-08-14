use std::rc::Rc;

use super::RadioView;

/// Connects Radio directly to app-wide location changes, without taking a
/// dependency on the optional Concerts runtime.
pub(super) fn subscribe(
    view: &RadioView,
    broadcast: &Rc<crate::ui::location_broadcast::LocationBroadcast>,
) {
    if let Some(dialog) = view.shared.add_dialog.borrow().as_ref() {
        dialog.subscribe_location(broadcast);
    }
}
