//! NAV-5: session-scoped view-state memory for the track table. Leaving a
//! source (sidebar switch, album/artist cross-navigation) captures scroll
//! position + selected track ids into a per-`ViewSource` map on `Shared`;
//! re-attaching the same source restores both, so the user finds the list
//! exactly as left. Deliberately in-memory only — NAV-5's precision says
//! view state must NOT persist across app restarts, so nothing here touches
//! the settings table. Save/restore happens ONLY on source switches
//! (`set_source_and_reload`), never on plain reloads (typing a filter or
//! clicking a sort header legitimately resets the viewport).

use gtk4::prelude::*;

use crate::ui::track_list::Shared;

/// Upper bound on remembered selected ids per source — a guard against a
/// pathological select-all on a 10k-track view being cloned around on every
/// source switch. Restoring the first 512 of such a selection is fine; the
/// point of NAV-5 is orientation, not perfect multi-selection fidelity.
const MAX_REMEMBERED_SELECTED_IDS: usize = 512;

/// How many idle-callback rounds the scroll restore waits for the rebuilt
/// list to gain usable geometry before giving up. Each round is one main-loop
/// iteration; a freshly repopulated `ColumnView` normally has its adjustment
/// updated after the first allocation pass.
const SCROLL_RESTORE_MAX_ATTEMPTS: u8 = 8;

/// Scroll offset + selected track ids of a track-table view, captured the
/// moment the user navigates away from a source.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::ui) struct SavedViewState {
    pub scroll_value: f64,
    pub selected_ids: Vec<i64>,
}

/// Maps remembered selected ids onto positions in the view's *current* id
/// list. Ids that no longer exist in the view (deleted, filtered away by a
/// changed smart playlist, …) are silently dropped; surviving ids map to
/// their new positions even when sorting moved them.
pub(in crate::ui) fn positions_to_select(saved: &SavedViewState, current_ids: &[i64]) -> Vec<u32> {
    let remembered: std::collections::HashSet<i64> = saved.selected_ids.iter().copied().collect();
    current_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| remembered.contains(id))
        .filter_map(|(position, _)| u32::try_from(position).ok())
        .collect()
}

/// Clamps a remembered scroll offset to the current content geometry, so a
/// view that shrank while the user was away (rescan, filter persisted on the
/// source) restores to its last valid offset instead of overshooting.
pub(in crate::ui) fn clamped_scroll(saved: f64, upper: f64, page_size: f64) -> f64 {
    if upper <= page_size {
        return 0.0;
    }
    saved.clamp(0.0, upper - page_size)
}

/// Captures the current scroll offset + selection of the track table.
pub(in crate::ui) fn capture(shared: &Shared) -> SavedViewState {
    let scroll_value = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)
        .map_or(0.0, |adjustment| adjustment.value());
    let selection = shared.selection.selection();
    let mut selected_ids = Vec::new();
    for index in 0..selection.size() {
        if selected_ids.len() >= MAX_REMEMBERED_SELECTED_IDS {
            break;
        }
        let position = selection.nth(u32::try_from(index).unwrap_or(u32::MAX));
        if let Some(track) = shared.model.track_at(position) {
            selected_ids.push(track.id);
        }
    }
    SavedViewState {
        scroll_value,
        selected_ids,
    }
}

/// Restores a captured view state after `reload` rebuilt the model for the
/// re-attached source: selection synchronously (the model rows exist), the
/// scroll offset via idle retries once the rebuilt list has geometry.
pub(in crate::ui) fn restore(shared: &Shared, saved: &SavedViewState, current_ids: &[i64]) {
    let positions = positions_to_select(saved, current_ids);
    shared.selection.unselect_all();
    for position in positions {
        shared.selection.select_item(position, false);
    }
    restore_scroll_when_ready(
        shared.column_view.clone(),
        saved.scroll_value,
        SCROLL_RESTORE_MAX_ATTEMPTS,
    );
}

/// Applies `value` to the table's vadjustment as soon as the adjustment has
/// usable geometry, retrying over at most `attempts` idle rounds. A list
/// that fits its viewport entirely (upper <= page) needs no scroll at all.
fn restore_scroll_when_ready(column_view: gtk4::ColumnView, value: f64, attempts: u8) {
    if value <= 0.0 {
        return;
    }
    gtk4::glib::idle_add_local_once(move || {
        let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&column_view) else {
            return;
        };
        let (upper, page) = (adjustment.upper(), adjustment.page_size());
        if upper > page {
            adjustment.set_value(clamped_scroll(value, upper, page));
        } else if attempts > 0 {
            restore_scroll_when_ready(column_view, value, attempts - 1);
        }
    });
}

/// The `set_source_and_reload` hook: captures `old_source`'s state before the
/// model is replaced. A same-source "switch" is a plain reload and captures
/// nothing (NAV-5 governs mode *changes* only).
pub(in crate::ui) fn remember_on_leave(
    shared: &Shared,
    old_source: &reprise_core::view_source::ViewSource,
    new_source: &reprise_core::view_source::ViewSource,
) {
    if old_source == new_source {
        return;
    }
    let state = capture(shared);
    shared
        .view_state_memory
        .borrow_mut()
        .insert(old_source.clone(), state);
}

/// The post-reload counterpart: restores the re-attached source's remembered
/// state, if any. The state stays in the map (not `remove`d) so bouncing
/// between two sources keeps restoring both ways; a later leave overwrites.
pub(in crate::ui) fn restore_on_attach(
    shared: &Shared,
    source: &reprise_core::view_source::ViewSource,
    current_ids: &[i64],
) {
    let saved = shared.view_state_memory.borrow().get(source).cloned();
    if let Some(saved) = saved {
        restore(shared, &saved, current_ids);
        tracing::debug!(
            source = %source.label(),
            scroll = saved.scroll_value,
            selected = saved.selected_ids.len(),
            "restored view state on source re-attach (NAV-5)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(ids: &[i64]) -> SavedViewState {
        SavedViewState {
            scroll_value: 0.0,
            selected_ids: ids.to_vec(),
        }
    }

    #[test]
    fn positions_to_select_maps_surviving_ids_only() {
        // 11 vanished; 9 and 7 moved — restore selects their new positions.
        assert_eq!(
            positions_to_select(&saved(&[7, 9, 11]), &[9, 42, 7]),
            vec![0, 2]
        );
    }

    #[test]
    fn positions_to_select_handles_empty_saved_and_empty_view() {
        assert_eq!(positions_to_select(&saved(&[]), &[1, 2]), Vec::<u32>::new());
        assert_eq!(positions_to_select(&saved(&[1]), &[]), Vec::<u32>::new());
    }

    #[test]
    fn clamped_scroll_clamps_to_content() {
        // Content shrank while away: 900 into a 500-tall list with a 200
        // viewport clamps to the last valid offset (300).
        assert_eq!(clamped_scroll(900.0, 500.0, 200.0), 300.0);
        assert_eq!(clamped_scroll(100.0, 500.0, 200.0), 100.0);
    }

    #[test]
    fn clamped_scroll_zero_when_list_fits() {
        assert_eq!(clamped_scroll(150.0, 100.0, 200.0), 0.0);
    }
}
