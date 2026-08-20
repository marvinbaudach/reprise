//! Full-cell interaction surfaces for the Releases column factories.

use gtk4::prelude::*;

pub(super) fn set_child(item: &gtk4::ListItem, child: &impl IsA<gtk4::Widget>) {
    let surface = crate::ui::source_context_surface::wrap(child);
    item.set_child(Some(&surface));
}

pub(super) fn child<T: IsA<gtk4::Widget>>(item: &gtk4::ListItem) -> Option<T> {
    item.child()?.first_child()?.downcast::<T>().ok()
}
