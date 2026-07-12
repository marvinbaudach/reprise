//! Session-specific sidebar selection with the standard vanished-row fallback.

use std::rc::Rc;

use reprise_core::view_source::ViewSource;

use crate::ui::sidebar::{find_row, resolve_select_source, Shared};
use crate::ui::strings;

pub(super) fn restore_source(shared: &Rc<Shared>, requested: ViewSource) -> (ViewSource, String) {
    let row_exists = find_row(shared, &requested).is_some();
    let source = resolve_select_source(requested, row_exists).0;
    let entry = shared
        .rows
        .borrow()
        .iter()
        .find(|(_, candidate, _)| candidate == &source)
        .map(|(row, _, title)| (row.clone(), title.clone()));
    let Some((row, title)) = entry else {
        return (ViewSource::Library, strings::text(strings::SIDEBAR_MUSIC));
    };
    *shared.current_source.borrow_mut() = source.clone();
    shared.listbox.select_row(Some(&row));
    (source, title)
}
