//! Row-selection and activation wiring for the navigation sidebar
//! (extracted from `sidebar.rs` to keep the orchestrator under the
//! 600-line gate). See `sidebar.rs`'s module doc — especially the
//! `## Reentrancy` and `## Row identity` sections — for the contracts
//! these handlers follow.

use std::rc::Rc;

use gtk4::prelude::*;

use super::sidebar::{find_row, select_row_in_its_listbox, Shared};
use super::sidebar_playlist_creation;

/// Wires the `ListBox`'s `row-selected` signal: a navigation row becoming
/// selected updates `shared.current_source` and notifies `on_select` — but
/// only when the newly selected row's `ViewSource` actually *differs* from
/// `shared.current_source`'s current value. This is a value comparison, not
/// a time-windowed suppress flag, because `rebuild` tears down and rebuilds
/// every row on every refresh: a routine refresh's silent re-selection is
/// still selecting a brand new `ListBoxRow` GObject (row identity always
/// changes), so only comparing the *logical* source can tell "nothing
/// actually changed" apart from a real switch. `row` is `None` for a
/// deselection, including the one `rebuild`'s `remove_all` causes when it
/// clears out the previously selected row — see the module doc's
/// `## Reentrancy` section.
pub(in crate::ui) fn wire_row_selected(shared: &Rc<Shared>) {
    // Both nav lists share one selection: selecting in either clears the other.
    wire_row_selected_on(shared, &shared.listbox, &shared.issues_listbox);
    wire_row_selected_on(shared, &shared.issues_listbox, &shared.listbox);
}

/// Wires `row-selected` on one nav list. Clears `sibling`'s selection first so
/// only one row is ever visually selected across the main list and the
/// bottom-pinned issues list — `sibling`'s handler then fires with `None` and
/// returns early, so there is no source change and no loop.
///
/// A selection that arrived through KEYBOARD FOCUS (`row.has_focus()`) does
/// NOT route: GTK's `ListBox` selects a row the moment Tab/arrow focus
/// lands on it, so routing here made merely tabbing THROUGH the sidebar
/// switch the whole view (keyboard-nav optics run: Tab #2 yanked the app to
/// the Queue). Focus-driven selection is browsing; committing is a single
/// click or Enter/Space — both fire `row-activated`, where the route lives
/// alongside the programmatic (focus-less) selection path here.
fn wire_row_selected_on(shared: &Rc<Shared>, listbox: &gtk4::ListBox, sibling: &gtk4::ListBox) {
    let shared = shared.clone();
    let sibling = sibling.clone();
    listbox.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        sibling.unselect_all();
        if row.has_focus() {
            return;
        }
        route_row(&shared, row);
    });
}

/// Resolves `row` to its `ViewSource` and routes there (dedup'd against
/// `current_source`) — the shared tail of programmatic selection
/// (`row-selected` without focus) and user activation (`row-activated`).
fn route_row(shared: &Rc<Shared>, row: &gtk4::ListBoxRow) {
    let matched = shared
        .rows
        .borrow()
        .iter()
        .find(|(r, _, _)| r == row)
        .map(|(_, source, title)| (source.clone(), title.clone()));
    let Some((source, title)) = matched else {
        // Selecting the "New playlist" row (or a header) can't happen —
        // both are `selectable(false)` — so this would only fire for a
        // genuine bug in row bookkeeping; warn rather than panic.
        tracing::warn!("sidebar: selected row not found in row map; ignoring");
        return;
    };
    if *shared.current_source.borrow() == source {
        // Same logical source as before (a routine refresh's silent
        // re-select, or re-selecting the row that's already active) —
        // nothing to notify.
        return;
    }
    tracing::debug!(source = %source.label(), "sidebar: row selected");
    *shared.current_source.borrow_mut() = source.clone();
    // Hoisted clone-out before calling, per this project's `RefCell`
    // callback discipline (see the module doc's `## Reentrancy`
    // section): `on_select` can synchronously trigger a `rebuild` that
    // touches every field on `shared`, including this same `RefCell`.
    let callback = shared.on_select.borrow().clone();
    if let Some(callback) = callback {
        callback(source, title);
    }
}

/// Wires the `ListBox`'s `row-activated` signal. Every navigation row is
/// both selectable *and* activatable (GTK's default), so a click on one
/// fires this alongside `row-selected` — but `row-selected` only notifies on
/// an actual source change (see `wire_row_selected`'s dedup-by-value check),
/// so re-activating the row that's already selected (re-tapping it after
/// backing out to the sidebar in collapsed mode, or pressing Enter on it)
/// fires `row-activated` alone. Stage 3 Task 4 review finding #1: that case
/// needs to bring the content page forward too, so every navigation row
/// (found in `shared.rows`) invokes `on_show_content` here unconditionally —
/// cheap and idempotent (`window.rs`'s callback only flips `show-content`
/// when the split view is collapsed), so firing it redundantly alongside a
/// real `on_select`-driven switch is harmless. The "New playlist" row (non-
/// selectable, so it never appears in `shared.rows`) is handled separately:
/// it opens the dialog instead.
/// While keyboard focus browses the sidebar, GTK's focus-driven selection
/// wanders WITHOUT routing (see `wire_row_selected_on`). If focus then
/// leaves the lists without a commit, snap the visual selection back to the
/// source that is actually shown — otherwise a merely-browsed row stays
/// highlighted while the content shows something else.
pub(in crate::ui) fn wire_focus_leave_resync(shared: &Rc<Shared>) {
    for listbox in [&shared.listbox, &shared.issues_listbox] {
        let controller = gtk4::EventControllerFocus::new();
        let shared = shared.clone();
        controller.connect_leave(move |_| {
            let current = shared.current_source.borrow().clone();
            let Some(row) = find_row(&shared, &current) else {
                return;
            };
            if !row.is_selected() {
                // Re-selecting fires `row-selected`, whose `route_row` then
                // dedups against `current_source` — no reroute, no loop.
                select_row_in_its_listbox(&row);
            }
        });
        listbox.add_controller(controller);
    }
}

pub(in crate::ui) fn wire_row_activated(shared: &Rc<Shared>) {
    wire_row_activated_on(shared, &shared.listbox);
    wire_row_activated_on(shared, &shared.issues_listbox);
}

fn wire_row_activated_on(shared: &Rc<Shared>, listbox: &gtk4::ListBox) {
    let shared = shared.clone();
    listbox.connect_row_activated(move |_, row| {
        let is_new_playlist_row = shared.new_playlist_row.borrow().as_ref() == Some(row);
        if is_new_playlist_row {
            sidebar_playlist_creation::show_new_playlist_dialog(&shared);
            return;
        }
        let is_import_playlist_row = shared.import_playlist_row.borrow().as_ref() == Some(row);
        if is_import_playlist_row {
            let callback = shared.on_import_playlist.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
            return;
        }
        let is_nav_row = shared.rows.borrow().iter().any(|(r, _, _)| r == row);
        if is_nav_row {
            // The user COMMITTED to this row (single click, Enter, Space) —
            // route now. Focus-driven `row-selected` deliberately skipped
            // routing (see `wire_row_selected_on`); a click still navigates
            // with that one click because activation fires right after the
            // focus-selection, and `route_row`'s dedup keeps the pair from
            // routing twice.
            route_row(&shared, row);
            // Hoisted clone-out before calling, per this project's `RefCell`
            // callback discipline (same reasoning as `wire_row_selected`'s
            // `on_select` clone-out just above).
            let callback = shared.on_show_content.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        }
    });
}
