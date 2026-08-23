//! Full-cell interaction surfaces for the Releases column factories.

use std::rc::Rc;

use gtk4::prelude::*;

pub(super) type OnWireCell = Rc<dyn Fn(&gtk4::Box, &gtk4::ListItem)>;

pub(super) fn set_child(
    item: &gtk4::ListItem,
    child: &impl IsA<gtk4::Widget>,
    wire: &dyn Fn(&gtk4::Box, &gtk4::ListItem),
) {
    let surface = crate::ui::source_context_surface::wrap(child);
    wire(&surface, item);
    item.set_child(Some(&surface));
}

pub(super) fn child<T: IsA<gtk4::Widget>>(item: &gtk4::ListItem) -> Option<T> {
    item.child()?.first_child()?.downcast::<T>().ok()
}
