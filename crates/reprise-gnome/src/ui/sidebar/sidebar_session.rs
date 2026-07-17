//! Session-specific sidebar selection with the standard vanished-row fallback.

use std::rc::Rc;

use reprise_core::view_source::ViewSource;

use crate::ui::sidebar::{find_row, resolve_select_source, select_row_in_its_listbox, Shared};
use crate::ui::strings;

pub(in crate::ui) fn restore_source(
    shared: &Rc<Shared>,
    requested: ViewSource,
) -> (ViewSource, String) {
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
    select_row_in_its_listbox(&row);
    (source, title)
}

/// Re-baselines the row-selected dedup against the view's ACTUAL source.
/// Paths that change the track list without going through the sidebar
/// (album/artist cross-navigation, smoke hooks) leave `current_source`
/// stale; NAV-9's jump and NAV-2's back call this first so their
/// `refresh_and_select` isn't swallowed as a same-source no-op.
pub(in crate::ui) fn sync_current_source(
    shared: &super::sidebar::Shared,
    source: &reprise_core::view_source::ViewSource,
) {
    *shared.current_source.borrow_mut() = source.clone();
}
