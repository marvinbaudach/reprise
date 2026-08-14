use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;

use super::Shared;

const LIBRARY_DOCTOR_PAGE: &str = "library-doctor";
const DEVICE_SYNC_PAGE: &str = "device-sync";

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

/// Reconciles both sidebar lists and the device cards with the visible place.
/// Every `RefCell` value is cloned out before GTK or callback code can re-enter.
pub(in crate::ui) fn apply_marking(shared: &std::rc::Rc<Shared>) {
    let place = shared.current_place.borrow().clone();
    match place {
        SidebarPlace::Source => apply_source_marking(shared),
        SidebarPlace::LibraryDoctor => {
            clear_list_marking(shared);
            mark_device(shared, None);
            let doctor_row = shared.doctor_row.borrow().clone();
            if let Some(row) = doctor_row {
                select_row_in_its_listbox(&row);
            }
        }
        SidebarPlace::Device(device_id) => {
            clear_list_marking(shared);
            mark_device(shared, Some(device_id.as_str()));
        }
        SidebarPlace::Unknown => {
            clear_list_marking(shared);
            mark_device(shared, None);
        }
    }
}

fn apply_source_marking(shared: &std::rc::Rc<Shared>) {
    mark_device(shared, None);
    let requested_source = shared.current_source.borrow().clone();
    if !has_sidebar_row(&requested_source) {
        clear_list_marking(shared);
        tracing::debug!(
            scope = %requested_source.label(),
            "scope view has no sidebar row; leaving the selection empty"
        );
        return;
    }
    let requested_row = find_row(shared, &requested_source);
    let (select_source, fell_back) =
        resolve_select_source(requested_source.clone(), requested_row.is_some());
    if fell_back {
        tracing::debug!(
            vanished_source = %requested_source.label(),
            "selected source vanished; falling back to Library"
        );
    }
    let row_to_select = if fell_back {
        find_row(shared, &select_source)
    } else {
        requested_row
    };
    if let Some(row) = row_to_select {
        select_row_in_its_listbox(&row);
    }
}

fn clear_list_marking(shared: &Shared) {
    shared.listbox.unselect_all();
    shared.issues_listbox.unselect_all();
}

fn mark_device(shared: &Shared, device_id: Option<&str>) {
    let callback = shared.mark_device.borrow().clone();
    if let Some(callback) = callback {
        callback(device_id);
    }
}

pub(in crate::ui) fn sync_place_from_stack(shared: &std::rc::Rc<Shared>) {
    let Some(stack) = shared.content_stack.upgrade() else {
        apply_marking(shared);
        return;
    };
    let visible_child = stack.visible_child_name();
    let open_device = shared.open_device.borrow().clone();
    let place = place_for_content_page(visible_child.as_deref(), open_device.as_deref());
    *shared.current_place.borrow_mut() = place;
    apply_marking(shared);
}

impl crate::ui::sidebar::Sidebar {
    pub(in crate::ui) fn bind_content_stack(&self, stack: &gtk4::Stack) {
        self.shared.content_stack.set(Some(stack));
        let shared = std::rc::Rc::downgrade(&self.shared);
        stack.connect_visible_child_name_notify(move |_| {
            if let Some(shared) = shared.upgrade() {
                sync_place_from_stack(&shared);
            }
        });
        sync_place_from_stack(&self.shared);
    }
}

#[cfg(test)]
#[path = "sidebar_place_tests.rs"]
mod tests;
