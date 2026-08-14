use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;

use super::Shared;

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the content-stack binding in Task 4")
)]
const LIBRARY_DOCTOR_PAGE: &str = "library-doctor";
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the content-stack binding in Task 4")
)]
const DEVICE_SYNC_PAGE: &str = "device-sync";

#[cfg_attr(not(test), expect(dead_code, reason = "stored by Shared in Task 2"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum SidebarPlace {
    /// A real track source is visible; `current_source` owns the marking.
    Source,
    /// Library Doctor is visible.
    LibraryDoctor,
    /// The sync page for this device is visible.
    Device(String),
    /// A placeless page without a matching sidebar entry is visible.
    Unknown,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by the content-stack binding in Task 4")
)]
pub(in crate::ui) fn place_for_content_page(
    visible_child: Option<&str>,
    open_device: Option<&str>,
) -> SidebarPlace {
    match visible_child {
        Some(LIBRARY_DOCTOR_PAGE) => SidebarPlace::LibraryDoctor,
        Some(DEVICE_SYNC_PAGE) => match open_device {
            Some(device_id) => SidebarPlace::Device(device_id.to_string()),
            None => {
                tracing::warn!("device sync page is visible without an open device id");
                SidebarPlace::Unknown
            }
        },
        _ => SidebarPlace::Source,
    }
}

/// Selects `row` in whichever of the two nav lists actually contains it (the
/// main scrolling list or the bottom-pinned issues list), so selection-follow
/// works regardless of which list a source lives in. Its `row-selected`
/// handler then clears the sibling list, keeping a single visible selection.
pub(in crate::ui) fn select_row_in_its_listbox(row: &gtk4::ListBoxRow) {
    if let Some(listbox) = row
        .parent()
        .and_then(|parent| parent.downcast::<gtk4::ListBox>().ok())
    {
        listbox.select_row(Some(row));
    }
}

/// Pure decision behind the vanished-source fallback (Stage 3 Task 4 review
/// finding #3): given the source `rebuild` would like to (re)select and
/// whether a row for it still exists, decides what to actually select.
/// Returns `(source_to_select, fell_back)`, where `fell_back` is `true` when
/// `requested` no longer has a row and `Library` was substituted instead.
/// Kept free of `Shared`/GTK so it's unit-testable without a live `ListBox`.
pub(in crate::ui) fn resolve_select_source(
    requested: ViewSource,
    row_exists: bool,
) -> (ViewSource, bool) {
    if row_exists {
        (requested, false)
    } else {
        (ViewSource::Library, true)
    }
}

/// Whether `source` is one the sidebar ever builds a row for. Album, artist,
/// and genre scopes are opened from inside the track list (metadata links,
/// the stats page) and deliberately have no row — so their absence from the
/// row set is the normal state, not a vanished row, and must never trigger
/// [`resolve_select_source`]'s Library fallback.
pub(in crate::ui) fn has_sidebar_row(source: &ViewSource) -> bool {
    !matches!(
        source,
        ViewSource::Album { .. } | ViewSource::Artist(_) | ViewSource::Genre(_)
    )
}

/// Looks up the row currently backing `source` in `shared.rows` (rebuilt on
/// every `rebuild` call, so this only ever searches the *current* row set).
pub(in crate::ui) fn find_row(
    shared: &std::rc::Rc<Shared>,
    source: &ViewSource,
) -> Option<gtk4::ListBoxRow> {
    shared
        .rows
        .borrow()
        .iter()
        .find(|(_, candidate, _)| candidate == source)
        .map(|(row, _, _)| row.clone())
}

#[cfg(test)]
#[path = "sidebar_place_tests.rs"]
mod tests;
