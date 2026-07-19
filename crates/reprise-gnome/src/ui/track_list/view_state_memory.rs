//! NAV-5: session-scoped view-state memory for the track table. Leaving a
//! source (sidebar switch, album/artist cross-navigation) captures scroll
//! position + selected track ids into a per-`ViewSource` map on `Shared`;
//! re-attaching the same source restores both, so the user finds the list
//! exactly as left. Deliberately in-memory only — NAV-5's precision says
//! view state must NOT persist across app restarts, so nothing here touches
//! the settings table. Save/restore happens ONLY on source switches
//! (`set_source_and_reload`), never on plain reloads (typing a filter or
//! clicking a sort header legitimately resets the viewport). Scroll is stored
//! as the stable id of the row at the viewport edge plus the offset into that
//! row, never as an absolute adjustment value.

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
    pub anchor: Option<(i64, f64)>,
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

#[cfg(test)]
fn scroll_target(
    saved: &SavedViewState,
    current_ids: &[i64],
    row_height: f64,
    viewport_height: f64,
) -> Option<f64> {
    super::reload_restore::scroll_target(saved.anchor, current_ids, row_height, viewport_height)
}

/// Captures the current scroll offset + selection of the track table.
pub(in crate::ui) fn capture(shared: &Shared) -> SavedViewState {
    let scroll_value = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)
        .map_or(0.0, |adjustment| adjustment.value());
    let total = shared.model.n_items();
    let anchor = row_height(&shared.column_view, total).and_then(|height| {
        let index = (scroll_value / height).floor().max(0.0) as u32;
        shared
            .model
            .track_at(index)
            .map(|track| (track.id, scroll_value - f64::from(index) * height))
    });
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
        anchor,
        selected_ids,
    }
}

fn row_height(column_view: &gtk4::ColumnView, n_rows: u32) -> Option<f64> {
    if n_rows == 0 {
        return None;
    }
    let adjustment = gtk4::prelude::ScrollableExt::vadjustment(column_view)?;
    let upper = adjustment.upper();
    (upper > 0.0).then(|| upper / f64::from(n_rows))
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
        saved.anchor,
        current_ids.to_vec(),
        SCROLL_RESTORE_MAX_ATTEMPTS,
    );
}

/// Applies `value` to the table's vadjustment as soon as the adjustment has
/// usable geometry, retrying over at most `attempts` idle rounds. A list
/// that fits its viewport entirely (upper <= page) needs no scroll at all.
fn restore_scroll_when_ready(
    column_view: gtk4::ColumnView,
    anchor: Option<(i64, f64)>,
    current_ids: Vec<i64>,
    attempts: u8,
) {
    if anchor.is_none() || current_ids.is_empty() {
        return;
    }
    gtk4::glib::idle_add_local_once(move || {
        let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&column_view) else {
            return;
        };
        let (upper, page) = (adjustment.upper(), adjustment.page_size());
        if upper > page {
            let height = upper / current_ids.len() as f64;
            if let Some(target) =
                super::reload_restore::scroll_target(anchor, &current_ids, height, page)
            {
                adjustment.set_value(target);
            }
        } else if attempts > 0 {
            restore_scroll_when_ready(column_view, anchor, current_ids, attempts - 1);
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
            anchor = ?saved.anchor,
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
            anchor: None,
            selected_ids: ids.to_vec(),
        }
    }

    #[test]
    fn nav_5_remembers_scroll_and_selection_per_view() {
        let library = SavedViewState {
            anchor: Some((42, 7.5)),
            selected_ids: vec![42, 99],
        };
        let playlist = SavedViewState {
            anchor: Some((7, 2.0)),
            selected_ids: vec![7],
        };
        let mut memory = std::collections::HashMap::new();
        memory.insert("tracks", library.clone());
        memory.insert("playlist", playlist.clone());

        assert_eq!(memory.get("tracks"), Some(&library));
        assert_eq!(memory.get("playlist"), Some(&playlist));
    }

    #[test]
    fn nav_5_anchor_survives_resort() {
        let state = SavedViewState {
            anchor: Some((42, 6.0)),
            selected_ids: vec![42],
        };
        let resorted = [5, 9, 11, 42, 77, 88];

        assert_eq!(scroll_target(&state, &resorted, 20.0, 40.0), Some(66.0));
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
}
