use gtk4::prelude::*;

/// Walk a widget's descendants and return the first `ScrolledWindow`.
pub(in crate::ui) fn find_scroller(widget: &gtk4::Widget) -> Option<gtk4::ScrolledWindow> {
    if let Ok(scroller) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
        return Some(scroller);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_scroller(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}
